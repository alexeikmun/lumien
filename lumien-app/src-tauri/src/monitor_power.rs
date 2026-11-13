use lumien_device::DisplaySession;
use lumien_protocol::DisplayPower;
use std::{
    ffi::c_void,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Condvar, Mutex,
    },
    time::Duration,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct State {
    monitor_on: bool,
    suspended: bool,
    display_reported: bool,
    pub resumes: u64,
    pub transitions: u64,
}

impl State {
    pub fn on(self) -> bool {
        self.monitor_on && !self.suspended
    }
}

static STATE: Mutex<State> = Mutex::new(State {
    monitor_on: true,
    suspended: false,
    display_reported: false,
    resumes: 0,
    transitions: 0,
});
static CHANGED: Condvar = Condvar::new();
const WAKE_MARKER: &str = "monitor-power-restore";

pub fn state() -> State {
    *STATE.lock().unwrap()
}

/// Returns true only when the deadline elapsed with no power or stop event.
pub fn wait(previous: State, timeout: Option<Duration>, stop: &AtomicBool) -> bool {
    let state = STATE.lock().unwrap();
    let pending = |current: &mut State| *current == previous && !stop.load(Ordering::SeqCst);
    if let Some(timeout) = timeout {
        let (state, result) = CHANGED.wait_timeout_while(state, timeout, pending).unwrap();
        result.timed_out() && *state == previous && !stop.load(Ordering::SeqCst)
    } else {
        drop(CHANGED.wait_while(state, pending).unwrap());
        false
    }
}

pub fn stop(flag: &AtomicBool) {
    let _state = STATE.lock().unwrap();
    flag.store(true, Ordering::SeqCst);
    CHANGED.notify_all();
}

pub fn needs_wake(config: &Path) -> bool {
    config.join(WAKE_MARKER).exists()
}

pub fn sync(dev: &mut DisplaySession, config: &Path, on: bool) -> Result<bool, String> {
    let path = config.join(WAKE_MARKER);
    let ours = match std::fs::metadata(&path) {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(format!("Read display power recovery state: {e}")),
    };
    if on && !ours {
        return Ok(true);
    }
    crate::ensure_usb_channel_available()?;
    let current = dev.display_power()?;
    if state().on() != on {
        return Ok(false);
    }
    if current.enabled != on {
        crate::ensure_usb_channel_available()?;
        if state().on() != on {
            return Ok(false);
        }
        // The marker records ownership of the off state. Brightness stays on the device.
        if !on && !ours {
            std::fs::create_dir_all(config).map_err(|e| e.to_string())?;
            crate::layout_files::atomic_write(&path, "off")?;
        }
        let target = DisplayPower {
            enabled: on,
            ..current
        };
        dev.set_display_power(target)?;
        log_change(target);
    }
    if on {
        std::fs::remove_file(path)
            .map_err(|e| format!("Remove display power recovery state: {e}"))?;
    }
    Ok(true)
}

fn log_change(target: DisplayPower) {
    let state = if target.enabled { "on" } else { "off" };
    crate::log_usb(
        "INFO",
        &format!(
            "Monitor power sync: TRYX display {state} (brightness {})",
            target.brightness
        ),
    );
}

#[repr(C)]
#[derive(PartialEq, Eq)]
struct Guid(u32, u16, u16, [u8; 8]);

const SESSION_DISPLAY_STATUS: Guid = Guid(
    0x2b84c20e,
    0xad23,
    0x4ddf,
    [0x93, 0xdb, 0x05, 0xff, 0xbd, 0x7e, 0xfc, 0xa5],
);

#[repr(C)]
struct PowerSetting {
    guid: Guid,
    data_length: u32,
    data: u32,
}

#[repr(C)]
struct SubscribeParameters {
    callback: unsafe extern "system" fn(*mut c_void, u32, *mut c_void) -> u32,
    context: *mut c_void,
}

#[link(name = "powrprof")]
extern "system" {
    fn PowerRegisterSuspendResumeNotification(
        flags: u32,
        recipient: *const SubscribeParameters,
        handle: *mut *mut c_void,
    ) -> u32;
    fn PowerSettingRegisterNotification(
        guid: *const Guid,
        flags: u32,
        recipient: *const SubscribeParameters,
        handle: *mut *mut c_void,
    ) -> u32;
}

unsafe extern "system" fn on_display_event(
    _context: *mut c_void,
    _kind: u32,
    setting: *mut c_void,
) -> u32 {
    if setting.is_null() {
        return 0;
    }
    let setting = setting.cast::<PowerSetting>();
    if unsafe { (*setting).guid != SESSION_DISPLAY_STATUS || (*setting).data_length != 4 } {
        return 0;
    }
    let monitor_on = match unsafe { (*setting).data } {
        0 => false,
        1 | 2 => true,
        _ => return 0,
    };
    let mut state = STATE.lock().unwrap();
    if !state.suspended {
        state.display_reported = true;
    }
    if state.monitor_on != monitor_on {
        state.monitor_on = monitor_on;
        state.transitions = state.transitions.wrapping_add(1);
        CHANGED.notify_all();
    }
    0
}

unsafe extern "system" fn on_sleep_event(
    context: *mut c_void,
    kind: u32,
    _setting: *mut c_void,
) -> u32 {
    const PBT_APMSUSPEND: u32 = 4;
    const PBT_APMRESUMESUSPEND: u32 = 7;
    const PBT_APMRESUMEAUTOMATIC: u32 = 0x12;
    let mut state = STATE.lock().unwrap();
    let old = *state;
    match kind {
        PBT_APMSUSPEND => {
            state.suspended = true;
            state.display_reported = false;
            // If the display subscription failed, there will be no display-on event on wake.
            if !context.is_null() {
                state.monitor_on = false;
            }
        }
        PBT_APMRESUMEAUTOMATIC => {
            state.suspended = false;
            state.resumes = state.resumes.wrapping_add(1);
        }
        PBT_APMRESUMESUSPEND => {
            if state.suspended {
                state.resumes = state.resumes.wrapping_add(1);
            }
            state.suspended = false;
            if !state.display_reported {
                state.monitor_on = true;
            }
        }
        _ => {}
    }
    if *state != old {
        state.transitions = state.transitions.wrapping_add(1);
        CHANGED.notify_all();
    }
    0
}

pub fn watch() {
    const DEVICE_NOTIFY_CALLBACK: u32 = 2;
    let display_params = Box::leak(Box::new(SubscribeParameters {
        callback: on_display_event,
        context: std::ptr::null_mut(),
    }));
    let mut handle = std::ptr::null_mut();
    let display = unsafe {
        PowerSettingRegisterNotification(
            &SESSION_DISPLAY_STATUS,
            DEVICE_NOTIFY_CALLBACK,
            display_params,
            &mut handle,
        )
    };
    let sleep_params = Box::leak(Box::new(SubscribeParameters {
        callback: on_sleep_event,
        context: if display == 0 {
            std::ptr::dangling_mut::<u8>().cast()
        } else {
            std::ptr::null_mut()
        },
    }));
    let resume = unsafe {
        PowerRegisterSuspendResumeNotification(DEVICE_NOTIFY_CALLBACK, sleep_params, &mut handle)
    };
    if display != 0 || resume != 0 {
        crate::log_usb(
            "WARN",
            &format!("Windows power notification registration: display={display}, sleep={resume}"),
        );
    } else {
        crate::log_usb(
            "INFO",
            "Windows monitor and sleep power notifications registered.",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_resume_waits_for_monitor_on_and_dim_keeps_it_on() {
        let sleep = |kind| unsafe {
            on_sleep_event(
                std::ptr::dangling_mut::<u8>().cast(),
                kind,
                std::ptr::null_mut(),
            )
        };
        let display = |data| {
            let mut setting = PowerSetting {
                guid: SESSION_DISPLAY_STATUS,
                data_length: 4,
                data,
            };
            unsafe {
                on_display_event(
                    std::ptr::null_mut(),
                    0,
                    (&mut setting as *mut PowerSetting).cast(),
                );
            }
        };
        display(2);
        assert!(state().on());
        display(0);
        assert!(!state().on());
        display(1);
        sleep(4);
        assert!(!state().on());
        sleep(0x12);
        assert!(!state().on());
        let resumed = state().resumes;
        sleep(7);
        assert!(state().on());
        assert_eq!(state().resumes, resumed);
        sleep(4);
        sleep(7);
        assert!(state().on());
        assert_eq!(state().resumes, resumed + 1);
        sleep(4);
        sleep(0x12);
        display(0);
        sleep(7);
        assert!(!state().on());
        display(1);
    }
}
