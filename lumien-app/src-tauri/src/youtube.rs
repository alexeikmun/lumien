//! YouTube is downloaded and transcoded locally; the LCD plays an elementary stream.
use std::{fs, path::{Path, PathBuf}, process::{Command, Stdio}, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use std::os::windows::process::CommandExt;

pub fn video_id(link: &str) -> Result<String, String> {
    let url = link.trim().strip_prefix("https://").ok_or("Use an HTTPS YouTube video link")?;
    let (host, rest) = url.split_once('/').ok_or("Missing video ID")?;
    let id = match host {
        "youtu.be" => rest.split(['?', '#']).next(),
        "youtube.com" | "www.youtube.com" | "m.youtube.com" => {
            if let Some(query) = rest.strip_prefix("watch?") {
                query.split('#').next().unwrap_or("").split('&').find_map(|p| p.strip_prefix("v="))
            } else { rest.strip_prefix("shorts/").or_else(|| rest.strip_prefix("embed/")).map(|s| s.split(['?', '#']).next().unwrap_or("")) }
        },
        _ => None,
    }.ok_or("Provide a YouTube video link, not a playlist or channel")?;
    if id.len() != 11 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err("Invalid YouTube video ID".into());
    }
    Ok(id.into())
}


fn run(command: &mut Command, dir: &Path, stage: &str) -> Result<(), String> {
    let error_path = dir.join("process-error.txt");
    let error = fs::File::create(&error_path).map_err(|e| e.to_string())?;
    let mut child = command.stdin(Stdio::null()).stderr(error).creation_flags(0x08000000)
        .spawn().map_err(|e| format!("{stage}: {e}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) if status.success() => return Ok(()),
            Some(_) => {
                let detail = fs::read_to_string(&error_path).unwrap_or_default();
                return Err(format!("{stage} failed: {}", detail.chars().rev().take(1500).collect::<String>().chars().rev().collect::<String>()));
            }
            None if start.elapsed() > Duration::from_secs(600) => {
                let _ = child.kill(); let _ = child.wait();
                return Err(format!("{stage} timed out after ten minutes"));
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

pub fn encode_for_lcd(source: &Path, output: &Path, ffmpeg: &Path, mp4: bool) -> Result<(), String> {
    let dir = output.parent().ok_or("Missing video output directory")?;
    let partial = output.with_extension("partial");
    let mut command = Command::new(ffmpeg);
    command.args(["-v", "error", "-y", "-protocol_whitelist", "file"]);
    if mp4 {
        command.args(["-f", "mov"]);
    }
    // The bitrate and duration cap keep uploads under 128 MiB.
    command.arg("-i").arg(source)
        .args(["-map", "0:V:0", "-an", "-vf", "scale=2240:1080:force_original_aspect_ratio=increase,crop=2240:1080,setsar=1",
            "-r", "30", "-t", "300", "-c:v", "libx264", "-profile:v", "main", "-level:v", "4.1",
            "-pix_fmt", "yuv420p", "-preset", "fast", "-b:v", "3M", "-maxrate", "3400k", "-bufsize", "6800k",
            "-bf", "0", "-g", "1", "-f", "h264"])
        .arg(&partial).stdout(Stdio::null());
    run(&mut command, dir, "Converting for LCD")?;
    let size = fs::metadata(&partial).map_err(|e| e.to_string())?.len();
    if size == 0 || size > 128 * 1024 * 1024 {
        return Err("Converted video must be nonempty and at most 128 MiB".into());
    }
    fs::rename(&partial, output).map_err(|e| e.to_string())
}

pub fn prepare(link: &str, cache: &Path, downloader: &Path, ffmpeg: &Path) -> Result<PathBuf, String> {
    let id = video_id(link)?;
    let url = format!("https://www.youtube.com/watch?v={id}");
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let dir = cache.join(format!("youtube-{stamp}"));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let result = (|| {
        let common = ["--ignore-config", "--no-playlist", "--no-progress", "--socket-timeout", "20", "--retries", "2", "--js-runtimes", "node"];
        let metadata = dir.join("metadata.json");
        run(Command::new(downloader).args(common).args(["--skip-download", "--dump-single-json", "--", &url])
            .stdout(fs::File::create(&metadata).map_err(|e| e.to_string())?), &dir, "Checking YouTube video")?;
        let info: serde_json::Value = serde_json::from_slice(&fs::read(metadata).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let duration = info["duration"].as_f64().ok_or("Video has no finite duration")?;
        if info["is_live"].as_bool() == Some(true) || info["live_status"] == "is_upcoming" || !(duration > 0.0 && duration <= 300.0) {
            return Err("Choose a recorded video up to five minutes; live streams are unsupported".into());
        }
        let source = dir.join("source.video");
        run(Command::new(downloader).args(common).args(["-f", "bestvideo[height<=1080]/best[height<=1080]", "--max-filesize", "128M", "--no-part", "-o"])
            .arg(&source).args(["--", &url]).stdout(Stdio::null()), &dir, "Downloading YouTube video")?;
        let size = fs::metadata(&source).map_err(|_| "Download missing or exceeds 128 MB")?.len();
        if size == 0 || size > 128 * 1024 * 1024 { return Err("Downloaded video exceeds 128 MB".into()); }
        let output = dir.join(format!("youtube_{id}.mp4.h264_2240x1080"));
        encode_for_lcd(&source, &output, ffmpeg, false)?;
        let _ = fs::remove_file(source);
        Ok(output)
    })();
    // Only this newly created job directory is removed on failure.
    if result.is_err() { let _ = fs::remove_dir_all(&dir); }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_canonical_youtube_video_ids_are_accepted() {
        for url in ["https://youtu.be/abcdefghijk?t=2", "https://www.youtube.com/watch?v=abcdefghijk&list=other", "https://youtube.com/shorts/abcdefghijk"] {
            assert_eq!(video_id(url).unwrap(), "abcdefghijk");
        }
        for url in ["file:///secret", "https://youtube.com.evil/watch?v=abcdefghijk", "https://youtube.com@evil/watch?v=abcdefghijk", "https://youtube.com/playlist?list=abc", "https://youtu.be/../file", "https://youtu.be/short"] {
            assert!(video_id(url).is_err(), "{url}");
        }
    }
}

pub fn read_prepared(path: &Path) -> Result<(&str, Vec<u8>), String> {
    let filename = path.file_name().and_then(|s| s.to_str()).ok_or("Invalid media filename")?;
    if !filename.ends_with(".h264_2240x1080") || fs::metadata(path).map_err(|e| e.to_string())?.len() > 128 * 1024 * 1024 {
        return Err("Expected a prepared H.264 stream under 128 MB".into());
    }
    let data = fs::read(path).map_err(|e| e.to_string())?;
    if data.is_empty() { return Err("Empty video".into()); }
    Ok((filename, data))
}

pub fn play_prepared(
    dev: &mut lumien_device::DisplaySession,
    filename: &str,
    data: &[u8],
    clear_overlay: bool,
) -> Result<(), String> {
    dev.transmit_file(filename, data, Duration::from_secs(10)).map_err(|e| format!("Video upload: {e:?}"))?;
    let clear = clear_overlay.then(lumien_protocol::encode_clear_run_config_frame);
    for frame in clear.into_iter().chain([lumien_protocol::encode_work_config_frame(filename)]) {
        dev.write_frame(&frame, Duration::from_secs(5)).map_err(|e| format!("Playback write: {e:?}"))?;
        let payload = dev.read_frame(Duration::from_secs(5)).map_err(|e| format!("Playback response: {e:?}"))?;
        let ack = lumien_protocol::parse_response_frame(&lumien_protocol::encode_frame(&payload)).map_err(|e| format!("Playback ACK: {e:?}"))?;
        if !ack.ok { return Err("Display rejected video playback".into()); }
    }
    Ok(())
}

/// Diagnostic CLI playback explicitly clears the overlay.
#[allow(dead_code)] // This module is shared with the desktop app.
pub fn play(path: &Path) -> Result<String, String> {
    let (filename, data) = read_prepared(path)?;
    let mut dev = lumien_device::DisplaySession::open_first().map_err(|e| format!("Open display: {e:?}"))?;
    play_prepared(&mut dev, filename, &data, true)?;
    Ok(filename.into())
}
