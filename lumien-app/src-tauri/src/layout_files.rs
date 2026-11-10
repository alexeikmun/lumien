use serde::Serialize;
use std::{collections::HashSet, fs, io::{Read, Write}, path::{Path, PathBuf}, sync::Mutex};
use tauri_plugin_dialog::DialogExt;
use tauri::Manager;

const MAX_BYTES: usize = 15_000_000;
#[derive(Default)]
pub struct LayoutFiles(Mutex<HashSet<PathBuf>>);

#[derive(Serialize)]
pub struct OpenedLayout { path: String, content: String }

fn read_layout(path: &Path) -> Result<OpenedLayout, String> {
    let mut content = String::new();
    fs::File::open(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?.take((MAX_BYTES + 1) as u64)
        .read_to_string(&mut content).map_err(|e| format!("Cannot read layout: {e}"))?;
    validate(content.trim_start_matches('\u{feff}'))?;
    Ok(OpenedLayout { path: path.to_string_lossy().into_owned(), content })
}

fn read_recent(config_path: &Path) -> Result<Option<OpenedLayout>, String> {
    let file = match fs::File::open(config_path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Cannot read recent layout: {e}")),
    };
    let mut recent = String::new();
    file.take(16_385).read_to_string(&mut recent).map_err(|e| e.to_string())?;
    if recent.len() > 16_384 { return Err("Recent layout path is too long".into()); }
    let path: PathBuf = serde_json::from_str(&recent).map_err(|e| format!("Invalid recent layout path: {e}"))?;
    read_layout(&path).map(Some)
}

#[tauri::command]
pub fn restore_layout(app: tauri::AppHandle, files: tauri::State<'_, LayoutFiles>) -> Result<Option<OpenedLayout>, String> {
    let config = app.path().app_config_dir().map_err(|e| e.to_string())?.join("recent-layout.json");
    let layout = read_recent(&config)?;
    if let Some(layout) = &layout {
        files.0.lock().map_err(|_| "Layout file lock failed")?.insert(PathBuf::from(&layout.path));
    }
    Ok(layout)
}

#[tauri::command]
pub fn remember_layout(app: tauri::AppHandle, files: tauri::State<'_, LayoutFiles>, path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    if !files.0.lock().map_err(|_| "Layout file lock failed")?.contains(&path) {
        return Err("Only a saved or opened layout can be remembered".into());
    }
    let directory = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&directory).map_err(|e| format!("Cannot create layout settings: {e}"))?;
    atomic_write(&directory.join("recent-layout.json"), &serde_json::to_string(&path).map_err(|e| e.to_string())?)
}

fn validate(content: &str) -> Result<serde_json::Value, String> {
    if content.len() > MAX_BYTES { return Err("Layout exceeds 15 MB".into()); }
    let value: serde_json::Value = serde_json::from_str(content).map_err(|e| format!("Invalid layout JSON: {e}"))?;
    if value["version"] != 2 || value["width"] != 2240 || value["height"] != 1080
        || !value["name"].is_string() || !value["widgets"].is_array()
        || value["widgets"].as_array().unwrap().len() > 100 {
        return Err("Unsupported layout document".into());
    }
    Ok(value)
}

fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temporary = path.with_file_name(format!(".lumien-{}-{}.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(|e| format!("Cannot create layout file: {e}"))?;
    let result = (|| {
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() { let _ = fs::remove_file(&temporary); }
    result.map_err(|e| format!("Could not save layout; previous file preserved: {e}"))
}

#[tauri::command]
pub async fn open_layout(window: tauri::WebviewWindow, files: tauri::State<'_, LayoutFiles>) -> Result<Option<OpenedLayout>, String> {
    let selected = tauri::async_runtime::spawn_blocking(move || window.dialog().file()
        .set_parent(&window).set_title("Open Lumien layout").add_filter("Lumien layout", &["json"]).blocking_pick_file())
        .await.map_err(|e| e.to_string())?;
    let Some(selected) = selected else { return Ok(None) };
    let path = selected.into_path().map_err(|e| e.to_string())?;
    let layout = read_layout(&path)?;
    files.0.lock().map_err(|_| "Layout file lock failed")?.insert(path.clone());
    Ok(Some(layout))
}

#[tauri::command]
pub async fn save_layout(window: tauri::WebviewWindow, files: tauri::State<'_, LayoutFiles>, content: String, path: Option<String>) -> Result<Option<String>, String> {
    let value = validate(&content)?;
    let path = if let Some(path) = path {
        let path = PathBuf::from(path);
        if !files.0.lock().map_err(|_| "Layout file lock failed")?.contains(&path) {
            return Err("Choose a layout using Open or Save before overwriting it".into());
        }
        path
    } else {
        let name = value["name"].as_str().unwrap().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect::<String>();
        let selected = tauri::async_runtime::spawn_blocking(move || window.dialog().file()
            .set_parent(&window).set_title("Save Lumien layout").add_filter("Lumien layout", &["json"])
            .set_file_name(format!("{name}.lumien.json")).blocking_save_file())
            .await.map_err(|e| e.to_string())?;
        let Some(selected) = selected else { return Ok(None) };
        let mut path = selected.into_path().map_err(|e| e.to_string())?;
        if path.extension().is_none() { path.set_extension("lumien.json"); }
        path
    };
    atomic_write(&path, &serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?)?;
    files.0.lock().map_err(|_| "Layout file lock failed")?.insert(path.clone());
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overwrite_is_complete_and_failure_preserves_old_file() {
        let root = std::env::temp_dir().join(format!("lumien-layout-files-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("layout.json");
        fs::write(&path, "original").unwrap();
        atomic_write(&path, "replacement").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "replacement");
        assert!(atomic_write(&root, "cannot replace directory").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "replacement");
        assert!(validate("{bad").is_err());
        assert!(validate(r#"{"version":1,"width":2240,"height":1080,"name":"old","widgets":[]}"#).is_err());
        fs::remove_file(path).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn recent_layout_reads_current_file_and_reports_missing_or_invalid_files() {
        let root = std::env::temp_dir().join(format!("lumien-recent-layout-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let config = root.join("recent-layout.json");
        let layout = root.join("dashboard.json");
        assert!(read_recent(&config).unwrap().is_none());
        let content = r#"{"version":2,"width":2240,"height":1080,"name":"Remember me","widgets":[]}"#;
        fs::write(&layout, content).unwrap();
        atomic_write(&config, &serde_json::to_string(&layout).unwrap()).unwrap();
        let restored = read_recent(&config).unwrap().unwrap();
        assert_eq!(restored.path, layout.to_string_lossy());
        assert_eq!(restored.content, content);
        fs::write(&layout, "invalid").unwrap();
        assert!(read_recent(&config).is_err());
        fs::remove_file(&layout).unwrap();
        assert!(read_recent(&config).is_err());
        fs::remove_file(config).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
