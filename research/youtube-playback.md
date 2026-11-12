# YouTube playback — 2026-11-11

## Supported workflow
Background options → YouTube link → Play full screen. Public recorded YouTube/watch, youtu.be, shorts and embed URLs are normalized to one video ID. Playlists, live streams and videos over five minutes are rejected. Downloads/converted files are limited to 128 MiB. The complete video is downloaded, converted and uploaded before playback; no browser/YouTube live streaming or audio reaches the LCD. Aspect ratio is preserved by center cropping to fill 2240 × 1080.

## Implementation and dependencies
Shared youtube.rs is compiled into desktop and CLI. yt-dlp is discovered in app config tools/yt-dlp.exe or PATH. FFmpeg uses existing discovery; Node supports yt-dlp JavaScript challenges. Installed official yt-dlp Windows release 2026.10.26 under %APPDATA%/dev.lumien.editor/tools for this host. Dependencies are not bundled by installer. Missing tools/download restrictions produce actionable errors; browser cookies/accounts are never imported. Process arguments bypass shell; downloader ignores external config; commands time out after ten minutes.

Research: [yt-dlp official README](https://github.com/yt-dlp/yt-dlp#readme), [YouTube JavaScript requirements](https://github.com/yt-dlp/yt-dlp/wiki/EJS), and existing media-pipeline.md. URL import uses video-only streams; FFmpeg produces raw Annex B H.264 Main@4.1/yuv420p, 30 fps, intra-only, no B frames. Existing transfer/CRC and WorkConfig commands reused. Upload and clear-overlay/playback transactions run under existing USB worker mutex after download completes. Saved editor layout remains available through Apply to Display; video mode is a runtime action and is not saved in layout JSON.

## Evidence and limits
User test URL: https://www.youtube.com/watch?v=krDWc30PAGg (Video Test 2K Samsung, 66 seconds; extractor offered 576 × 360 source). Initial GOP-30 stream received ACKs but user reported black/frozen display. Second test used intra-only encoding and cleared overlays BEFORE playback activation; user confirmed moving video. These changes were tested together, so neither is claimed as sole cause. Final implementation uses this working combination. Upscaling cannot create source detail. No extended playback soak or reboot persistence tested. Download caches remain under app config/youtube (CLI target/youtube); storage cleanup is manual for now.

CLI: prepare-youtube URL YT_DLP_EXE FFMPEG_EXE produces a local prepared path with no USB writes. play-video PREPARED_H264_FILE uploads and clears overlays; check-prereqs and stop other writers first.

## Download gallery
Backgrounds now lists existing completed app downloads with cached local thumbnails, titles, duration and size. Click a card to re-upload/play without network download or transcoding. Gallery refreshes on entering Backgrounds and after import/replay. In-progress .partial streams are excluded. Selection IDs are resolved within canonical library root; removed files return an error. Repeated downloads appear as separate entries; first-frame thumbnails may be black for black intros.
