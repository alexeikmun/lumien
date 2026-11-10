#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod layout_files;

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
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
    fn GetLocalTime(lp_system_time: *mut SYSTEMTIME);
}

fn hex_preview(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ")
}

fn log_usb(level: &str, msg: &str) {
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
    value: u32,
    binding: String,
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
fn get_telemetry() -> Result<HashMap<String, u32>, String> {
    let mut map = HashMap::new();
    let cpu_load = read_cpu_load();
    let ram_load = read_ram_load();
    let (gpu_temp, gpu_load) = read_gpu_telemetry();

    map.insert("cpu_load".to_string(), cpu_load);
    map.insert("cpu_temp".to_string(), 40 + (cpu_load / 4));
    map.insert("ram".to_string(), ram_load);
    map.insert("gpu_load".to_string(), gpu_load);
    map.insert("gpu_temp".to_string(), gpu_temp);
    // Backward compatibility keys
    map.insert("cpu".to_string(), cpu_load);
    map.insert("gpu".to_string(), gpu_load);
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
        if w.kind == "Metric value" {
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
                text_font: "roboto-regular".to_string(),
                text_size: 110,
                text_color: color,
                text: w.value.to_string(),
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
                text_size: 80,
                text_color: color,
                text: if w.binding.to_lowercase().contains("temp") { "°C".to_string() } else { "%".to_string() },
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
                text_font: "roboto-regular".to_string(),
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

                if bindings.is_empty() {
                    continue;
                }

                let mut updates = Vec::new();
                for b in &bindings {
                    let val_str = match b.sensor.as_str() {
                        "CPU load" => cpu.to_string(),
                        "CPU temperature" => (40 + (cpu / 4)).to_string(),
                        "GPU load" => gpu_load.to_string(),
                        "GPU temperature" => gpu_temp.to_string(),
                        "Memory usage" => ram.to_string(),
                        _ => cpu.to_string(),
                    };
                    updates.push(GroupLabelUpdate {
                        group_id: b.group_id,
                        label_texts: vec![LabelTextUpdate {
                            label_id: b.label_id,
                            text: val_str,
                        }],
                    });
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

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(layout_files::LayoutFiles::default())
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
}
