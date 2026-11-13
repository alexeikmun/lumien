use serde::Serialize;
use std::{collections::HashSet, fs, path::{Path, PathBuf}, process::{Command, Stdio}};
use std::os::windows::process::CommandExt;

#[derive(Serialize)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub duration: f64,
    pub bytes: u64,
    pub thumbnail: String,
}

pub fn resolve(root: &Path, id: &str) -> Result<PathBuf, String> {
    let parts: Vec<_> = id.split('/').collect();
    if parts.len() != 2 || !parts[0].starts_with("youtube-")
        || !parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)))
        || parts.iter().any(|p| *p == "." || *p == "..")
        || !parts[1].ends_with(".h264_2240x1080") {
        return Err("Invalid downloaded video selection".into());
    }
    let root = root.canonicalize().map_err(|_| "Downloaded video library is unavailable")?;
    let path = root.join(parts[0]).join(parts[1]).canonicalize().map_err(|_| "Downloaded video was removed")?;
    if !path.starts_with(&root) || !path.is_file() { return Err("Video is outside the download library".into()); }
    Ok(path)
}

pub fn is_video_filename(name: &str) -> bool {
    name.strip_prefix("youtube_")
        .and_then(|name| name.strip_suffix(".mp4.h264_2240x1080"))
        .is_some_and(|id| id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)))
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
        if dir.parent() != Some(root.as_path()) || !dir.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("youtube-")) { continue; }
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
        if !job_name.starts_with("youtube-") { continue; }
        let Ok(files) = fs::read_dir(job.path()) else { continue };
        for file in files.flatten() {
            let id = format!("{job_name}/{}", file.file_name().to_string_lossy());
            let Ok(path) = resolve(root, &id) else { continue };
            let Some(video_id) = path.file_name().and_then(|f| f.to_str())
                .and_then(|f| f.strip_prefix("youtube_")).and_then(|f| f.split(".mp4.").next()) else { continue };
            if !seen.insert(video_id.to_owned()) { continue; }
            let bytes = fs::metadata(&path).map_err(|e| e.to_string())?.len();
            if bytes == 0 || bytes > 128 * 1024 * 1024 { continue; }
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
            videos.push(Video { id, title: info["title"].as_str().unwrap_or("Downloaded video").into(),
                duration: info["duration"].as_f64().unwrap_or(0.0), bytes, thumbnail });
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
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("video.h264_2240x1080"), [0,0,0,1]).unwrap();
        fs::write(dir.join("metadata.json"), br#"{"title":"Saved clip","duration":66}"#).unwrap();
        fs::write(dir.join("unfinished.partial"), [1]).unwrap();
        let videos = list(&root, None).unwrap();
        assert_eq!(videos.len(), 1);
        assert_eq!(videos[0].title, "Saved clip");
        assert_eq!(videos[0].duration, 66.0);
        assert!(resolve(&root, &videos[0].id).is_ok());
        for id in ["../secret", "youtube-123/../../secret", "C:/secret", "youtube-123/missing.h264_2240x1080"] {
            assert!(resolve(&root, id).is_err());
        }
        for file in ["video.h264_2240x1080", "metadata.json", "unfinished.partial"] { fs::remove_file(dir.join(file)).unwrap(); }
        fs::remove_dir(dir).unwrap(); fs::remove_dir(root).unwrap();
    }
}
