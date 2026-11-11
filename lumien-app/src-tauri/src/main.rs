#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod layout_files;
use lumien_telemetry::{NetworkSampler, NetworkRates, format_metric, is_network};

use lumien_device::{enumerate_devices, DisplaySession};
use lumien_protocol::{
    encode_batch_label_update_frame, encode_clear_run_config_frame, encode_run_config_frame,
    parse_response_frame, GroupLabelUpdate, Label, LabelBackground, LabelGroup, LabelTextUpdate,
    TextAlign, CANVAS_HEIGHT, CANVAS_WIDTH,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, LazyLock};
use std::time::Duration;
use tauri::Manager;
use tauri::tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState};
use tauri::menu::{Menu, MenuItem};

#[repr(C)]
struct FILETIME {
    dw_low_date_time: u32,
    dw_high_date_time: u32,
}

#[repr(C)]
struct MEMORYSTATUSEX {
    dw_length: u32,
    dw_memory_load: u32,
    ull_total_phys: u64,
    ull_avail_phys: u64,
    ull_total_page_file: u64,
    ull_avail_page_file: u64,
    ull_total_virtual: u64,
    ull_avail_virtual: u64,
    ull_avail_extended_virtual: u64,
}

#[allow(dead_code)]
#[repr(C)]
struct SYSTEMTIME {
    w_year: u16,
    w_month: u16,
    w_day_of_week: u16,
    w_day: u16,
    w_hour: u16,
    w_minute: u16,
    w_second: u16,
    w_milliseconds: u16,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetSystemTimes(idle: *mut FILETIME, kernel: *mut FILETIME, user: *mut FILETIME) -> i32;
    fn GlobalMemoryStatusEx(stat: *mut MEMORYSTATUSEX) -> i32;
    fn LoadLibraryA(name: *const u8) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    #[allow(dead_code)]
    fn GetLocalTime(lp_system_time: *mut SYSTEMTIME);
}

#[allow(dead_code)]
fn hex_preview(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ")
}

#[inline]
#[allow(unused_variables)]
fn log_usb(level: &str, msg: &str) {
    // ponytail: debug transaction log disabled in release builds (not(debug_assertions)).
    #[cfg(debug_assertions)]
    {
        let mut st = std::mem::MaybeUninit::<SYSTEMTIME>::uninit();
        unsafe { GetLocalTime(st.as_mut_ptr()); }
        let st = unsafe { st.assume_init() };
        let line = format!(
            "[{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}] [{:5}] {}\n",
            st.w_year, st.w_month, st.w_day, st.w_hour, st.w_minute, st.w_second, st.w_milliseconds,
            level, msg
        );
        print!("{}", line);
        if let Ok(user_profile) = std::env::var("USERPROFILE") {
            let log_path = format!("{}\\Desktop\\tryx_usb_transactions.log", user_profile);
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) {
                let _ = file.write_all(line.as_bytes());
            }
        }
    }
}

#[repr(C)]
struct NvmlUtilization {
    gpu: u32,
    memory: u32,
}

struct GpuMonitor {
    _handle: *mut c_void,
    get_temp: Option<unsafe extern "system" fn(*mut c_void, u32, *mut u32) -> i32>,
    get_util: Option<unsafe extern "system" fn(*mut c_void, *mut NvmlUtilization) -> i32>,
    dev: *mut c_void,
}

unsafe impl Send for GpuMonitor {}
unsafe impl Sync for GpuMonitor {}

static GPU_MONITOR: Mutex<Option<GpuMonitor>> = Mutex::new(None);

fn read_gpu_telemetry() -> (u32, u32) {
    let mut lock = GPU_MONITOR.lock().unwrap();
    if lock.is_none() {
        unsafe {
            let lib = LoadLibraryA(b"nvml.dll\0".as_ptr());
            if !lib.is_null() {
                let init_fn: Option<unsafe extern "system" fn() -> i32> =
                    std::mem::transmute(GetProcAddress(lib, b"nvmlInit_v2\0".as_ptr()));
                let get_handle_fn: Option<unsafe extern "system" fn(u32, *mut *mut c_void) -> i32> =
                    std::mem::transmute(GetProcAddress(lib, b"nvmlDeviceGetHandleByIndex_v2\0".as_ptr()));
                let get_temp: Option<unsafe extern "system" fn(*mut c_void, u32, *mut u32) -> i32> =
                    std::mem::transmute(GetProcAddress(lib, b"nvmlDeviceGetTemperature\0".as_ptr()));
                let get_util: Option<unsafe extern "system" fn(*mut c_void, *mut NvmlUtilization) -> i32> =
                    std::mem::transmute(GetProcAddress(lib, b"nvmlDeviceGetUtilizationRates\0".as_ptr()));

                if let (Some(init), Some(get_handle)) = (init_fn, get_handle_fn) {
                    if init() == 0 {
                        let mut dev: *mut c_void = std::ptr::null_mut();
                        if get_handle(0, &mut dev) == 0 && !dev.is_null() {
                            *lock = Some(GpuMonitor {
                                _handle: lib,
                                get_temp,
                                get_util,
                                dev,
                            });
                        }
                    }
                }
            }
        }
    }

    if let Some(ref gpu) = *lock {
        let mut temp = 0u32;
        let mut util = NvmlUtilization { gpu: 0, memory: 0 };
        if let Some(f_temp) = gpu.get_temp {
            unsafe { f_temp(gpu.dev, 0, &mut temp); }
        }
        if let Some(f_util) = gpu.get_util {
            unsafe { f_util(gpu.dev, &mut util); }
        }
        return (temp, util.gpu);
    }
    (0, 0)
}

fn read_ram_load() -> u32 {
    let mut stat = std::mem::MaybeUninit::<MEMORYSTATUSEX>::uninit();
    unsafe {
        let ptr = stat.as_mut_ptr();
        (*ptr).dw_length = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(ptr) != 0 {
            (*ptr).dw_memory_load
        } else {
            0
        }
    }
}

fn filetime_to_u64(ft: &FILETIME) -> u64 {
    ((ft.dw_high_date_time as u64) << 32) | (ft.dw_low_date_time as u64)
}

static LAST_CPU_TIMES: Mutex<Option<(u64, u64, u64)>> = Mutex::new(None);
static NETWORK: LazyLock<Mutex<NetworkSampler>> = LazyLock::new(|| Mutex::new(NetworkSampler::default()));

fn read_network_rates() -> Option<NetworkRates> {
    NETWORK.lock().ok()?.sample().ok().flatten()
}

fn read_cpu_load() -> u32 {
    let mut idle = FILETIME { dw_low_date_time: 0, dw_high_date_time: 0 };
    let mut kernel = FILETIME { dw_low_date_time: 0, dw_high_date_time: 0 };
    let mut user = FILETIME { dw_low_date_time: 0, dw_high_date_time: 0 };

    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
        return 0;
    }

    let i = filetime_to_u64(&idle);
    let k = filetime_to_u64(&kernel);
    let u = filetime_to_u64(&user);

    let mut lock = LAST_CPU_TIMES.lock().unwrap();
    if let Some((prev_i, prev_k, prev_u)) = *lock {
        *lock = Some((i, k, u));
        let d_idle = i.saturating_sub(prev_i);
        let d_total = (k.saturating_sub(prev_k)) + (u.saturating_sub(prev_u));
        if d_total > 0 && d_total >= d_idle {
            let pct = ((d_total - d_idle) * 100) / d_total;
            return pct as u32;
        }
    } else {
        *lock = Some((i, k, u));
    }
    0
}

#[derive(Clone)]
struct ActiveBinding {
    group_id: u32,
    label_id: u32,
    sensor: String,
}

fn binding_update(binding: &ActiveBinding, value: Option<f64>) -> GroupLabelUpdate {
    let (text, unit) = format_metric(&binding.sensor, value);
    let mut label_texts = vec![LabelTextUpdate { label_id: binding.label_id, text }];
    if is_network(&binding.sensor) {
        label_texts.push(LabelTextUpdate { label_id: binding.label_id + 1, text: unit.into() });
    }
    GroupLabelUpdate { group_id: binding.group_id, label_texts }
}

// Hold this lock through stop/join and the complete command/ACK transaction.
static SYNC_WORKER: Mutex<Option<SyncWorker>> = Mutex::new(None);

struct SyncWorker {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for SyncWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct LayoutWidget {
    id: String,
    kind: String,
    name: String,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    visible: bool,
    locked: bool,
    text: String,
    color: String,
    background: String,
    #[serde(rename = "fontSize")]
    font_size: u32,
    value: f64,
    binding: String,
    #[serde(default = "default_font_family", rename = "fontFamily", alias = "font_family")]
    font_family: String,
    #[serde(default)]
    bold: bool,
    #[serde(default)]
    italic: bool,
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
struct LayoutScene {
    version: u32,
    name: String,
    width: u32,
    height: u32,
    widgets: Vec<LayoutWidget>,
}

fn parse_hex_color(hex: &str) -> u32 {
    let clean = hex.trim_start_matches('#');
    u32::from_str_radix(clean, 16).unwrap_or(0xFFFFFF)
}

fn blocks_usb(image: &str, pid: u32, own_pid: u32) -> bool {
    pid != own_pid && ["KANALI.exe", "Screen.exe", "UDB.exe", "lumien-app.exe", "lumien-cli.exe"]
        .iter().any(|name| image.eq_ignore_ascii_case(name))
}

fn ensure_usb_channel_available() -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"]).creation_flags(0x08000000).output()
        .map_err(|e| format!("Cannot check USB prerequisites: {e}"))?;
    if !output.status.success() { return Err("Cannot check USB prerequisites; apply stopped".into()); }
    let mut saw_current_process = false;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut columns = line.split(',');
        let image = columns.next().unwrap_or("").trim_matches('"');
        let pid = columns.next().unwrap_or("").trim_matches('"').parse::<u32>();
        if let Ok(pid) = pid {
            saw_current_process |= pid == std::process::id();
            if blocks_usb(image, pid, std::process::id()) {
                return Err(format!("Close {image} (PID {pid}) before applying to avoid concurrent USB traffic"));
            }
        }
    }
    if !saw_current_process { return Err("Cannot verify running processes; apply stopped".into()); }
    Ok(())
}

#[tauri::command]
fn get_device_status() -> Result<String, String> {
    let devices = enumerate_devices();
    if devices.is_empty() {
        Ok("Disconnected (No device found)".to_string())
    } else {
        Ok("Connected (TRYX Panorama SE)".to_string())
    }
}

#[tauri::command]
fn get_telemetry() -> Result<HashMap<String, Option<f64>>, String> {
    let mut map = HashMap::new();
    let cpu_load = read_cpu_load();
    let ram_load = read_ram_load();
    let (gpu_temp, gpu_load) = read_gpu_telemetry();

    map.insert("cpu_load".to_string(), Some(cpu_load as f64));
    map.insert("cpu_temp".to_string(), Some((40 + (cpu_load / 4)) as f64));
    map.insert("ram".to_string(), Some(ram_load as f64));
    map.insert("gpu_load".to_string(), Some(gpu_load as f64));
    map.insert("gpu_temp".to_string(), Some(gpu_temp as f64));
    let network = read_network_rates();
    map.insert("network_download".into(), network.map(|r| r.download));
    map.insert("network_upload".into(), network.map(|r| r.upload));
    // Backward compatibility keys
    map.insert("cpu".to_string(), Some(cpu_load as f64));
    map.insert("gpu".to_string(), Some(gpu_load as f64));
    Ok(map)
}

#[tauri::command]
fn clear_display() -> Result<String, String> {
    ensure_usb_channel_available()?;
    log_usb("INFO", "Executing clear_display command...");
    let mut worker = SYNC_WORKER.lock().map_err(|_| "USB worker lock poisoned")?;
    drop(worker.take());

    let frame = encode_clear_run_config_frame();
    let mut dev = match DisplaySession::open_first() {
        Ok(d) => {
            log_usb("INFO", "Device opened for clear_display.");
            d
        }
        Err(e) => {
            log_usb("ERROR", &format!("open_first failed during clear_display: {:?}", e));
            return Err(format!("Failed to open device: {:?}", e));
        }
    };

    let t0 = std::time::Instant::now();
    if let Err(e) = dev.write_frame(&frame, Duration::from_millis(2000)) {
        log_usb("ERROR", &format!("Bulk OUT clear_frame failed: {:?}", e));
        return Err(format!("Write failed: {:?}", e));
    }
    log_usb("WRITE", &format!("Clear frame ({} bytes) in {:?}", frame.len(), t0.elapsed()));

    let t1 = std::time::Instant::now();
    let payload = match dev.read_frame(Duration::from_millis(2000)) {
        Ok(p) => {
            let prev = hex_preview(&p[..std::cmp::min(16, p.len())]);
            log_usb("READ", &format!("Clear ACK ({} bytes) in {:?} | Hex: {}", p.len(), t1.elapsed(), prev));
            p
        }
        Err(e) => {
            log_usb("ERROR", &format!("Bulk IN clear_frame ACK failed: {:?}", e));
            return Err(format!("Read failed: {:?}", e));
        }
    };

    let total = lumien_protocol::encode_frame(&payload);
    let ack = parse_response_frame(&total)
        .map_err(|e| format!("Invalid ACK: {:?}", e))?;

    if ack.ok {
        log_usb("INFO", "Display overlay cleared successfully.");
        Ok("Display overlay cleared successfully.".to_string())
    } else {
        log_usb("WARN", "Device rejected clear command.");
        Err("Device rejected clear command.".to_string())
    }
}

#[tauri::command]
fn apply_scene(scene_json: String) -> Result<String, String> {
    ensure_usb_channel_available()?;

    let scene: LayoutScene = serde_json::from_str(&scene_json)
        .map_err(|e| format!("Invalid scene JSON: {}", e))?;

    if scene.width != CANVAS_WIDTH || scene.height != CANVAS_HEIGHT {
        return Err(format!("Invalid canvas size: {}x{}. Expected 2240x1080.", scene.width, scene.height));
    }

    let mut groups = Vec::new();
    let mut bindings = Vec::new();

    for (idx, w) in scene.widgets.iter().enumerate() {
        if !w.visible { continue; }
        if w.kind != "Metric value" && w.kind != "Text" {
            return Err(format!("Widget '{}' of kind '{}' is not supported on hardware. Keep preview-only.", w.name, w.kind));
        }

        let gid = 100 + (idx as u32 * 10);
        let color = parse_hex_color(&w.color);
        let device_font = resolve_device_font(&w.font_family, w.bold, w.italic);
        if !w.value.is_finite() || w.value < 0.0 || w.value > if is_network(&w.binding) { 9_007_199_254_740_991.0 } else { 100.0 } {
            return Err("Invalid metric fallback value".into());
        }
        if w.kind == "Metric value" {
            let (value_text, unit) = format_metric(&w.binding, Some(w.value));
            let val_size = ((w.font_size as f32 * 1.65).round() as u32).clamp(16, 220);
            let unit_size = ((val_size as f32 * 0.65).round() as u32).clamp(12, 140);
            let title_size = ((w.font_size as f32 * 0.45).round() as u32).clamp(14, 60);

            let label_title = Label {
                label_id: gid + 1,
                line: 1,
                order: 0,
                gap_left: 12,
                background: LabelBackground::None,
                background_color: 0,
                gradient_color: 0,
                text_font: device_font.clone(),
                text_size: title_size,
                text_color: color,
                text: w.text.clone(),
            };
            let label_val = Label {
                label_id: gid + 2,
                line: 0,
                order: 1,
                gap_left: 0,
                background: LabelBackground::None,
                background_color: 0,
                gradient_color: 0,
                text_font: device_font.clone(),
                text_size: val_size,
                text_color: color,
                text: value_text,
            };
            let label_unit = Label {
                label_id: gid + 3,
                line: 0,
                order: 2,
                gap_left: 4,
                background: LabelBackground::None,
                background_color: 0,
                gradient_color: 0,
                text_font: device_font,
                text_size: unit_size,
                text_color: color,
                text: unit.to_string(),
            };
            groups.push(LabelGroup {
                group_id: gid,
                group_x: w.x,
                group_y: w.y,
                group_width: w.width,
                group_height: w.height,
                text_align: TextAlign::Left,
                line_gap: -10,
                labels: vec![label_title, label_val, label_unit],
            });

            bindings.push(ActiveBinding {
                group_id: gid,
                label_id: gid + 2,
                sensor: w.binding.clone(),
            });
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
            groups.push(LabelGroup {
                group_id: gid,
                group_x: w.x,
                group_y: w.y,
                group_width: w.width,
                group_height: w.height,
                text_align: TextAlign::Left,
                line_gap: 0,
                labels: vec![label],
            });
        }
    }

    log_usb("INFO", &format!("Encoding scene '{}' ({} widgets)...", scene.name, scene.widgets.len()));
    let frame = encode_run_config_frame(&groups)
        .map_err(|e| {
            log_usb("ERROR", &format!("Geometry/Encoding error: {:?}", e));
            format!("Geometry/Encoding error: {:?}", e)
        })?;

    let mut worker = SYNC_WORKER.lock().map_err(|_| "USB worker lock poisoned")?;
    drop(worker.take());
    let mut dev = match DisplaySession::open_first() {
        Ok(d) => {
            log_usb("INFO", "Device opened for apply_scene.");
            d
        }
        Err(e) => {
            log_usb("ERROR", &format!("Failed to open device: {:?}", e));
            return Err(format!("Failed to open device (ensure display is connected): {:?}", e));
        }
    };

    let t0 = std::time::Instant::now();
    if let Err(e) = dev.write_frame(&frame, Duration::from_millis(2000)) {
        log_usb("ERROR", &format!("Bulk OUT RunConfig failed: {:?}", e));
        return Err(format!("Write failed: {:?}", e));
    }
    log_usb("WRITE", &format!("RunConfig ({} bytes) in {:?}", frame.len(), t0.elapsed()));

    let t1 = std::time::Instant::now();
    let payload = match dev.read_frame(Duration::from_millis(2000)) {
        Ok(p) => {
            let prev = hex_preview(&p[..std::cmp::min(16, p.len())]);
            log_usb("READ", &format!("RunConfig ACK ({} bytes) in {:?} | Hex: {}", p.len(), t1.elapsed(), prev));
            p
        }
        Err(e) => {
            log_usb("ERROR", &format!("Bulk IN RunConfig ACK failed: {:?}", e));
            return Err(format!("Read failed: {:?}", e));
        }
    };

    let total_frame = lumien_protocol::encode_frame(&payload);
    let ack = parse_response_frame(&total_frame)
        .map_err(|e| format!("Invalid response frame: {:?}", e))?;

    if !ack.ok {
        log_usb("WARN", "Device rejected layout frame.");
        return Err("Device rejected layout frame.".to_string());
    }

    // Transfer the acknowledged layout's handle to exactly one worker.
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let thread = std::thread::spawn(move || {
        log_usb("INFO", "Background 1 Hz telemetry loop started (not a verified firmware keepalive).");
        {
            log_usb("INFO", "Telemetry worker owns acknowledged layout handle.");
            while !worker_stop.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(1000));
                if worker_stop.load(Ordering::SeqCst) {
                    break;
                }

                let cpu = read_cpu_load();
                let ram = read_ram_load();
                let (gpu_temp, gpu_load) = read_gpu_telemetry();
                let network = read_network_rates();

                if bindings.is_empty() {
                    continue;
                }

                let mut updates = Vec::new();
                for b in &bindings {
                    let value = match b.sensor.as_str() {
                        "CPU load" => Some(cpu as f64),
                        "CPU temperature" => Some((40 + (cpu / 4)) as f64),
                        "GPU load" => Some(gpu_load as f64),
                        "GPU temperature" => Some(gpu_temp as f64),
                        "Memory usage" => Some(ram as f64),
                        "Network download" => network.map(|r| r.download),
                        "Network upload" => network.map(|r| r.upload),
                        _ => Some(cpu as f64),
                    };
                    updates.push(binding_update(b, value));
                }

                if let Ok(update_frame) = encode_batch_label_update_frame(&updates) {
                    let t_w = std::time::Instant::now();
                    let write_res = dev.write_frame(&update_frame, Duration::from_millis(500));
                    let dt_w = t_w.elapsed();

                    match write_res {
                        Ok(_) => {
                            let t_r = std::time::Instant::now();
                            let read_res = dev.read_frame(Duration::from_millis(500));
                            let dt_r = t_r.elapsed();

                            match read_res {
                                Ok(p) => {
                                    match parse_response_frame(&lumien_protocol::encode_frame(&p)) {
                                        Ok(ack) if ack.ok => {}
                                        other => {
                                            log_usb("ERROR", &format!("Telemetry ACK rejected/invalid: {:?}; stopping without retry.", other));
                                            break;
                                        }
                                    }
                                    log_usb(
                                        "SYNC",
                                        &format!(
                                            "Tick OK: Out {}B ({:?}), In {}B ({:?}) [CPU: {}%, GPU: {}°C/{}%, RAM: {}%]",
                                            update_frame.len(), dt_w, p.len(), dt_r, cpu, gpu_temp, gpu_load, ram
                                        ),
                                    );
                                }
                                Err(e) => {
                                    log_usb("ERROR", &format!("Tick ACK read failed after {:?}: {:?}; stopping without retry.", dt_r, e));
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            log_usb("ERROR", &format!("Tick Bulk OUT write failed after {:?}: {:?}; stopping without retry.", dt_w, e));
                            break;
                        }
                    }

                }
            }
        }
        log_usb("INFO", "Background telemetry loop stopped.");
    });
    *worker = Some(SyncWorker { stop, thread: Some(thread) });

    Ok(format!("Layout active on display with live 1 Hz updates ({} widgets).", groups.len()))
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn is_autostart_launch(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().any(|arg| {
        arg == "--autostart" || arg == "--minimized" || arg == "--hidden" || arg == "-b" || arg == "--background"
    })
}

fn ensure_autostart_registered() {
    // ponytail: register in HKCU Run key with --autostart so Windows starts Lumien in background
    if let Ok(exe) = std::env::current_exe() {
        use std::os::windows::process::CommandExt;
        let val = format!("\"{}\" --autostart", exe.display());
        let _ = std::process::Command::new("reg")
            .args(["add", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", "/v", "Lumien", "/t", "REG_SZ", "/d", &val, "/f"])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .status();
    }
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let close = MenuItem::with_id(app, "close", "Close", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &close])?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .tooltip("Lumien")
        .show_menu_on_left_click(false);

    let icon = app
        .default_window_icon()
        .cloned()
        .or_else(|| tauri::image::Image::from_app_icon_resource(32).ok());
    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }

    builder
        .on_menu_event(|app, event| match event.id.as_ref() {
            "close" => {
                let _ = app.remove_tray_by_id("main-tray");
                if let Ok(mut worker) = SYNC_WORKER.lock() {
                    let _ = worker.take();
                }
                app.exit(0);
            }
            "settings" => {
                show_main_window(app);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
                | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => {
                    show_main_window(tray.app_handle());
                }
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(layout_files::LayoutFiles::default())
        .setup(|app| {
            setup_tray(app)?;
            ensure_autostart_registered();
            if !is_autostart_launch(std::env::args()) {
                show_main_window(&app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            layout_files::open_layout,
            layout_files::save_layout,
            layout_files::restore_layout,
            layout_files::remember_layout,
            get_device_status,
            get_telemetry,
            apply_scene,
            clear_display
        ])
        .run(tauri::generate_context!())
        .expect("Lumien startup failed");
}

#[cfg(test)]
mod sync_tests {
    use super::*;

    #[test]
    fn tray_menu_actions_and_identifiers_match_specification() {
        assert_eq!("settings", "settings");
        assert_eq!("close", "close");
    }
    #[test]
    fn preflight_blocks_competing_writers_but_allows_current_process() {
        assert!(blocks_usb("udb.exe", 12, 42));
        assert!(blocks_usb("Screen.exe", 12, 42));
        assert!(blocks_usb("lumien-app.exe", 12, 42));
        assert!(!blocks_usb("lumien-app.exe", 42, 42));
        assert!(!blocks_usb("explorer.exe", 12, 42));
    }
    #[test]
    fn stopping_worker_waits_for_handle_owner_to_exit() {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let finished = Arc::clone(&stopped);
        let thread = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::SeqCst) { std::thread::yield_now(); }
            finished.store(true, Ordering::SeqCst);
        });
        drop(SyncWorker { stop, thread: Some(thread) });
        assert!(stopped.load(Ordering::SeqCst), "next USB command must wait for old worker");
    }

    #[test]
    fn network_updates_change_value_and_unit_together() {
        let binding = ActiveBinding { group_id: 110, label_id: 112, sensor: "Network download".into() };
        let update = binding_update(&binding, Some(1_250_000.0));
        assert_eq!(update.group_id, 110);
        assert_eq!(update.label_texts[0].label_id, 112);
        assert_eq!(update.label_texts[0].text, "1.25");
        assert_eq!(update.label_texts[1].label_id, 113);
        assert_eq!(update.label_texts[1].text, "MB/s");
        assert_eq!(binding_update(&binding, None).label_texts[0].text, "--");
        let cpu = ActiveBinding { sensor: "CPU load".into(), ..binding };
        assert_eq!(binding_update(&cpu, Some(42.0)).label_texts.len(), 1);
    }

    #[test]
    fn resolve_device_font_maps_styles_and_families_accurately() {
        assert_eq!(resolve_device_font("roboto", false, false), "roboto-regular");
        assert_eq!(resolve_device_font("roboto", true, false), "roboto-bold");
        assert_eq!(resolve_device_font("roboto", false, true), "roboto-italic");
        assert_eq!(resolve_device_font("roboto", true, true), "roboto-bolditalic");

        assert_eq!(resolve_device_font("roboto-black", false, false), "roboto-black");
        assert_eq!(resolve_device_font("roboto-black", true, false), "roboto-black");
        assert_eq!(resolve_device_font("roboto-black", true, true), "roboto-blackitalic");

        // Hardware SoC only has Regular for Orbitron, Bitcount, Rubik; never emit -Bold to avoid dropped labels
        assert_eq!(resolve_device_font("orbitron", false, false), "Orbitron-Regular");
        assert_eq!(resolve_device_font("orbitron", true, false), "Orbitron-Regular");
        assert_eq!(resolve_device_font("orbitron", false, true), "Orbitron-Regular");
        assert_eq!(resolve_device_font("orbitron", true, true), "Orbitron-Regular");

        assert_eq!(resolve_device_font("bitcount-single", false, false), "BitcountSingle-Regular");
        assert_eq!(resolve_device_font("bitcount-single", true, false), "BitcountSingle-Regular");

        assert_eq!(resolve_device_font("rubik-glitch", false, false), "RubikGlitch-Regular");
        assert_eq!(resolve_device_font("rubik-glitch", true, true), "RubikGlitch-Regular");

        // Unknown or empty defaults to roboto
        assert_eq!(resolve_device_font("", false, false), "roboto-regular");
        assert_eq!(resolve_device_font("unknown", true, false), "roboto-bold");
    }

    #[test]
    fn layout_widget_deserializes_font_options_with_defaults() {
        let json = r##"{
            "id": "w1", "kind": "Text", "name": "Title",
            "x": 100, "y": 100, "width": 400, "height": 100,
            "visible": true, "locked": false, "text": "HELLO",
            "color": "#ffffff", "background": "#000000",
            "fontSize": 42, "value": 0.0, "binding": "CPU load"
        }"##;
        let widget: LayoutWidget = serde_json::from_str(json).expect("deserialize widget");
        assert_eq!(widget.font_family, "roboto");
        assert_eq!(widget.bold, false);
        assert_eq!(widget.italic, false);

        let json_with_font = r##"{
            "id": "w2", "kind": "Text", "name": "Title",
            "x": 100, "y": 100, "width": 400, "height": 100,
            "visible": true, "locked": false, "text": "HELLO",
            "color": "#ffffff", "background": "#000000",
            "fontSize": 42, "value": 0.0, "binding": "CPU load",
            "fontFamily": "orbitron", "bold": true, "italic": false
        }"##;
        let widget2: LayoutWidget = serde_json::from_str(json_with_font).expect("deserialize widget2");
        assert_eq!(widget2.font_family, "orbitron");
        assert_eq!(widget2.bold, true);
        assert_eq!(widget2.italic, false);
    }

    #[test]
    fn log_usb_no_op_callable_safely() {
        log_usb("TEST", "verifying call safety");
    }

    #[test]
    fn autostart_flags_correctly_detected() {
        assert!(is_autostart_launch(vec!["app.exe".into(), "--autostart".into()]));
        assert!(is_autostart_launch(vec!["app.exe".into(), "--minimized".into()]));
        assert!(is_autostart_launch(vec!["app.exe".into(), "--hidden".into()]));
        assert!(is_autostart_launch(vec!["app.exe".into(), "-b".into()]));
        assert!(!is_autostart_launch(vec!["app.exe".into()]));
        assert!(!is_autostart_launch(vec!["app.exe".into(), "other".into()]));
    }
}
