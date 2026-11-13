#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

macro_rules! log_usb {
    ($level:expr, $message:expr $(,)?) => {
        if cfg!(debug_assertions) {
            crate::write_usb_debug($level, $message)
        }
    };
}

mod layout_files;
mod cpu_temperature;
mod monitor_power;
mod youtube;
mod video_gallery;
mod presets;
use presets::PresetItem;
use lumien_telemetry::{NetworkSampler, NetworkRates, format_metric, is_network};

use lumien_device::{enumerate_devices, DisplaySession};
use lumien_protocol::{
    encode_batch_label_update_frame, encode_clear_run_config_frame, encode_run_config_frame,
    parse_response_frame, GroupLabelUpdate, Label, LabelBackground, LabelGroup, LabelTextUpdate,
    TextAlign, CANVAS_HEIGHT, CANVAS_WIDTH,
};
use serde::{Deserialize, Serialize};
use tauri_plugin_dialog::DialogExt;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, LazyLock};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
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
    fn FreeLibrary(module: *mut c_void) -> i32;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    #[allow(dead_code)]
    fn GetLocalTime(lp_system_time: *mut SYSTEMTIME);
}

#[allow(dead_code)]
fn hex_preview(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ")
}

#[inline]
#[allow(unused_variables)]
fn write_usb_debug(_level: &str, _msg: &str) {
    // USB diagnostics go to the debug console.
    #[cfg(debug_assertions)]
    {
        let mut st = std::mem::MaybeUninit::<SYSTEMTIME>::uninit();
        unsafe { GetLocalTime(st.as_mut_ptr()); }
        let st = unsafe { st.assume_init() };
        let line = format!(
            "[{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}] [{:5}] {}\n",
            st.w_year, st.w_month, st.w_day, st.w_hour, st.w_minute, st.w_second, st.w_milliseconds,
            _level, _msg
        );
        print!("{}", line);
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

static GPU_MONITOR: Mutex<(Option<GpuMonitor>, Option<Instant>)> = Mutex::new((None, None));

fn read_gpu_telemetry(temperature: bool, load: bool) -> (u32, u32) {
    let mut lock = GPU_MONITOR.lock().unwrap();
    if lock.0.is_none() && lock.1.is_none_or(|at| at.elapsed() >= Duration::from_secs(60)) {
        lock.1 = Some(Instant::now());
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
                            lock.0 = Some(GpuMonitor {
                                _handle: lib,
                                get_temp,
                                get_util,
                                dev,
                            });
                        } else {
                            let shutdown: Option<unsafe extern "system" fn() -> i32> =
                                std::mem::transmute(GetProcAddress(lib, b"nvmlShutdown\0".as_ptr()));
                            if let Some(shutdown) = shutdown { shutdown(); }
                        }
                    }
                }
                if lock.0.is_none() { FreeLibrary(lib); }
            }
        }
    }

    if let Some(ref gpu) = lock.0 {
        let mut temp = 0u32;
        let mut util = NvmlUtilization { gpu: 0, memory: 0 };
        if let Some(f_temp) = gpu.get_temp.filter(|_| temperature) {
            unsafe { f_temp(gpu.dev, 0, &mut temp); }
        }
        if let Some(f_util) = gpu.get_util.filter(|_| load) {
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

struct CpuSample {
    at: Instant,
    idle: u64,
    total: u64,
    load: u32,
}

static LAST_CPU_TIMES: Mutex<Option<CpuSample>> = Mutex::new(None);
static NETWORK: LazyLock<Mutex<NetworkSampler>> = LazyLock::new(|| Mutex::new(NetworkSampler::default()));

fn read_network_rates() -> Option<NetworkRates> {
    NETWORK.lock().ok()?.sample().ok().flatten()
}

fn read_cpu_load() -> u32 {
    let mut last = LAST_CPU_TIMES.lock().unwrap();
    let now = Instant::now();
    if let Some(sample) = &*last {
        if now.duration_since(sample.at) < Duration::from_millis(900) {
            return sample.load;
        }
    }
    let mut idle = FILETIME { dw_low_date_time: 0, dw_high_date_time: 0 };
    let mut kernel = FILETIME { dw_low_date_time: 0, dw_high_date_time: 0 };
    let mut user = FILETIME { dw_low_date_time: 0, dw_high_date_time: 0 };

    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
        return 0;
    }

    let idle = filetime_to_u64(&idle);
    let total = filetime_to_u64(&kernel) + filetime_to_u64(&user);
    let load = last.as_ref().map_or(0, |sample| {
        let elapsed = total.saturating_sub(sample.total);
        let busy = elapsed.saturating_sub(idle.saturating_sub(sample.idle));
        (busy * 100).checked_div(elapsed).unwrap_or(0) as u32
    });
    *last = Some(CpuSample { at: now, idle, total, load });
    load
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
    thread: Option<std::thread::JoinHandle<Option<DisplaySession>>>,
    restore: Arc<Restore>,
}

impl SyncWorker {
    fn start(dev: Option<DisplaySession>, restore: Arc<Restore>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker_restore = Arc::clone(&restore);
        let thread = std::thread::spawn(move || run_sync_worker(&worker_stop, dev, &worker_restore));
        Self { stop, thread: Some(thread), restore }
    }

    fn stop(&mut self) -> Option<DisplaySession> {
        monitor_power::stop(&self.stop);
        self.thread.take()?.join().ok().flatten()
    }
}

impl Drop for SyncWorker {
    fn drop(&mut self) {
        drop(self.stop());
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
    #[serde(default = "default_bg_type", rename = "bgType", alias = "bg_type")]
    bg_type: String,
    #[serde(default = "default_bg_color", rename = "bgColor", alias = "bg_color")]
    bg_color: String,
}

fn default_bg_type() -> String {
    "preset".to_string()
}

fn default_bg_color() -> String {
    "#000000".to_string()
}

fn parse_hex_color(hex: &str) -> u32 {
    let clean = hex.trim_start_matches('#');
    u32::from_str_radix(clean, 16).unwrap_or(0xFFFFFF)
}

fn blocks_usb(image: &[u16], pid: u32, own_pid: u32) -> bool {
    pid != own_pid && ["KANALI.exe", "Screen.exe", "UDB.exe", "lumien-app.exe", "lumien-cli.exe"]
        .iter().any(|name| name.len() == image.len() && name.bytes().zip(image).all(|(a, &b)| {
            u8::try_from(b).is_ok_and(|b| a.eq_ignore_ascii_case(&b))
        }))
}

#[repr(C)]
struct ProcessEntry {
    size: u32,
    usage: u32,
    pid: u32,
    heap: usize,
    module: u32,
    threads: u32,
    parent: u32,
    priority: i32,
    flags: u32,
    image: [u16; 260],
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> *mut c_void;
    fn Process32FirstW(snapshot: *mut c_void, entry: *mut ProcessEntry) -> i32;
    fn Process32NextW(snapshot: *mut c_void, entry: *mut ProcessEntry) -> i32;
    fn GetLastError() -> u32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

fn ensure_usb_channel_available() -> Result<(), String> {
    const TH32CS_SNAPPROCESS: u32 = 2;
    const ERROR_NO_MORE_FILES: u32 = 18;
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == lumien_device::INVALID_HANDLE_VALUE {
        return Err(format!("Cannot check USB prerequisites: {}", std::io::Error::last_os_error()));
    }
    let mut entry: ProcessEntry = unsafe { std::mem::zeroed() };
    entry.size = std::mem::size_of::<ProcessEntry>() as u32;
    let result = (|| {
        if unsafe { Process32FirstW(snapshot, &mut entry) } == 0 {
            return Err(format!("Cannot check USB prerequisites: {}", std::io::Error::last_os_error()));
        }
        loop {
            let len = entry.image.iter().position(|c| *c == 0).unwrap_or(entry.image.len());
            let image = &entry.image[..len];
            if blocks_usb(image, entry.pid, std::process::id()) {
                let image = String::from_utf16_lossy(image);
                return Err(format!("Close {image} (PID {}) to avoid concurrent USB traffic", entry.pid));
            }
            if unsafe { Process32NextW(snapshot, &mut entry) } == 0 {
                return if unsafe { GetLastError() } == ERROR_NO_MORE_FILES {
                    Ok(())
                } else {
                    Err(format!("Cannot check USB prerequisites: {}", std::io::Error::last_os_error()))
                };
            }
        }
    })();
    unsafe { CloseHandle(snapshot); }
    result
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Sensor {
    CpuLoad,
    CpuTemp,
    Ram,
    GpuLoad,
    GpuTemp,
    NetworkDownload,
    NetworkUpload,
}

impl Sensor {
    fn for_binding(binding: &str) -> Self {
        match binding {
            "CPU temperature" => Self::CpuTemp,
            "GPU load" => Self::GpuLoad,
            "GPU temperature" => Self::GpuTemp,
            "Memory usage" => Self::Ram,
            "Network download" => Self::NetworkDownload,
            "Network upload" => Self::NetworkUpload,
            _ => Self::CpuLoad,
        }
    }
}

fn read_sensors(keys: &[Sensor]) -> HashMap<Sensor, Option<f64>> {
    use Sensor::*;
    let gpu_temp = keys.contains(&GpuTemp);
    let gpu_load = keys.contains(&GpuLoad);
    let gpu = (gpu_temp || gpu_load).then(|| read_gpu_telemetry(gpu_temp, gpu_load));
    let network = (keys.contains(&NetworkDownload) || keys.contains(&NetworkUpload))
        .then(read_network_rates).flatten();
    let mut readings = HashMap::with_capacity(keys.len());
    for &key in keys {
        readings.entry(key).or_insert_with(|| match key {
            CpuLoad => Some(read_cpu_load() as f64),
            CpuTemp => cpu_temperature::read(),
            Ram => Some(read_ram_load() as f64),
            GpuTemp => gpu.map(|(temp, _)| temp as f64),
            GpuLoad => gpu.map(|(_, load)| load as f64),
            NetworkDownload => network.map(|r| r.download),
            NetworkUpload => network.map(|r| r.upload),
        });
    }
    readings
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Telemetry {
    readings: HashMap<Sensor, Option<f64>>,
    cpu_temp_status: Option<String>,
}

#[tauri::command]
fn get_telemetry(keys: Vec<Sensor>) -> Telemetry {
    let readings = read_sensors(&keys);
    let cpu_temp_status = keys.contains(&Sensor::CpuTemp).then(cpu_temperature::status).flatten();
    Telemetry { readings, cpu_temp_status }
}

#[tauri::command]
fn editor_active(window: tauri::WebviewWindow) -> bool {
    let shown = window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false);
    shown && monitor_power::state().on()
}

const EDITOR_ACTIVITY_CHANGED: &str = "editor-activity-changed";
static EDITOR_SHOWN: AtomicBool = AtomicBool::new(false);

fn editor_visibility_changed(app: &tauri::AppHandle, shown: bool) {
    if EDITOR_SHOWN.swap(shown, Ordering::Relaxed) != shown {
        let _ = app.emit(EDITOR_ACTIVITY_CHANGED, ());
    }
}

#[tauri::command]
fn warm_cpu_temperature() {
    let _ = cpu_temperature::read();
}

fn pause_worker(worker: &mut Option<SyncWorker>) -> Result<(Option<DisplaySession>, Arc<Restore>), String> {
    let mut active = worker.take().ok_or("Display worker is shutting down")?;
    let dev = active.stop();
    Ok((dev, Arc::clone(&active.restore)))
}

#[tauri::command(async)]
fn clear_display() -> Result<String, String> {
    ensure_usb_channel_available()?;
    let mut worker = SYNC_WORKER.lock().map_err(|_| "USB worker lock poisoned")?;
    let frame = encode_clear_run_config_frame();
    let (mut dev, previous) = pause_worker(&mut worker)?;
    match send_layout(&mut dev, &frame) {
        Ok(()) => {
            let restore = Arc::new(Restore {
                frame, config_dir: previous.config_dir.clone(), bindings: Vec::new(), recover_content: true,
            });
            *worker = Some(SyncWorker::start(dev, restore));
            Ok("Display overlay cleared successfully.".into())
        }
        Err(error) => {
            let e = match error {
                Transfer::Failed(e) => { dev = None; e }
                Transfer::Rejected(e) => e,
            };
            *worker = Some(SyncWorker::start(dev, previous));
            Err(e)
        }
    }
}

#[tauri::command]
fn list_presets(app: tauri::AppHandle) -> Result<Vec<PresetItem>, String> {
    let config_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(presets::get_all_presets(&config_dir))
}

#[tauri::command]
fn import_kanali_presets(app: tauri::AppHandle) -> Result<Vec<PresetItem>, String> {
    let config_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    presets::scan_and_import_kanali(&config_dir)
}

#[tauri::command]
fn get_active_preset(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let config_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(presets::read_active_preset(&config_dir).filter(|filename| {
        layout_files::is_valid_media_name(filename)
            && (!video_gallery::is_video_filename(filename)
                || video_gallery::resolve_filename(&config_dir.join("youtube"), filename).is_ok())
    }))
}



#[tauri::command]
async fn list_downloaded_videos(app: tauri::AppHandle) -> Result<Vec<video_gallery::Video>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = app.path().app_config_dir().map_err(|e| e.to_string())?;
        video_gallery::list(&config.join("youtube"), presets::find_ffmpeg(&config).as_deref())
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn import_mp4(window: tauri::WebviewWindow) -> Result<bool, String> {
    let config = window.app_handle().path().app_config_dir().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let selected = window.dialog().file().set_parent(&window)
            .set_title("Import MP4 background").add_filter("MP4 video", &["mp4"]).blocking_pick_file();
        let Some(selected) = selected else { return Ok(false) };
        let source = selected.into_path().map_err(|e| e.to_string())?;
        let ffmpeg = presets::find_ffmpeg(&config).ok_or("FFmpeg is required to import MP4 backgrounds")?;
        video_gallery::import_mp4(&config.join("youtube"), &source, &ffmpeg)?;
        Ok(true)
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn play_downloaded_video(app: tauri::AppHandle, id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = app.path().app_config_dir().map_err(|e| e.to_string())?;
        let path = video_gallery::resolve(&config.join("youtube"), &id)?;
        play_video_background(&config, &path)
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn remove_downloaded_video(app: tauri::AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = app.path().app_config_dir().map_err(|e| e.to_string())?;
        video_gallery::remove(&config.join("youtube"), &id)
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn play_youtube(app: tauri::AppHandle, url: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        static IMPORT: Mutex<()> = Mutex::new(());
        let _import = IMPORT.try_lock().map_err(|_| "A YouTube import is already running")?;
        let config = app.path().app_config_dir().map_err(|e| e.to_string())?;
        youtube::video_id(&url)?;
        let downloader = presets::find_tool("yt-dlp.exe", &config)?;
        let ffmpeg = presets::find_ffmpeg(&config).ok_or("FFmpeg is required to convert YouTube videos")?;
        let path = youtube::prepare(&url, &config.join("youtube"), &downloader, &ffmpeg)?;
        play_video_background(&config, &path)
    }).await.map_err(|e| e.to_string())?
}

fn play_video_background(config: &std::path::Path, path: &std::path::Path) -> Result<String, String> {
    let (filename, size, data) = youtube::open_prepared(path)?;
    change_background(config, filename, |dev| youtube::play_prepared(dev, filename, size, data, false))?;
    Ok(filename.into())
}

fn restore_video_background(config: &std::path::Path, dev: &mut DisplaySession, filename: &str) -> Result<(), String> {
    let path = video_gallery::resolve_filename(&config.join("youtube"), filename)?;
    let (filename, size, data) = youtube::open_prepared(&path)?;
    youtube::play_prepared(dev, filename, size, data, false)
}

fn change_background(
    config: &std::path::Path,
    filename: &str,
    change: impl FnOnce(&mut DisplaySession) -> Result<(), String>,
) -> Result<(), String> {
    ensure_usb_channel_available()?;
    let mut worker = SYNC_WORKER.lock().map_err(|_| "USB worker lock poisoned")?;
    let (mut dev, previous) = pause_worker(&mut worker)?;
    let mut activated = false;
    let result = (|| {
        ensure_usb_channel_available()?;
        if dev.is_none() { dev = Some(open_session("change_background")?); }
        let session = dev.as_mut().unwrap();
        if let Err(e) = change(session) {
            dev = None;
            return Err(e);
        }
        activated = true;
        if let Err(e) = presets::save_active_preset(config, filename) {
            log_usb!("WARN", &format!("Could not remember active background: {e}"));
        }
        if !previous.frame.is_empty() {
            send_layout(&mut dev, &previous.frame).map_err(|error| {
                if matches!(error, Transfer::Failed(_)) { dev = None; }
                let (Transfer::Failed(e) | Transfer::Rejected(e)) = error;
                format!("Background switched, but restoring widgets failed: {e}")
            })?;
        }
        Ok(())
    })();
    let restore = if activated && !previous.recover_content {
        Arc::new(Restore { recover_content: true, ..(*previous).clone() })
    } else { previous };
    *worker = Some(SyncWorker::start(dev, restore));
    result
}

/// Sends WorkConfigPb for an already-uploaded media file and waits for the device ACK.
fn activate_media(dev: &mut DisplaySession, file_name: &str) -> Result<(), String> {
    let frame = lumien_protocol::encode_work_config_frame(file_name);
    let t0 = std::time::Instant::now();
    dev.write_frame(&frame, Duration::from_millis(2000)).map_err(|e| {
        log_usb!("ERROR", &format!("Bulk OUT work_config frame failed: {:?}", e));
        format!("Write failed: {:?}", e)
    })?;
    let payload = dev.read_frame(Duration::from_millis(2000)).map_err(|e| {
        log_usb!("ERROR", &format!("Bulk IN WorkConfig ACK failed: {:?}", e));
        format!("Read failed: {:?}", e)
    })?;
    log_usb!("READ", &format!("WorkConfig ACK ({} bytes) in {:?} | Hex: {}", payload.len(), t0.elapsed(),
        hex_preview(&payload[..std::cmp::min(16, payload.len())])));
    let ack = parse_response_frame(&lumien_protocol::encode_frame(&payload)).map_err(|e| format!("Invalid ACK: {:?}", e))?;
    if !ack.ok {
        log_usb!("WARN", "Device rejected work config command.");
        return Err("Device rejected work config command.".to_string());
    }
    Ok(())
}

fn open_session(command: &str) -> Result<DisplaySession, String> {
    DisplaySession::open_first().map_err(|e| {
        log_usb!("ERROR", &format!("open_first failed during {command}: {:?}", e));
        format!("Failed to open device: {:?}", e)
    })
}

// async: keep USB transfers (and FFmpeg encodes) off the main/UI thread.
#[tauri::command(async)]
fn set_display_background(app: tauri::AppHandle, file_name: String) -> Result<String, String> {
    if !layout_files::is_valid_media_name(&file_name) {
        return Err("Invalid background media file".into());
    }
    let config = app.path().app_config_dir().map_err(|e| e.to_string())?;
    if video_gallery::is_video_filename(&file_name) {
        change_background(&config, &file_name, |dev| restore_video_background(&config, dev, &file_name))?;
        return Ok("Video playing as background.".into());
    }
    change_background(&config, &file_name, |dev| activate_media(dev, &file_name))?;
    log_usb!("INFO", "Background media switched successfully.");
    Ok(format!("Background set to {}", file_name))
}

#[tauri::command(async)]
fn set_solid_color_background(app: tauri::AppHandle, color: String) -> Result<String, String> {
    let config_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    let (file_name, data) = presets::get_or_create_solid_color_video(&config_dir, &color)?;

    log_usb!("INFO", &format!("Executing set_solid_color_background: {} ({} bytes)", file_name, data.len()));

    change_background(&config_dir, &file_name, |dev| {
        dev.transmit_file(&file_name, &data, Duration::from_millis(2000))
            .map_err(|e| format!("Transmit solid background failed: {e:?}"))?;
        activate_media(dev, &file_name)
    })?;
    log_usb!("INFO", &format!("Solid color background ({}) activated successfully.", color));
    Ok(format!("Solid background ({}) activated on display.", color))
}

// async: stopping a worker mid-restore and the USB transaction must not freeze the UI thread.
#[tauri::command(async)]
fn apply_scene(app: tauri::AppHandle, scene_json: String) -> Result<String, String> {
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
            let value = if w.binding == "CPU temperature" { cpu_temperature::read() } else { Some(w.value) };
            let (value_text, unit) = format_metric(&w.binding, value);
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

    log_usb!("INFO", &format!("Encoding scene '{}' ({} widgets)...", scene.name, scene.widgets.len()));
    let frame = encode_run_config_frame(&groups)
        .map_err(|e| {
            log_usb!("ERROR", &format!("Geometry/Encoding error: {:?}", e));
            format!("Geometry/Encoding error: {:?}", e)
        })?;

    let mut worker = SYNC_WORKER.lock().map_err(|_| "USB worker lock poisoned")?;
    let (mut dev, previous) = pause_worker(&mut worker)?;

    // A transport failure (display still booting, asleep, or unplugged) is not final: the worker keeps the
    // layout and applies it as soon as the display answers. Only an explicit device rejection is returned.
    let restore = Arc::new(Restore { frame, config_dir: app.path().app_config_dir().ok(), bindings, recover_content: true });
    let message = match send_layout(&mut dev, &restore.frame) {
        Ok(()) => format!("Layout active on display with live 1 Hz updates ({} widgets).", groups.len()),
        Err(Transfer::Rejected(e)) => {
            *worker = Some(SyncWorker::start(dev, previous));
            return Err(e);
        }
        Err(Transfer::Failed(e)) => {
            log_usb!("WARN", &format!("Display not ready ({e}); layout queued for automatic apply."));
            dev = None;
            "Display is not responding yet. Lumien will apply this layout automatically as soon as it is ready.".to_string()
        }
    };

    *worker = Some(SyncWorker::start(dev, restore));
    Ok(message)
}

enum Transfer { Failed(String), Rejected(String) }

fn send_layout(dev: &mut Option<DisplaySession>, frame: &[u8]) -> Result<(), Transfer> {
    ensure_usb_channel_available().map_err(Transfer::Rejected)?;
    if dev.is_none() { *dev = Some(open_session("layout").map_err(Transfer::Failed)?); }
    send_run_config(dev.as_mut().unwrap(), frame)
}

/// Sends a RunConfig layout frame and waits for its ACK.
fn send_run_config(dev: &mut DisplaySession, frame: &[u8]) -> Result<(), Transfer> {
    let t0 = std::time::Instant::now();
    dev.write_frame(frame, Duration::from_millis(2000)).map_err(|e| {
        log_usb!("ERROR", &format!("Bulk OUT RunConfig failed: {:?}", e));
        Transfer::Failed(format!("Write failed: {:?}", e))
    })?;
    let payload = dev.read_frame(Duration::from_millis(2000)).map_err(|e| {
        log_usb!("ERROR", &format!("Bulk IN RunConfig ACK failed: {:?}", e));
        Transfer::Failed(format!("Read failed: {:?}", e))
    })?;
    log_usb!("READ", &format!("RunConfig ({} bytes) ACK ({} bytes) in {:?} | Hex: {}", frame.len(), payload.len(), t0.elapsed(),
        hex_preview(&payload[..std::cmp::min(16, payload.len())])));
    let ack = parse_response_frame(&lumien_protocol::encode_frame(&payload))
        .map_err(|e| Transfer::Failed(format!("Invalid response frame: {:?}", e)))?;
    if !ack.ok {
        log_usb!("WARN", "Device rejected layout frame.");
        return Err(Transfer::Rejected("Device rejected layout frame.".to_string()));
    }
    Ok(())
}

/// What the display must show again after it resets: the acknowledged layout and the saved background.
#[derive(Clone)]
struct Restore {
    frame: Vec<u8>,
    config_dir: Option<std::path::PathBuf>,
    bindings: Vec<ActiveBinding>,
    recover_content: bool,
}

/// Restores the active background and layout on the worker's current session.
fn restore_display(dev: &mut DisplaySession, restore: &Restore) -> Result<(), String> {
    ensure_usb_channel_available()?;
    if let Some(config) = restore.config_dir.as_deref() {
        let media = presets::load_active_preset(config);
        if layout_files::is_valid_media_name(&media) {
            let background = if video_gallery::is_video_filename(&media) {
                restore_video_background(config, dev, &media)
            } else {
                activate_media(dev, &media)
            };
            if let Err(e) = background {
                log_usb!("WARN", &format!("Restore: background {media} not re-activated: {e}"));
            }
        }
    }
    if restore.frame.is_empty() { return Ok(()); }
    ensure_usb_channel_available()?;
    match send_run_config(dev, &restore.frame) {
        Ok(()) => Ok(()),
        Err(Transfer::Failed(e) | Transfer::Rejected(e)) => Err(e),
    }
}

/// Owns the display handle for an applied layout: pushes 1 Hz telemetry, and when the display stops
/// answering or the PC resumes from sleep, reconnects and restores background + layout with backoff.
fn run_sync_worker(stop: &AtomicBool, mut dev: Option<DisplaySession>, restore: &Restore) -> Option<DisplaySession> {
    const MAX_BACKOFF: Duration = Duration::from_secs(10);
    log_usb!("INFO", "Layout worker started (1 Hz telemetry, automatic display recovery).");
    let mut seen_resumes = monitor_power::state().resumes;
    let mut synced = None;
    let mut restore_on_wake = dev.is_none() && restore.recover_content;
    let mut attempted_off = None;
    let mut backoff = Duration::from_secs(1);
    let mut failures = 0u32;
    let keys: Vec<_> = restore.bindings.iter().map(|b| Sensor::for_binding(&b.sensor)).collect();
    while !stop.load(Ordering::SeqCst) {
        let state = monitor_power::state();
        if state.resumes != seen_resumes {
            seen_resumes = state.resumes;
            dev = None;
            synced = None;
            restore_on_wake = restore.recover_content;
            backoff = Duration::from_secs(3);
        }
        let on = state.on();
        if dev.is_none() {
            let needs_wake = restore.config_dir.as_deref().is_some_and(monitor_power::needs_wake);
            if (!on && attempted_off == Some(state.transitions)) || (on && !restore_on_wake && !needs_wake) {
                monitor_power::wait(state, None, stop);
                continue;
            }
            if on && !monitor_power::wait(state, Some(backoff), stop) { continue; }
            if !on { attempted_off = Some(state.transitions); }
            match ensure_usb_channel_available().and_then(|_| open_session("monitor power")) {
                Ok(d) => {
                    log_usb!("INFO", &format!("Display connection opened after {failures} failed attempt(s)."));
                    dev = Some(d);
                    synced = None;
                }
                Err(e) => {
                    failures += 1;
                    if failures == 1 || failures % 30 == 0 { log_usb!("WARN", &format!("Display restore attempt {failures} failed: {e}")); }
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                }
            }
            continue;
        }

        if synced != Some(on) || (on && restore_on_wake) {
            let session = dev.as_mut().expect("display connected");
            let mut result = Ok(true);
            if on && restore_on_wake {
                result = restore_display(session, restore).map(|_| true);
                if result.is_ok() { restore_on_wake = false; }
            }
            if let Some(config) = restore.config_dir.as_deref() {
                result = result.and_then(|_| monitor_power::sync(session, config, on));
            }
            match result {
                Ok(true) => {}
                Ok(false) => continue,
                Err(e) => {
                    log_usb!("WARN", &format!("Monitor power sync failed: {e}"));
                    dev = None;
                    if !on { attempted_off = Some(state.transitions); }
                    restore_on_wake = restore.recover_content;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                    continue;
                }
            }
            synced = Some(on);
            failures = 0;
            backoff = Duration::from_secs(1);
            if !restore.recover_content {
                dev = None;
                if !on { attempted_off = Some(state.transitions); }
            }
            continue;
        }

        let telemetry = on && !restore.bindings.is_empty();
        if !monitor_power::wait(state, telemetry.then_some(Duration::from_secs(1)), stop) { continue; }

        let readings = read_sensors(&keys);
        let updates: Vec<_> = restore.bindings.iter().zip(&keys)
            .map(|(binding, key)| binding_update(binding, readings[key]))
            .collect();
        let Ok(update_frame) = encode_batch_label_update_frame(&updates) else { continue };

        if let Err(e) = ensure_usb_channel_available() {
            log_usb!("WARN", &format!("Telemetry paused: {e}"));
            dev = None;
            restore_on_wake = restore.recover_content;
            continue;
        }
        if stop.load(Ordering::SeqCst) || monitor_power::state() != state { continue; }
        let session = dev.as_mut().expect("checked above");
        let result = session.write_frame(&update_frame, Duration::from_millis(500))
            .map_err(|e| format!("write: {:?}", e))
            .and_then(|_| session.read_frame(Duration::from_millis(500)).map_err(|e| format!("ACK read: {:?}", e)))
            .and_then(|p| match parse_response_frame(&lumien_protocol::encode_frame(&p)) {
                Ok(ack) if ack.ok => Ok(p.len()),
                other => Err(format!("ACK rejected/invalid: {:?}", other)),
            });
        match result {
            Ok(n) => log_usb!("SYNC", &format!("Tick OK: Out {}B, In {}B {readings:?}", update_frame.len(), n)),
            Err(e) => {
                // A rejected update usually means the display lost the layout (reset); restore it.
                log_usb!("WARN", &format!("Telemetry tick failed ({e}); reconnecting and restoring the display."));
                dev = None;
                restore_on_wake = true;
                backoff = Duration::from_secs(1);
            }
        }
    }
    log_usb!("INFO", "Layout worker stopped.");
    dev
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        editor_visibility_changed(app, window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false));
    }
}

fn is_autostart_launch(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().any(|arg| {
        arg == "--autostart" || arg == "--minimized" || arg == "--hidden" || arg == "-b" || arg == "--background"
    })
}

#[cfg_attr(debug_assertions, allow(dead_code))]
const AUTOSTART_TASK: &str = "Lumien";

#[cfg_attr(debug_assertions, allow(dead_code))]
#[link(name = "shell32")]
extern "system" {
    fn IsUserAnAdmin() -> i32;
}

#[cfg_attr(debug_assertions, allow(dead_code))]
fn hidden(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut command = std::process::Command::new(program);
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    command
}

#[cfg_attr(debug_assertions, allow(dead_code))]
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Task Scheduler XML for an at-sign-in launch with the user's highest privileges. A Run key launch is
/// never elevated, and LibreHardwareMonitor needs administrator rights for CPU temperature sensors.
#[cfg_attr(debug_assertions, allow(dead_code))]
fn autostart_task_xml(exe: &std::path::Path, user: &str) -> String {
    format!(r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Starts Lumien at sign-in with access to CPU temperature sensors.</Description></RegistrationInfo>
  <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId><Delay>PT10S</Delay></LogonTrigger></Triggers>
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author"><Exec><Command>{exe}</Command><Arguments>--autostart</Arguments></Exec></Actions>
</Task>"#, user = xml_escape(user), exe = xml_escape(&exe.display().to_string()))
}

/// Registers (or refreshes) the elevated sign-in task. Requires an elevated process.
#[cfg_attr(debug_assertions, allow(dead_code))]
fn register_autostart_task(exe: &std::path::Path) -> Result<(), String> {
    let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(domain), Ok(name)) => format!("{domain}\\{name}"),
        _ => return Err("Cannot determine the signed-in user".into()),
    };
    // schtasks expects the XML file in UTF-16 with a byte-order mark.
    let bytes: Vec<u8> = std::iter::once(0xFEFFu16).chain(autostart_task_xml(exe, &user).encode_utf16()).flat_map(u16::to_le_bytes).collect();
    let path = std::env::temp_dir().join(format!("lumien-autostart-{}.xml", std::process::id()));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    let status = hidden("schtasks").args(["/Create", "/F", "/TN", AUTOSTART_TASK, "/XML"]).arg(&path).status();
    let _ = std::fs::remove_file(&path);
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("schtasks exited with {s}")),
        Err(e) => Err(e.to_string()),
    }
}

fn ensure_autostart_registered() {
    // Only register release / production builds to autostart, never debug development builds
    #[cfg(not(debug_assertions))]
    {
        const RUN_KEY: &str = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run";
        let Ok(exe) = std::env::current_exe() else { return };
        // Elevated: switch to the elevated sign-in task and drop the Run entry so only one instance starts.
        if unsafe { IsUserAnAdmin() } != 0 {
            match register_autostart_task(&exe) {
                Ok(()) => {
                    let _ = hidden("reg").args(["delete", RUN_KEY, "/v", "Lumien", "/f"]).status();
                    return;
                }
                Err(e) => log_usb!("WARN", &format!("Elevated autostart task not registered: {e}")),
            }
        }
        // Not elevated: keep an existing elevated task; otherwise fall back to the (unelevated) Run entry.
        if hidden("schtasks").args(["/Query", "/TN", AUTOSTART_TASK]).output().is_ok_and(|o| o.status.success()) {
            return;
        }
        let val = format!("\"{}\" --autostart", exe.display());
        let _ = hidden("reg").args(["add", RUN_KEY, "/v", "Lumien", "/t", "REG_SZ", "/d", &val, "/f"]).status();
    }
    #[cfg(debug_assertions)]
    {
        // In debug builds, remove any stale debug registration from registry so Windows boot doesn't launch unhosted debug binaries
        use std::os::windows::process::CommandExt;
        if let Ok(output) = std::process::Command::new("reg")
            .args(["query", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", "/v", "Lumien"])
            .creation_flags(0x08000000)
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            if text.contains("target\\debug") {
                let _ = std::process::Command::new("reg")
                    .args(["delete", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", "/v", "Lumien", "/f"])
                    .creation_flags(0x08000000)
                    .status();
            }
        }
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
            monitor_power::watch();
            let app_handle = app.handle().clone();
            let mut previous = monitor_power::state();
            std::thread::spawn(move || {
                let stop = AtomicBool::new(false);
                loop {
                    monitor_power::wait(previous, None, &stop);
                    let current = monitor_power::state();
                    if current.on() != previous.on() {
                        let _ = app_handle.emit(EDITOR_ACTIVITY_CHANGED, ());
                    }
                    previous = current;
                }
            });
            if let Ok(mut worker) = SYNC_WORKER.lock() {
                let restore = Restore {
                    frame: Vec::new(), config_dir: app.path().app_config_dir().ok(),
                    bindings: Vec::new(), recover_content: false,
                };
                *worker = Some(SyncWorker::start(None, Arc::new(restore)));
            }
            if !is_autostart_launch(std::env::args()) {
                show_main_window(&app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                editor_visibility_changed(window.app_handle(), window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false));
            }
            if matches!(event, tauri::WindowEvent::Focused(_) | tauri::WindowEvent::Resized(_)) {
                editor_visibility_changed(window.app_handle(), window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false));
            }
        })
        .invoke_handler(tauri::generate_handler![
            layout_files::list_layouts,
            layout_files::load_layout,
            layout_files::save_layout,
            layout_files::restore_layout,
            layout_files::remember_layout,
            get_device_status,
            get_telemetry,
            editor_active,
            warm_cpu_temperature,
            apply_scene,
            clear_display,
            list_presets,
            import_kanali_presets,
            get_active_preset,
            set_display_background,
            play_youtube,
            import_mp4,
            list_downloaded_videos,
            play_downloaded_video,
            remove_downloaded_video,
            set_solid_color_background
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
        let blocks = |name: &str, pid| blocks_usb(&name.encode_utf16().collect::<Vec<_>>(), pid, 42);
        assert!(blocks("udb.exe", 12));
        assert!(blocks("Screen.exe", 12));
        assert!(blocks("lumien-app.exe", 12));
        assert!(!blocks("lumien-app.exe", 42));
        assert!(!blocks("explorer.exe", 12));
        assert!(!blocks("ＵDB.exe", 12));
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
            None
        });
        let restore = Arc::new(Restore { frame: vec![], config_dir: None, bindings: vec![], recover_content: false });
        drop(SyncWorker { stop, thread: Some(thread), restore });
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
        log_usb!("TEST", "verifying call safety");
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
