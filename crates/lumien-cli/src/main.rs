#[path = "../../../lumien-app/src-tauri/src/youtube.rs"]
mod youtube;
use lumien_device::{enumerate_devices, PanoramaDevice};
use lumien_protocol::{
    encode_batch_label_update_frame, encode_brightness_frame, encode_clear_run_config_frame,
    encode_frame, encode_get_auth_frame, encode_get_device_info_frame, encode_run_config_frame,
    encode_work_config_frame, parse_response_frame, GroupLabelUpdate, Label, LabelBackground,
    LabelGroup, LabelTextUpdate, TextAlign, CANVAS_HEIGHT, CANVAS_WIDTH,
};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::time::{Duration, Instant};
use lumien_telemetry::{NetworkSampler, format_rate, format_metric, is_network};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LayoutWidget {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub visible: bool,
    pub locked: bool,
    pub text: String,
    pub color: String,
    pub background: String,
    #[serde(rename = "fontSize")]
    pub font_size: u32,
    pub value: f64,
    pub binding: String,
    #[serde(default)]
    pub image: String,
    #[serde(default = "default_font_family", rename = "fontFamily", alias = "font_family")]
    pub font_family: String,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
}

fn default_font_family() -> String {
    "roboto".to_string()
}

fn resolve_device_font(family: &str, bold: bool, italic: bool) -> String {
    match family.to_ascii_lowercase().as_str() {
        "orbitron" => "Orbitron-Regular".to_string(),
        "bitcount-single" | "bitcount" => "BitcountSingle-Regular".to_string(),
        "rubik-glitch" | "rubik" => "RubikGlitch-Regular".to_string(),
        "roboto-black" => {
            if italic {
                "roboto-blackitalic".to_string()
            } else {
                "roboto-black".to_string()
            }
        }
        _ => {
            match (bold, italic) {
                (true, true) => "roboto-bolditalic".to_string(),
                (true, false) => "roboto-bold".to_string(),
                (false, true) => "roboto-italic".to_string(),
                (false, false) => "roboto-regular".to_string(),
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LayoutScene {
    pub version: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub widgets: Vec<LayoutWidget>,
}

fn print_usage() {
    eprintln!("Lumien CLI - TRYX Panorama SE Controller");
    eprintln!("Usage:");
    eprintln!("  lumien-cli prepare-youtube URL YT_DLP_EXE FFMPEG_EXE   Download and convert; no USB writes");
    eprintln!("  lumien-cli play-video PREPARED_H264_FILE               Upload, play full screen, clear overlays");
    eprintln!("  lumien-cli network                               Sample host download/upload speed (read-only, no USB)");
    eprintln!("  lumien-cli probe                                  Enumerate devices and verify handle acquisition");
    eprintln!("  lumien-cli check-prereqs                          Check running processes and communication prerequisites");
    eprintln!("  lumien-cli get-info                               Query device OS, firmware, and serial number (read-only)");
    eprintln!("  lumien-cli set-brightness <0..100>                Send a single verified brightness transaction");
    eprintln!("  lumien-cli set-media <file_name>                  Switch active background video on display");
    eprintln!("  lumien-cli apply-widget [--dry-run] [options]     Apply one minimal supported widget to 2240x1080 canvas");
    eprintln!("         options: --x <u32> --y <u32> --text <str> --val <u32>");
    eprintln!("  lumien-cli update-text <gid> <lid> <text> [--dry] Update label text dynamically at runtime (1 Hz)");
    eprintln!("  lumien-cli clear-widgets [--dry-run]              Clear all LVGL widgets from display overlay");
    eprintln!("  lumien-cli save-layout <file> [options]           Save a 2240x1080 layout to JSON (Scene v2)");
    eprintln!("  lumien-cli load-layout <file> [--dry-run]         Validate and apply a JSON layout to display");
}

fn check_process(name: &str) -> Vec<u32> {
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {}", name), "/FO", "CSV", "/NH"])
        .output();

    let mut pids = Vec::new();
    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() >= 2 {
                let clean_pid = parts[1].trim_matches('"').trim();
                if let Ok(pid) = clean_pid.parse::<u32>() {
                    pids.push(pid);
                }
            }
        }
    }
    pids
}

fn parse_hex_color(hex: &str) -> u32 {
    let clean = hex.trim_start_matches('#');
    u32::from_str_radix(clean, 16).unwrap_or(0x67E8D0)
}

fn build_metric_group(gid: u32, x: u32, y: u32, width: u32, height: u32, title: &str, value_str: &str, color_hex: &str) -> LabelGroup {
    let color = parse_hex_color(color_hex);
    let label_title = Label {
        label_id: gid + 1,
        line: 1,
        order: 0,
        gap_left: 12,
        background: LabelBackground::None,
        background_color: 0,
        gradient_color: 0,
        text_font: "roboto-regular".to_string(),
        text_size: 32,
        text_color: color,
        text: title.to_string(),
    };
    let label_val = Label {
        label_id: gid + 2,
        line: 0,
        order: 1,
        gap_left: 0,
        background: LabelBackground::None,
        background_color: 0,
        gradient_color: 0,
        text_font: "roboto-regular".to_string(),
        text_size: 140,
        text_color: color,
        text: value_str.to_string(),
    };
    let label_unit = Label {
        label_id: gid + 3,
        line: 0,
        order: 2,
        gap_left: 4,
        background: LabelBackground::None,
        background_color: 0,
        gradient_color: 0,
        text_font: "roboto-regular".to_string(),
        text_size: 140,
        text_color: color,
        text: "°C".to_string(),
    };

    LabelGroup {
        group_id: gid,
        group_x: x,
        group_y: y,
        group_width: width,
        group_height: height,
        text_align: TextAlign::Left,
        line_gap: -10,
        labels: vec![label_title, label_val, label_unit],
    }
}

/// Exits when UDB.exe is running, to prevent USB packet interleaving with KANALI.
fn require_quiet_usb() {
    let udb_pids = check_process("UDB.exe");
    if !udb_pids.is_empty() {
        eprintln!("[PRE-FLIGHT BLOCKER] UDB.exe (PID(s): {:?}) is running.", udb_pids);
        eprintln!("Please close KANALI before testing to prevent USB packet interleaving.");
        std::process::exit(2);
    }
}

fn send_usb_frame(frame: &[u8], desc: &str) {
    require_quiet_usb();

    let t_start = Instant::now();
    println!("[{:?}] Connecting to TRYX Panorama SE (VID 0x391A, PID 0x1021)...", t_start.elapsed());
    let mut dev = match PanoramaDevice::open_first() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[ERROR] Failed to open device: {:?}", e);
            eprintln!("        Ensure display USB cable is connected and power is on.");
            std::process::exit(1);
        }
    };
    println!("[{:?}] Device handle open OK.", t_start.elapsed());

    println!("[{:?}] Writing {} ({} bytes) to Bulk OUT (timeout: 2000ms)...", t_start.elapsed(), desc, frame.len());
    let t_write = Instant::now();
    match dev.write_frame(frame, Duration::from_millis(2000)) {
        Ok(bytes) => {
            println!("[{:?}] Write completed: {} bytes in {:.2?}.", t_start.elapsed(), bytes, t_write.elapsed());
        }
        Err(e) => {
            eprintln!("[ERROR] Write failed: {:?}", e);
            std::process::exit(1);
        }
    }

    println!("[{:?}] Awaiting device ACK from Bulk IN (timeout: 2000ms)...", t_start.elapsed());
    let t_read = Instant::now();
    match dev.read_frame(Duration::from_millis(2000)) {
        Ok(payload) => {
            let total_frame = encode_frame(&payload);
            println!("[{:?}] Read completed in {:.2?}. Response ({} bytes):", t_start.elapsed(), t_read.elapsed(), total_frame.len());
            print!("  Hex: ");
            for b in &total_frame {
                print!("{:02x} ", b);
            }
            println!();

            match parse_response_frame(&total_frame) {
                Ok(ack) => {
                    if ack.ok {
                        println!("\n>>> TRANSACTION SUCCESSFUL! Device acknowledged '{}' with 'ok'.", desc);
                    } else {
                        println!("\n[WARNING] Device acknowledged but status was not 'ok'.");
                    }
                }
                Err(e) => {
                    println!("[WARNING] Response received but payload parse returned: {:?}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("[ERROR] Read failed or timed out: {:?}", e);
            std::process::exit(1);
        }
    }

    drop(dev);
    println!("[{:?}] Handle closed cleanly.", t_start.elapsed());
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    match args[1].as_str() {
        "play-video" => {
            if args.len() != 3 { eprintln!("Usage: lumien-cli play-video PREPARED_H264_FILE"); std::process::exit(1); }
            require_quiet_usb();
            match youtube::play(std::path::Path::new(&args[2])) {
                Ok(name) => println!("Playback and clear-overlay ACKs received: {name}"),
                Err(error) => { eprintln!("{error}"); std::process::exit(1); }
            }
        }
        "prepare-youtube" => {
            if args.len() != 5 { eprintln!("Usage: lumien-cli prepare-youtube URL YT_DLP_EXE FFMPEG_EXE (offline conversion target; no USB writes)"); std::process::exit(1); }
            match youtube::prepare(&args[2], std::path::Path::new("target/youtube"), std::path::Path::new(&args[3]), std::path::Path::new(&args[4])) {
                Ok(path) => println!("{}", path.display()),
                Err(error) => { eprintln!("{error}"); std::process::exit(1); }
            }
        }
        "network" => {
            let mut sampler = NetworkSampler::default();
            if let Err(error) = sampler.sample() { eprintln!("Network counters unavailable: {error}"); std::process::exit(1); }
            std::thread::sleep(Duration::from_secs(1));
            match sampler.sample() {
                Ok(Some(rates)) => {
                    let (down, down_unit) = format_rate(Some(rates.download));
                    let (up, up_unit) = format_rate(Some(rates.upload));
                    println!("Active physical Ethernet/Wi-Fi adapters (combined):");
                    println!("Download: {down} {down_unit} ({:.0} bytes/s)", rates.download);
                    println!("Upload:   {up} {up_unit} ({:.0} bytes/s)", rates.upload);
                }
                other => { eprintln!("Network counters unavailable: {other:?}"); std::process::exit(1); }
            }
        }
        "probe" => {
            println!("=== Lumien Device Probe ===");
            let devices = enumerate_devices();
            if devices.is_empty() {
                println!("[ERROR] No TRYX Panorama SE devices found (VID 0x391A, PID 0x1021).");
                std::process::exit(1);
            }
            println!("Found {} matching device(s):", devices.len());
            for (i, path) in devices.iter().enumerate() {
                println!("  [{}] {}", i + 1, path);
                let t0 = Instant::now();
                match PanoramaDevice::open(path) {
                    Ok(dev) => {
                        println!("      Status: OPEN OK in {:.2?}", t0.elapsed());
                        drop(dev);
                        println!("      Status: CLOSED cleanly.");
                    }
                    Err(e) => println!("      Status: OPEN FAILED: {:?}", e),
                }
            }
        }

        "check-prereqs" => {
            println!("=== Lumien Communication Prerequisites Check ===");
            let kanali_pids = check_process("KANALI.exe");
            let screen_pids = check_process("Screen.exe");
            let udb_pids = check_process("UDB.exe");

            println!("Active Process Inventory:");
            println!("  KANALI.exe : {} instance(s) {:?}", kanali_pids.len(), kanali_pids);
            println!("  Screen.exe : {} instance(s) {:?}", screen_pids.len(), screen_pids);
            println!("  UDB.exe    : {} instance(s) {:?}", udb_pids.len(), udb_pids);

            let devices = enumerate_devices();
            println!("\nDevice Topology:");
            println!("  Detected paths : {}", devices.len());
            for p in &devices {
                println!("    {}", p);
            }

            println!("\nPrerequisite Evaluation:");
            if !udb_pids.is_empty() || !kanali_pids.is_empty() {
                println!("  [BLOCKER] KANALI/UDB processes are active. Please close KANALI to prevent USB packet collisions.");
            } else {
                println!("  [OK] No KANALI processes active. Host USB channel is quiescent.");
            }
        }

        "get-info" => {
            println!("=== Lumien Device Information Query ===");
            let frame = encode_get_device_info_frame();
            send_usb_frame(&frame, "get_device_info");
        }

        "auth" => {
            println!("=== Lumien Device Auth Query ===");
            let frame = encode_get_auth_frame();
            send_usb_frame(&frame, "get_device_auth");
        }

        "file-list" => {
            println!("=== Lumien Query Device File List (Tag 103) ===");
            let mut dummy_msg = Vec::with_capacity(4);
            dummy_msg.extend_from_slice(&[0x0a, 0x02, 0x4e, 0x41]); // dummy: "NA"

            let mut req_pkg = Vec::with_capacity(16);
            req_pkg.extend_from_slice(&[0x0a, 0x02, 0x08, 0x01]); // header
            req_pkg.extend_from_slice(&[0xba, 0x06, dummy_msg.len() as u8]); // tag 103
            req_pkg.extend_from_slice(&dummy_msg);

            let frame = lumien_protocol::encode_frame(&req_pkg);
            send_usb_frame(&frame, "get_file_list");
        }

        "set-brightness" => {
            if args.len() < 3 {
                eprintln!("Error: brightness value (0..100) required.");
                std::process::exit(1);
            }
            let brightness: u8 = match args[2].parse() {
                Ok(v) if v <= 100 => v,
                _ => {
                    eprintln!("Error: brightness must be between 0 and 100.");
                    std::process::exit(1);
                }
            };
            println!("=== Lumien Set Brightness ===");
            let frame = encode_brightness_frame(brightness);
            send_usb_frame(&frame, &format!("set_brightness_{}", brightness));
        }

        "set-media" => {
            if args.len() < 3 {
                eprintln!("Error: media file name required (e.g. default_01.mp4.h264_2240x1080).");
                std::process::exit(1);
            }
            let media_file = &args[2];
            println!("=== Lumien Set Media Background ===");
            let frame = encode_work_config_frame(media_file);
            send_usb_frame(&frame, &format!("set_media_{}", media_file));
        }

        "set-solid-color" => {
            if args.len() < 3 {
                eprintln!("Error: hex color required (e.g. #000000 or #18262e).");
                std::process::exit(1);
            }
            let raw_color = &args[2];
            let clean = raw_color.trim_start_matches('#').to_ascii_lowercase();
            if clean.len() != 6 || !clean.bytes().all(|b| b.is_ascii_hexdigit()) {
                eprintln!("Error: expected six-digit hex color (e.g. #18262e).");
                std::process::exit(1);
            }
            let filename = format!("solid_{}.mp4.h264_2240x1080", clean);
            println!("=== Lumien Set Solid Color Background: #{} ===", clean);

            // Read solid color bytes from scratch/swatches or generate
            let swatch_path = format!("lumien-app/src-tauri/assets/swatches/solid_{}.h264_2240x1080", clean);
            let data = match fs::read(&swatch_path) {
                Ok(bytes) => bytes,
                Err(_) => {
                    let alt_path = format!("scratch/swatches/solid_{}.h264_2240x1080", clean);
                    match fs::read(&alt_path) {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            eprintln!("[ERROR] Could not find swatch file: {:?}", e);
                            std::process::exit(1);
                        }
                    }
                }
            };

            require_quiet_usb();
            let mut dev = match PanoramaDevice::open_first() {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("[ERROR] Failed to open device: {:?}", e);
                    std::process::exit(1);
                }
            };

            println!("Transmitting {} ({} bytes) over USB Bulk OUT...", filename, data.len());
            let t0 = Instant::now();
            if let Err(e) = dev.transmit_file(&filename, &data, Duration::from_millis(2000)) {
                eprintln!("[ERROR] Transmit failed: {:?}", e);
                std::process::exit(1);
            }
            println!("Transmit complete in {:.2?}.", t0.elapsed());

            println!("Activating playback via WorkConfigPb...");
            let frame = encode_work_config_frame(&filename);
            if let Err(e) = dev.write_frame(&frame, Duration::from_millis(2000)) {
                eprintln!("[ERROR] Activate write failed: {:?}", e);
                std::process::exit(1);
            }
            let rsp = match dev.read_frame(Duration::from_millis(2000)) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[ERROR] Activate ACK read failed: {:?}", e);
                    std::process::exit(1);
                }
            };
            let total = lumien_protocol::encode_frame(&rsp);
            match parse_response_frame(&total) {
                Ok(ack) if ack.ok => println!(">>> SUCCESS: Solid color #{} active on display!", clean),
                _ => eprintln!("[WARNING] Device did not return OK for activate."),
            }
        }

        "apply-widget" => {
            let is_dry_run = args.iter().any(|a| a == "--dry-run");
            let mut x = 100u32;
            let mut y = 200u32;
            let mut text = "LUMIEN CPU".to_string();
            let mut val = "42".to_string();
            let mut font = "roboto-regular".to_string();

            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--x" if i + 1 < args.len() => { x = args[i + 1].parse().unwrap_or(100); i += 1; }
                    "--y" if i + 1 < args.len() => { y = args[i + 1].parse().unwrap_or(200); i += 1; }
                    "--text" if i + 1 < args.len() => { text = args[i + 1].clone(); i += 1; }
                    "--val" if i + 1 < args.len() => { val = args[i + 1].clone(); i += 1; }
                    "--font" if i + 1 < args.len() => { font = args[i + 1].clone(); i += 1; }
                    _ => {}
                }
                i += 1;
            }

            println!("=== Lumien Apply Widget (Tag 201 RunConfigPb) ===");
            println!("Target coordinates: ({}, {}) | Dimensions: 440 x 240 px", x, y);
            println!("Widget Title: '{}' | Value: '{}' | Font: '{}' | Canvas bounds: {}x{}", text, val, font, CANVAS_WIDTH, CANVAS_HEIGHT);

            let mut group = build_metric_group(100, x, y, 440, 240, &text, &val, "#67e8d0");
            for l in &mut group.labels {
                l.text_font = font.clone();
            }
            let frame = match encode_run_config_frame(&[group]) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("[ERROR] Geometry/encoding validation failed: {:?}", e);
                    std::process::exit(1);
                }
            };

            println!("Wire Frame Encoded: {} bytes (Tag 201 run_config)", frame.len());
            print!("  Hex: ");
            for b in &frame {
                print!("{:02x} ", b);
            }
            println!();

            if is_dry_run {
                println!("\n[DRY-RUN] Geometry validated within 2240x1080. Frame successfully encoded.");
                println!("          No USB write performed (--dry-run flag active).");
                return;
            }

            send_usb_frame(&frame, "apply_widget_run_config");
        }

        "update-text" => {
            if args.len() < 5 {
                eprintln!("Usage: lumien-cli update-text <group_id> <label_id> <text> [--dry-run]");
                std::process::exit(1);
            }
            let is_dry_run = args.iter().any(|a| a == "--dry-run" || a == "--dry");
            let group_id: u32 = args[2].parse().expect("valid group_id");
            let label_id: u32 = args[3].parse().expect("valid label_id");
            let new_text = args[4].clone();

            println!("=== Lumien Dynamic Text Update (Tag 300 BatchGroupLabelUpdatePb) ===");
            println!("Updating Group #{} -> Label #{} text to '{}'", group_id, label_id, new_text);

            let update = GroupLabelUpdate {
                group_id,
                label_texts: vec![LabelTextUpdate {
                    label_id,
                    text: new_text.clone(),
                }],
            };

            let frame = match encode_batch_label_update_frame(&[update]) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("[ERROR] Encoding failed: {:?}", e);
                    std::process::exit(1);
                }
            };

            println!("Wire Frame Encoded: {} bytes (Tag 300 batch_group_label_update)", frame.len());
            print!("  Hex: ");
            for b in &frame {
                print!("{:02x} ", b);
            }
            println!();

            if is_dry_run {
                println!("\n[DRY-RUN] Delta text frame successfully encoded.");
                println!("          No USB write performed (--dry-run flag active).");
                return;
            }

            send_usb_frame(&frame, &format!("update_text_{}_{}", group_id, label_id));
        }

        "clear-widgets" => {
            let is_dry_run = args.iter().any(|a| a == "--dry-run");
            println!("=== Lumien Clear All Widgets ===");
            let frame = encode_clear_run_config_frame();
            println!("Wire Frame Encoded: {} bytes (Tag 201 empty run_config)", frame.len());
            print!("  Hex: ");
            for b in &frame {
                print!("{:02x} ", b);
            }
            println!();

            if is_dry_run {
                println!("\n[DRY-RUN] Clear frame successfully encoded. No USB write performed.");
                return;
            }

            send_usb_frame(&frame, "clear_widgets");
        }

        "save-layout" => {
            if args.len() < 3 {
                eprintln!("Usage: lumien-cli save-layout <file_path> [--x <x>] [--y <y>] [--text <str>] [--val <u32>]");
                std::process::exit(1);
            }
            let path = &args[2];
            let mut x = 100u32;
            let mut y = 200u32;
            let mut text = "CPU TEMPERATURE".to_string();
            let mut val = 42u32;

            let mut i = 3;
            while i < args.len() {
                match args[i].as_str() {
                    "--x" if i + 1 < args.len() => { x = args[i + 1].parse().unwrap_or(100); i += 1; }
                    "--y" if i + 1 < args.len() => { y = args[i + 1].parse().unwrap_or(200); i += 1; }
                    "--text" if i + 1 < args.len() => { text = args[i + 1].clone(); i += 1; }
                    "--val" if i + 1 < args.len() => { val = args[i + 1].parse().unwrap_or(42); i += 1; }
                    _ => {}
                }
                i += 1;
            }

            let widget = LayoutWidget {
                id: "widget-1".to_string(),
                kind: "Metric value".to_string(),
                name: "CPU Temperature".to_string(),
                x,
                y,
                width: 440,
                height: 240,
                visible: true,
                locked: false,
                text,
                color: "#67e8d0".to_string(),
                background: "#18262e".to_string(),
                font_size: 64,
                value: val as f64,
                binding: "CPU temperature".to_string(),
                image: "".to_string(),
                font_family: "roboto".to_string(),
                bold: false,
                italic: false,
            };

            let scene = LayoutScene {
                version: 2,
                name: "Lumien Controlled Layout".to_string(),
                width: CANVAS_WIDTH,
                height: CANVAS_HEIGHT,
                widgets: vec![widget],
            };

            let json_str = serde_json::to_string_pretty(&scene).expect("serialize scene JSON");
            if let Some(parent) = std::path::Path::new(path).parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = fs::create_dir_all(parent);
                }
            }
            fs::write(path, json_str).expect("write JSON layout file");
            println!("Saved 2240x1080 layout to '{}' successfully.", path);
        }

        "load-layout" => {
            if args.len() < 3 {
                eprintln!("Usage: lumien-cli load-layout <file_path> [--dry-run]");
                std::process::exit(1);
            }
            let path = &args[2];
            let is_dry_run = args.iter().any(|a| a == "--dry-run");

            println!("=== Lumien Load & Apply Layout ===");
            let content = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[ERROR] Could not read file '{}': {:?}", path, e);
                    std::process::exit(1);
                }
            };

            let scene: LayoutScene = match serde_json::from_str(&content) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[ERROR] JSON validation failed: {:?}", e);
                    std::process::exit(1);
                }
            };

            if scene.width != CANVAS_WIDTH || scene.height != CANVAS_HEIGHT {
                eprintln!("[ERROR] Invalid canvas resolution: {}x{}. Expected 2240x1080.", scene.width, scene.height);
                std::process::exit(1);
            }

            println!("Loaded layout '{}' ({} widgets).", scene.name, scene.widgets.len());

            let mut groups = Vec::new();
            for (idx, w) in scene.widgets.iter().enumerate() {
                if !w.visible { continue; }
                // Gate 2 / Gate 9: Reject unsupported widgets on hardware
                if w.kind != "Metric value" && w.kind != "Text" {
                    eprintln!("[ERROR] Unsupported widget kind '{}' on hardware.", w.kind);
                    eprintln!("        Only 'Metric value' and 'Text' are supported by device LVGL engine.");
                    std::process::exit(1);
                }
                let gid = 100 + (idx as u32 * 10);
                let color = parse_hex_color(&w.color);
                let device_font = resolve_device_font(&w.font_family, w.bold, w.italic);
                if !w.value.is_finite() || w.value < 0.0 || w.value > if is_network(&w.binding) { 9_007_199_254_740_991.0 } else { 100.0 } {
                    eprintln!("[ERROR] Invalid metric fallback value"); std::process::exit(1);
                }
                let group = if w.kind == "Metric value" {
                    let (text, unit) = format_metric(&w.binding, Some(w.value));
                    let mut group = build_metric_group(gid, w.x, w.y, w.width, w.height, &w.text, &text, &w.color);
                    let val_size = ((w.font_size as f32 * 1.65).round() as u32).clamp(16, 220);
                    let unit_size = ((val_size as f32 * 0.65).round() as u32).clamp(12, 140);
                    let title_size = ((w.font_size as f32 * 0.45).round() as u32).clamp(14, 60);
                    group.labels[0].text_size = title_size;
                    group.labels[0].text_font = device_font.clone();
                    group.labels[1].text_size = val_size;
                    group.labels[1].text_font = device_font.clone();
                    group.labels[2].text = unit.into();
                    group.labels[2].text_size = unit_size;
                    group.labels[2].text_font = device_font;
                    group
                } else {
                    let label = Label {
                        label_id: gid + 1,
                        line: 0,
                        order: 0,
                        gap_left: 0,
                        background: LabelBackground::None,
                        background_color: 0,
                        gradient_color: 0,
                        text_font: device_font,
                        text_size: w.font_size,
                        text_color: color,
                        text: w.text.clone(),
                    };
                    LabelGroup {
                        group_id: gid,
                        group_x: w.x,
                        group_y: w.y,
                        group_width: w.width,
                        group_height: w.height,
                        text_align: TextAlign::Left,
                        line_gap: 0,
                        labels: vec![label],
                    }
                };
                groups.push(group);
            }

            let frame = match encode_run_config_frame(&groups) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("[ERROR] Geometry validation failed: {:?}", e);
                    std::process::exit(1);
                }
            };

            println!("Layout encoded to {} bytes (Tag 201).", frame.len());
            if is_dry_run {
                println!("\n[DRY-RUN] Layout validated and encoded. No USB write performed.");
                return;
            }

            send_usb_frame(&frame, &format!("apply_layout_{}", scene.name));
        }

        cmd => {
            eprintln!("Unknown command: {}", cmd);
            print_usage();
            std::process::exit(1);
        }
    }
}
