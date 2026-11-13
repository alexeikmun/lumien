use serde::Serialize;
use std::{collections::HashSet, fs, path::{Path, PathBuf}, process::{Command, Stdio}, time::{SystemTime, UNIX_EPOCH}};
use std::os::windows::process::CommandExt;

#[derive(Serialize)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub duration: Option<f64>,
    pub bytes: u64,
    pub thumbnail: String,
}

fn is_job(name: &str) -> bool {
    name.starts_with("youtube-") || name.starts_with("local-")
}

pub fn import_mp4(root: &Path, source: &Path, ffmpeg: &Path) -> Result<(), String> {
    if !source.is_file() || !source.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("mp4")) {
        return Err("Choose an MP4 video file".into());
    }
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let dir = root.join(format!("local-{stamp}"));
    fs::create_dir(&dir).map_err(|e| e.to_string())?;
    let output = dir.join(format!("local_{stamp}.mp4.h264_2240x1080"));
    let metadata = dir.join("metadata.json");
    let title = source.file_stem().map(|name| name.to_string_lossy()).unwrap_or_default();
    let result = fs::write(&metadata, serde_json::json!({ "title": title }).to_string())
        .map_err(|e| e.to_string())
        .and_then(|_| crate::youtube::encode_for_lcd(source, &output, ffmpeg, true));
    if result.is_err() {
        let _ = fs::remove_file(output.with_extension("partial"));
        let _ = fs::remove_file(metadata);
        let _ = fs::remove_file(dir.join("process-error.txt"));
        let _ = fs::remove_dir(dir);
    }
    result
}

pub fn resolve(root: &Path, id: &str) -> Result<PathBuf, String> {
    let parts: Vec<_> = id.split('/').collect();
    if parts.len() != 2 || !is_job(parts[0])
        || !parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)))
        || parts.iter().any(|p| *p == "." || *p == "..")
        || !is_video_filename(parts[1]) {
        return Err("Invalid downloaded video selection".into());
    }
    let root = root.canonicalize().map_err(|_| "Downloaded video library is unavailable")?;
    let path = root.join(parts[0]).join(parts[1]).canonicalize().map_err(|_| "Downloaded video was removed")?;
    if !path.starts_with(&root) || !path.is_file() { return Err("Video is outside the download library".into()); }
    Ok(path)
}

pub fn is_video_filename(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".mp4.h264_2240x1080") else { return false };
    if let Some(id) = stem.strip_prefix("youtube_") {
        return id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b));
    }
    stem.strip_prefix("local_")
        .is_some_and(|id| !id.is_empty() && id.len() <= 39 && id.bytes().all(|b| b.is_ascii_digit()))
}

pub fn resolve_filename(root: &Path, filename: &str) -> Result<PathBuf, String> {
    if !is_video_filename(filename) {
        return Err("Invalid downloaded video filename".into());
    }
    let jobs = fs::read_dir(root).map_err(|_| "Downloaded video library is unavailable")?;
    jobs.flatten()
        .filter_map(|job| job.file_name().into_string().ok())
        .filter_map(|job| resolve(root, &format!("{job}/{filename}")).ok())
        .filter(|path| path.file_name().and_then(|name| name.to_str()) == Some(filename))
        .max()
        .ok_or_else(|| "Downloaded background video is missing".into())
}

pub fn remove(root: &Path, id: &str) -> Result<(), String> {
    let selected = resolve(root, id)?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let filename = selected.file_name().ok_or("Invalid video filename")?;
    let mut removed = false;
    for entry in fs::read_dir(&root).map_err(|e| e.to_string())?.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_dir()) { continue; }
        let dir = entry.path().canonicalize().map_err(|e| e.to_string())?;
        if dir.parent() != Some(root.as_path()) || !dir.file_name().and_then(|n| n.to_str()).is_some_and(is_job) { continue; }
        let contains_video = fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten()
            .any(|file| file.file_name() == filename);
        if contains_video {
            fs::remove_dir_all(&dir).map_err(|e| format!("Could not remove saved video: {e}"))?;
            removed = true;
        }
    }
    if removed { Ok(()) } else { Err("Downloaded video was already removed".into()) }
}

pub fn list(root: &Path, ffmpeg: Option<&Path>) -> Result<Vec<Video>, String> {
    if !root.exists() { return Ok(Vec::new()); }
    let mut jobs: Vec<_> = fs::read_dir(root).map_err(|e| e.to_string())?.flatten().collect();
    jobs.sort_by_key(|job| std::cmp::Reverse(job.file_name()));
    let mut videos = Vec::new();
    let mut seen = HashSet::new();
    for job in jobs {
        let job_name = job.file_name().to_string_lossy().into_owned();
        if !is_job(&job_name) { continue; }
        let Ok(files) = fs::read_dir(job.path()) else { continue };
        for file in files.flatten() {
            let id = format!("{job_name}/{}", file.file_name().to_string_lossy());
            let Ok(path) = resolve(root, &id) else { continue };
            let bytes = fs::metadata(&path).map_err(|e| e.to_string())?.len();
            if bytes == 0 || bytes > 128 * 1024 * 1024 { continue; }
            if !seen.insert(file.file_name()) { continue; }
            let dir = path.parent().ok_or("Invalid video directory")?;
            let info: serde_json::Value = fs::read(dir.join("metadata.json")).ok()
                .and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            let thumb = path.with_file_name(format!("{}.thumb.jpg", path.file_stem().unwrap_or_default().to_string_lossy()));
            if !thumb.exists() {
                if let Some(ffmpeg) = ffmpeg {
                    let result = Command::new(ffmpeg).args(["-v", "error", "-y", "-i"]).arg(&path)
                        .args(["-frames:v", "1", "-vf", "thumbnail=100,scale=320:-2", "-update", "1"]).arg(&thumb)
                        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).creation_flags(0x08000000).status();
                    if !result.is_ok_and(|s| s.success()) { let _ = fs::remove_file(&thumb); }
                }
            }
            let thumbnail = fs::read(&thumb).ok().filter(|b| b.len() <= 1024 * 1024)
                .map(|b| crate::presets::make_data_url(&b, "jpg")).unwrap_or_default();
            videos.push(Video { id, title: info["title"].as_str().unwrap_or("Video").into(),
                duration: info["duration"].as_f64(), bytes, thumbnail });
        }
    }
    videos.sort_by(|a,b| b.id.cmp(&a.id));
    Ok(videos)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovers_existing_downloads_and_rejects_traversal() {
        let root = std::env::temp_dir().join(format!("lumien-gallery-{}", std::process::id()));
        let dir = root.join("youtube-123");
        let youtube_name = "youtube_abcdefghijk.mp4.h264_2240x1080";
        let local_dir = root.join("local-456");
        let local_name = "local_456.mp4.h264_2240x1080";
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(&local_dir).unwrap();
        fs::write(dir.join(youtube_name), [0,0,0,1]).unwrap();
        fs::write(local_dir.join(local_name), [0,0,0,1]).unwrap();
        fs::write(local_dir.join("metadata.json"), br#"{"title":"My MP4"}"#).unwrap();
        fs::write(dir.join("metadata.json"), br#"{"title":"Saved clip","duration":66}"#).unwrap();
        fs::write(dir.join("unfinished.partial"), [1]).unwrap();
        let videos = list(&root, None).unwrap();
        assert_eq!(videos.len(), 2);
        assert_eq!(videos[0].title, "Saved clip");
        assert_eq!(videos[0].duration, Some(66.0));
        assert!(resolve(&root, &videos[0].id).is_ok());
        assert_eq!(videos[1].title, "My MP4");
        assert!(videos[1].duration.is_none());
        assert_eq!(resolve_filename(&root, local_name).unwrap(), resolve(&root, &videos[1].id).unwrap());
        assert!(remove(&root, &videos[1].id).is_ok());
        assert!(resolve_filename(&root, local_name).is_err());
        for id in ["../secret", "youtube-123/../../secret", "C:/secret", "youtube-123/missing.h264_2240x1080"] {
            assert!(resolve(&root, id).is_err());
        }
        for file in [youtube_name, "metadata.json", "unfinished.partial"] { fs::remove_file(dir.join(file)).unwrap(); }
        fs::remove_dir(dir).unwrap(); fs::remove_dir(root).unwrap();
    }
}
