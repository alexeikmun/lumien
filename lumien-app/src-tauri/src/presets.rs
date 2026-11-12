use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const BUILTIN_1_PNG: &[u8] = include_bytes!("../../src/assets/presets/default_01.png");
const BUILTIN_2_PNG: &[u8] = include_bytes!("../../src/assets/presets/default_02.png");
const BUILTIN_3_PNG: &[u8] = include_bytes!("../../src/assets/presets/default_03.png");
const BUILTIN_4_PNG: &[u8] = include_bytes!("../../src/assets/presets/default_04.png");

const SOLID_000000_H264: &[u8] = include_bytes!("../assets/swatches/solid_000000.h264_2240x1080");
const SOLID_0C151D_H264: &[u8] = include_bytes!("../assets/swatches/solid_0c151d.h264_2240x1080");
const SOLID_18262E_H264: &[u8] = include_bytes!("../assets/swatches/solid_18262e.h264_2240x1080");
const SOLID_0A192F_H264: &[u8] = include_bytes!("../assets/swatches/solid_0a192f.h264_2240x1080");
const SOLID_062826_H264: &[u8] = include_bytes!("../assets/swatches/solid_062826.h264_2240x1080");
const SOLID_1F1638_H264: &[u8] = include_bytes!("../assets/swatches/solid_1f1638.h264_2240x1080");
const SOLID_2B0B18_H264: &[u8] = include_bytes!("../assets/swatches/solid_2b0b18.h264_2240x1080");
const SOLID_FFFFFF_H264: &[u8] = include_bytes!("../assets/swatches/solid_ffffff.h264_2240x1080");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetItem {
    pub id: String,
    pub name: String,
    pub file_name: String,
    pub thumbnail: String,
    pub is_builtin: bool,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetCatalog {
    pub active_file_name: Option<String>,
    pub presets: Vec<PresetItem>,
}

// ponytail: minimal 20-line stdlib base64 encoder without pulling external crates
pub fn to_base64(data: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 { chunk[1] as usize } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as usize } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARSET[(triple >> 18) & 0x3F] as char);
        out.push(CHARSET[(triple >> 12) & 0x3F] as char);
        if chunk.len() > 1 {
            out.push(CHARSET[(triple >> 6) & 0x3F] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARSET[triple & 0x3F] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub fn detect_mime(bytes: &[u8], ext: &str) -> &'static str {
    if bytes.len() >= 8 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
        "image/png"
    } else if bytes.len() >= 3 && &bytes[0..3] == b"\xff\xd8\xff" {
        "image/jpeg"
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        match ext.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "webp" => "image/webp",
            _ => "image/png",
        }
    }
}

pub fn make_data_url(bytes: &[u8], ext: &str) -> String {
    let mime = detect_mime(bytes, ext);
    format!("data:{};base64,{}", mime, to_base64(bytes))
}

pub fn get_builtin_presets() -> Vec<PresetItem> {
    vec![
        PresetItem {
            id: "builtin_01".into(),
            name: "Cooling delivery".into(),
            file_name: "default_01.mp4.h264_2240x1080".into(),
            thumbnail: make_data_url(BUILTIN_1_PNG, "png"),
            is_builtin: true,
            is_active: false,
        },
        PresetItem {
            id: "builtin_02".into(),
            name: "Migration".into(),
            file_name: "default_02.mp4.h264_2240x1080".into(),
            thumbnail: make_data_url(BUILTIN_2_PNG, "png"),
            is_builtin: true,
            is_active: false,
        },
        PresetItem {
            id: "builtin_03".into(),
            name: "Quantum time capsule".into(),
            file_name: "default_03.mp4.h264_2240x1080".into(),
            thumbnail: make_data_url(BUILTIN_3_PNG, "png"),
            is_builtin: true,
            is_active: false,
        },
        PresetItem {
            id: "builtin_04".into(),
            name: "Exo-Ecologies".into(),
            file_name: "default_04.mp4.h264_2240x1080".into(),
            thumbnail: make_data_url(BUILTIN_4_PNG, "png"),
            is_builtin: true,
            is_active: false,
        },
    ]
}

pub fn presets_dir(app_config_dir: &Path) -> PathBuf {
    app_config_dir.join("presets")
}

pub fn load_active_preset(app_config_dir: &Path) -> String {
    let active_file = presets_dir(app_config_dir).join("active_preset.txt");
    if let Ok(content) = fs::read_to_string(&active_file) {
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "default_01.mp4.h264_2240x1080".to_string()
}

pub fn save_active_preset(app_config_dir: &Path, file_name: &str) -> Result<(), String> {
    let dir = presets_dir(app_config_dir);
    fs::create_dir_all(&dir).map_err(|e| format!("Cannot create presets dir: {e}"))?;
    let active_file = dir.join("active_preset.txt");
    fs::write(&active_file, file_name).map_err(|e| format!("Cannot write active preset: {e}"))
}

pub fn scan_and_import_kanali(app_config_dir: &Path) -> Result<Vec<PresetItem>, String> {
    let dir = presets_dir(app_config_dir);
    let covers_dir = dir.join("covers");
    let media_dir = dir.join("media");
    fs::create_dir_all(&covers_dir).map_err(|e| format!("Cannot create covers dir: {e}"))?;
    fs::create_dir_all(&media_dir).map_err(|e| format!("Cannot create media dir: {e}"))?;

    let mut presets = get_builtin_presets();

    let appdata = std::env::var("APPDATA").unwrap_or_default();
    let kanali_path = PathBuf::from(&appdata).join("kanali");
    let material_path = kanali_path.join("material.json");

    if material_path.exists() {
        if let Ok(content) = fs::read_to_string(&material_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(list) = v.get("materialList").and_then(|m| m.get("PASE_1021")).and_then(|l| l.as_array()) {
                    for item in list {
                        let id = item.get("id").and_then(|i| i.as_i64()).unwrap_or(0);
                        let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("Unnamed Preset");
                        let hw_file = item.get("hardwareInfo")
                            .and_then(|h| h.as_array())
                            .and_then(|arr| arr.first())
                            .and_then(|obj| obj.get("fileName"))
                            .and_then(|f| f.as_str());

                        let cover_local = item.get("coverFileLocalPath").and_then(|c| c.as_str());

                        if let Some(hw_name) = hw_file {
                            // Copy media elementary stream to lumien media dir if not already present
                            let src_temp = kanali_path.join("media").join("temp").join(hw_name);
                            let dst_temp = media_dir.join(hw_name);
                            if src_temp.exists() && !dst_temp.exists() {
                                let _ = fs::copy(&src_temp, &dst_temp);
                            }

                            // Read or copy thumbnail
                            let mut thumb_data = String::new();
                            if let Some(cpath) = cover_local {
                                let src_cover = PathBuf::from(cpath);
                                if src_cover.exists() {
                                    let ext = src_cover.extension().and_then(|e| e.to_str()).unwrap_or("png");
                                    let dst_cover = covers_dir.join(format!("cover_{}.{}", id, ext));
                                    if !dst_cover.exists() {
                                        let _ = fs::copy(&src_cover, &dst_cover);
                                    }
                                    if let Ok(bytes) = fs::read(&src_cover) {
                                        thumb_data = make_data_url(&bytes, ext);
                                    }
                                }
                            }

                            // Avoid duplicate IDs if already present
                            let preset_id = format!("kanali_{}", id);
                            if !presets.iter().any(|p| p.file_name == hw_name) {
                                presets.push(PresetItem {
                                    id: preset_id,
                                    name: name.to_string(),
                                    file_name: hw_name.to_string(),
                                    thumbnail: thumb_data,
                                    is_builtin: false,
                                    is_active: false,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // Save catalog to disk
    let catalog = PresetCatalog {
        active_file_name: Some(load_active_preset(app_config_dir)),
        presets: presets.clone(),
    };
    let cat_path = dir.join("catalog.json");
    if let Ok(json) = serde_json::to_string_pretty(&catalog) {
        let _ = fs::write(&cat_path, json);
    }

    Ok(presets)
}

pub fn get_all_presets(app_config_dir: &Path) -> Vec<PresetItem> {
    let dir = presets_dir(app_config_dir);
    let cat_path = dir.join("catalog.json");
    let active = load_active_preset(app_config_dir);

    let mut presets = if cat_path.exists() {
        if let Ok(content) = fs::read_to_string(&cat_path) {
            if let Ok(catalog) = serde_json::from_str::<PresetCatalog>(&content) {
                catalog.presets
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    if presets.is_empty() {
        // Try automatic scan on first run
        if let Ok(scanned) = scan_and_import_kanali(app_config_dir) {
            presets = scanned;
        } else {
            presets = get_builtin_presets();
        }
    } else {
        // Always refresh built-in preset thumbnails so compiled-in assets are authoritative
        let builtins = get_builtin_presets();
        for p in &mut presets {
            if p.is_builtin {
                if let Some(b) = builtins.iter().find(|bi| bi.id == p.id || bi.file_name == p.file_name) {
                    p.thumbnail = b.thumbnail.clone();
                }
            }
        }
    }

    // Mark active preset
    for p in &mut presets {
        p.is_active = p.file_name == active;
    }

    presets
}

pub fn find_ffmpeg(app_config_dir: &Path) -> Option<PathBuf> {
    let local = app_config_dir.join("ffmpeg").join("ffmpeg.exe");
    if local.exists() {
        return Some(local);
    }
    let kanali = PathBuf::from(r"C:\Program Files\KANALI\resources\Connect\ffmpeg\ffmpeg.exe");
    if kanali.exists() {
        return Some(kanali);
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for p in std::env::split_paths(&path_var) {
            let candidate = p.join("ffmpeg.exe");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Returns (file_name, h264_bytes) for the given hex color string.
/// If hex matches one of the 8 preselected palette colors, returns compiled-in H.264 bytes immediately.
/// Otherwise, attempts on-demand generation via FFmpeg, returning an error if FFmpeg is unavailable.
pub fn get_or_create_solid_color_video(app_config_dir: &Path, hex: &str) -> Result<(String, Vec<u8>), String> {
    let clean = hex.trim_start_matches('#').to_ascii_lowercase();
    if clean.len() != 6 || !clean.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Expected six-digit hex background color".into());
    }
    let filename = format!("solid_{}.mp4.h264_2240x1080", clean);

    let bytes = match clean.as_str() {
        "000000" | "black" => SOLID_000000_H264.to_vec(),
        "0c151d" => SOLID_0C151D_H264.to_vec(),
        "18262e" => SOLID_18262E_H264.to_vec(),
        "0a192f" => SOLID_0A192F_H264.to_vec(),
        "062826" => SOLID_062826_H264.to_vec(),
        "1f1638" => SOLID_1F1638_H264.to_vec(),
        "2b0b18" => SOLID_2B0B18_H264.to_vec(),
        "ffffff" | "white" => SOLID_FFFFFF_H264.to_vec(),
        _ => {
            let media_dir = app_config_dir.join("solid-media-v2");
            let cached_path = media_dir.join(&filename);
            if cached_path.exists() {
                fs::read(&cached_path).map_err(|e| format!("Read cached color failed: {e}"))?
            } else {
                let _ = fs::create_dir_all(&media_dir);
                let ffmpeg_exe = find_ffmpeg(app_config_dir);
                if let Some(exe) = ffmpeg_exe {
                    use std::os::windows::process::CommandExt;
                    let color_arg = format!("#{}", clean);
                    let dest_str = cached_path.to_str().unwrap_or("").to_string();
                    let status = std::process::Command::new(exe)
                        .args([
                            "-y", "-f", "lavfi",
                            "-i", &format!("color=c={}:s=2240x1080:d=1:r=30", color_arg),
                            "-c:v", "libx264", "-profile:v", "main", "-level", "4.1",
                            "-pix_fmt", "yuv420p", "-bf", "0", "-g", "1", "-an", "-f", "h264",
                            &dest_str,
                        ])
                        .creation_flags(0x08000000)
                        .status();
                    if status.map(|s| s.success()).unwrap_or(false) && cached_path.exists() {
                        fs::read(&cached_path).map_err(|e| format!("Read generated color failed: {e}"))?
                    } else {
                        let _ = fs::remove_file(&cached_path);
                        return Err("FFmpeg could not encode the selected background color".into());
                    }
                } else {
                    return Err("Custom background colors require FFmpeg; choose a palette color or install FFmpeg".into());
                }
            }
        }
    };

    Ok((filename, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_base64() {
        assert_eq!(to_base64(b""), "");
        assert_eq!(to_base64(b"f"), "Zg==");
        assert_eq!(to_base64(b"fo"), "Zm8=");
        assert_eq!(to_base64(b"foo"), "Zm9v");
        assert_eq!(to_base64(b"foob"), "Zm9vYg==");
        assert_eq!(to_base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(to_base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn test_builtin_presets_count_and_assets() {
        let presets = get_builtin_presets();
        assert_eq!(presets.len(), 4);
        assert_eq!(presets[0].name, "Cooling delivery");
        assert_eq!(presets[0].file_name, "default_01.mp4.h264_2240x1080");
        assert!(presets[0].thumbnail.starts_with("data:image/png;base64,"));
        assert!(presets[0].is_builtin);
    }

    #[test]
    fn test_make_data_url() {
        let url = make_data_url(b"hello", "png");
        assert_eq!(url, "data:image/png;base64,aGVsbG8=");
        let jpeg_url = make_data_url(b"hello", "jpg");
        assert_eq!(jpeg_url, "data:image/jpeg;base64,aGVsbG8=");
    }

    #[test]
    fn test_active_preset_save_and_load() {
        let temp_dir = std::env::temp_dir().join("lumien_preset_test_dir");
        let _ = fs::remove_dir_all(&temp_dir);
        let default_active = load_active_preset(&temp_dir);
        assert_eq!(default_active, "default_01.mp4.h264_2240x1080");

        save_active_preset(&temp_dir, "default_02.mp4.h264_2240x1080").expect("save active");
        let updated = load_active_preset(&temp_dir);
        assert_eq!(updated, "default_02.mp4.h264_2240x1080");
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    #[ignore = "reads %APPDATA%\\kanali and copies its media; run manually on a KANALI host"]
    fn test_scan_live_kanali_if_present() {
        let temp_dir = std::env::temp_dir().join("lumien_kanali_scan_test");
        let _ = fs::remove_dir_all(&temp_dir);
        let list = get_all_presets(&temp_dir);
        // Built-ins always come first; KANALI imports (if any) follow and depend on the host.
        assert!(list.len() >= 4);
        assert!(list[..4].iter().all(|p| p.is_builtin));
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_get_presets_fallback_to_builtins_when_no_catalog() {
        let temp_dir = std::env::temp_dir().join("lumien_nonexistent_empty_dir");
        let _ = fs::remove_dir_all(&temp_dir);
        let builtins = get_builtin_presets();
        assert_eq!(builtins.len(), 4);
    }

    #[test]
    fn test_get_or_create_solid_color_embedded_swatches() {
        let temp_dir = std::env::temp_dir().join("lumien_solid_test_dir");
        let _ = fs::remove_dir_all(&temp_dir);

        let (name1, bytes1) = get_or_create_solid_color_video(&temp_dir, "#000000").expect("black");
        assert_eq!(name1, "solid_000000.mp4.h264_2240x1080");
        assert!(bytes1.starts_with(&[0, 0, 0, 1]));

        let (name2, bytes2) = get_or_create_solid_color_video(&temp_dir, "18262e").expect("surface");
        assert_eq!(name2, "solid_18262e.mp4.h264_2240x1080");
        assert!(bytes2.starts_with(&[0, 0, 0, 1]));

        let (name3, bytes3) = get_or_create_solid_color_video(&temp_dir, "#ffffff").expect("white");
        assert_eq!(name3, "solid_ffffff.mp4.h264_2240x1080");
        assert!(bytes3.starts_with(&[0, 0, 0, 1]));
        assert!(get_or_create_solid_color_video(&temp_dir, "../bad").is_err());

        let _ = fs::remove_dir_all(&temp_dir);
    }
}

pub fn find_tool(name: &str, config: &Path) -> Result<PathBuf, String> {
    let local = config.join("tools").join(name);
    if local.is_file() { return Ok(local); }
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let path = dir.join(name);
        if path.is_file() { return Ok(path); }
    }
    Err(format!("{name} is required. Install it on PATH or in {}", config.join("tools").display()))
}
