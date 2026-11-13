//! Narrow Windows USB transport for TRYX Panorama display controllers.
//!
//! Uses Microsoft in-box usbprint / USB device interface via Win32 asynchronous I/O.

use lumien_protocol::{decode_frame, DeviceResponse, ProtocolError, FRAME_HEADER_LEN};
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};
use std::sync::{Arc, mpsc};

pub type Handle = *mut c_void;
pub const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;

const GENERIC_READ: u32 = 0x80000000;
const GENERIC_WRITE: u32 = 0x40000000;
const FILE_SHARE_READ: u32 = 0x00000001;
const FILE_SHARE_WRITE: u32 = 0x00000002;
const OPEN_EXISTING: u32 = 3;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x00000080;
const FILE_FLAG_OVERLAPPED: u32 = 0x40000000;

const ERROR_IO_PENDING: u32 = 997;
const WAIT_OBJECT_0: u32 = 0;
const WAIT_TIMEOUT: u32 = 258;
const INFINITE: u32 = u32::MAX;

const MAX_PAYLOAD_LIMIT: usize = 65536; // 64 KB safety limit to prevent malformed buffer inflation

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

// GUID_DEVINTERFACE_USB_DEVICE: {A5DCBF10-6530-11D2-901F-00C04FB951ED}
pub const GUID_DEVINTERFACE_USB_DEVICE: GUID = GUID {
    data1: 0xA5DCBF10,
    data2: 0x6530,
    data3: 0x11D2,
    data4: [0x90, 0x1F, 0x00, 0xC0, 0x4F, 0xB9, 0x51, 0xED],
};

#[repr(C)]
struct SP_DEVICE_INTERFACE_DATA {
    cb_size: u32,
    interface_class_guid: GUID,
    flags: u32,
    reserved: usize,
}

#[repr(C)]
#[derive(Default)]
struct OVERLAPPED {
    internal: usize,
    internal_high: usize,
    offset: u32,
    offset_high: u32,
    h_event: Handle,
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(
        file_name: *const u16,
        desired_access: u32,
        share_mode: u32,
        security_attributes: *const c_void,
        creation_disposition: u32,
        flags_and_attributes: u32,
        template_file: Handle,
    ) -> Handle;

    fn CloseHandle(handle: Handle) -> i32;
    fn GetLastError() -> u32;

    fn CreateEventW(
        event_attributes: *const c_void,
        manual_reset: i32,
        initial_state: i32,
        name: *const u16,
    ) -> Handle;

    fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
    fn WaitForMultipleObjects(count: u32, handles: *const Handle, wait_all: i32, milliseconds: u32) -> u32;
    fn SetEvent(handle: Handle) -> i32;

    fn WriteFile(
        file: Handle,
        buffer: *const u8,
        number_of_bytes_to_write: u32,
        number_of_bytes_written: *mut u32,
        overlapped: *mut OVERLAPPED,
    ) -> i32;

    fn ReadFile(
        file: Handle,
        buffer: *mut u8,
        number_of_bytes_to_read: u32,
        number_of_bytes_read: *mut u32,
        overlapped: *mut OVERLAPPED,
    ) -> i32;

    fn GetOverlappedResult(
        file: Handle,
        overlapped: *mut OVERLAPPED,
        number_of_bytes_transferred: *mut u32,
        wait: i32,
    ) -> i32;

    fn CancelIoEx(file: Handle, overlapped: *mut OVERLAPPED) -> i32;
}

#[link(name = "setupapi")]
extern "system" {
    fn SetupDiGetClassDevsW(
        class_guid: *const GUID,
        enumerator: *const u16,
        hwnd_parent: Handle,
        flags: u32,
    ) -> Handle;

    fn SetupDiEnumDeviceInterfaces(
        device_info_set: Handle,
        device_info_data: *const c_void,
        interface_class_guid: *const GUID,
        member_index: u32,
        device_interface_data: *mut SP_DEVICE_INTERFACE_DATA,
    ) -> i32;

    fn SetupDiGetDeviceInterfaceDetailW(
        device_info_set: Handle,
        device_interface_data: *mut SP_DEVICE_INTERFACE_DATA,
        device_interface_detail_data: *mut u8,
        device_interface_detail_data_size: u32,
        required_size: *mut u32,
        device_info_data: *mut c_void,
    ) -> i32;

    fn SetupDiDestroyDeviceInfoList(device_info_set: Handle) -> i32;
}

#[derive(Debug)]
pub enum DeviceError {
    DeviceNotFound,
    OpenFailed(u32),
    WriteFailed(u32),
    ReadFailed(u32),
    Timeout,
    IncompleteWrite { expected: usize, actual: usize },
    BufferOverflow,
    Protocol(ProtocolError),
    /// Device answered with a non-OK status for the named transaction stage.
    Rejected(String),
}

impl From<ProtocolError> for DeviceError {
    fn from(err: ProtocolError) -> Self {
        DeviceError::Protocol(err)
    }
}

/// Enumerate connected TRYX Panorama SE devices (VID 0x391A, PID 0x1021).
pub fn enumerate_devices() -> Vec<String> {
    let mut found = Vec::new();
    let flags = 0x00000002 | 0x00000010; // DIGCF_PRESENT | DIGCF_DEVICEINTERFACE

    let set = unsafe { SetupDiGetClassDevsW(&GUID_DEVINTERFACE_USB_DEVICE, null(), null_mut(), flags) };
    if set == INVALID_HANDLE_VALUE {
        return found;
    }

    let mut if_data = SP_DEVICE_INTERFACE_DATA {
        cb_size: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
        interface_class_guid: GUID_DEVINTERFACE_USB_DEVICE,
        flags: 0,
        reserved: 0,
    };

    let mut idx = 0;
    while unsafe { SetupDiEnumDeviceInterfaces(set, null(), &GUID_DEVINTERFACE_USB_DEVICE, idx, &mut if_data) } != 0 {
        let mut req_size = 0u32;
        unsafe {
            SetupDiGetDeviceInterfaceDetailW(set, &mut if_data, null_mut(), 0, &mut req_size, null_mut());
        }

        if req_size > 4 {
            let mut buf = vec![0u8; req_size as usize];
            // cbSize on x64 is 8 bytes
            let cb_size = (if std::mem::size_of::<usize>() == 8 { 8 } else { 6 }) as u32;
            buf[0..4].copy_from_slice(&cb_size.to_ne_bytes());

            if unsafe {
                SetupDiGetDeviceInterfaceDetailW(
                    set,
                    &mut if_data,
                    buf.as_mut_ptr(),
                    req_size,
                    null_mut(),
                    null_mut(),
                )
            } != 0
            {
                // Path string starts at offset 4
                let u16_slice = unsafe {
                    let ptr = buf.as_ptr().add(4) as *const u16;
                    let mut len = 0;
                    while *ptr.add(len) != 0 {
                        len += 1;
                    }
                    std::slice::from_raw_parts(ptr, len)
                };
                let path = String::from_utf16_lossy(u16_slice);
                let path_lower = path.to_lowercase();
                if path_lower.contains("vid_391a") && path_lower.contains("pid_1021") {
                    found.push(path);
                }
            }
        }
        idx += 1;
    }

    unsafe { SetupDiDestroyDeviceInfoList(set) };
    found
}

/// Managed handle to the opened USB device interface.
pub struct PanoramaDevice {
    handle: Handle,
    read_buffer: Vec<u8>,
}

unsafe impl Send for PanoramaDevice {}
unsafe impl Sync for PanoramaDevice {}

impl PanoramaDevice {
    /// Opens the specified device path using verified non-exclusive shared parameters.
    pub fn open(path: &str) -> Result<Self, DeviceError> {
        let wide_path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();

        let handle = unsafe {
            CreateFileW(
                wide_path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE, // Verified non-exclusive mode
                null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
                null_mut(),
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            let err = unsafe { GetLastError() };
            return Err(DeviceError::OpenFailed(err));
        }

        Ok(Self {
            handle,
            read_buffer: Vec::with_capacity(1024),
        })
    }

    /// Automatically discovers and opens the first available Panorama SE device.
    pub fn open_first() -> Result<Self, DeviceError> {
        let devices = enumerate_devices();
        if let Some(path) = devices.first() {
            Self::open(path)
        } else {
            Err(DeviceError::DeviceNotFound)
        }
    }

    /// Sends a complete wire frame with bounded timeout and verified byte completion.
    pub fn write_frame(&self, frame: &[u8], timeout: Duration) -> Result<usize, DeviceError> {
        let event = unsafe { CreateEventW(null(), 1, 0, null()) };
        if event.is_null() {
            return Err(DeviceError::WriteFailed(unsafe { GetLastError() }));
        }

        let mut overlapped = OVERLAPPED {
            h_event: event,
            ..Default::default()
        };

        let mut bytes_written = 0u32;
        let ok = unsafe {
            WriteFile(
                self.handle,
                frame.as_ptr(),
                frame.len() as u32,
                &mut bytes_written,
                &mut overlapped,
            )
        };

        let res = if ok == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_IO_PENDING {
                let wait = unsafe { WaitForSingleObject(event, timeout.as_millis() as u32) };
                if wait == WAIT_TIMEOUT {
                    // Cancel I/O and await completion cleanup before returning
                    unsafe {
                        CancelIoEx(self.handle, &mut overlapped);
                        let mut transferred = 0u32;
                        GetOverlappedResult(self.handle, &mut overlapped, &mut transferred, 1);
                    }
                    Err(DeviceError::Timeout)
                } else if wait == WAIT_OBJECT_0 {
                    let mut transferred = 0u32;
                    if unsafe { GetOverlappedResult(self.handle, &mut overlapped, &mut transferred, 0) } != 0 {
                        Ok(transferred as usize)
                    } else {
                        Err(DeviceError::WriteFailed(unsafe { GetLastError() }))
                    }
                } else {
                    let err = unsafe { GetLastError() };
                    unsafe {
                        CancelIoEx(self.handle, &mut overlapped);
                        GetOverlappedResult(self.handle, &mut overlapped, &mut bytes_written, 1);
                    }
                    Err(DeviceError::WriteFailed(err))
                }
            } else {
                Err(DeviceError::WriteFailed(err))
            }
        } else {
            Ok(bytes_written as usize)
        };

        unsafe { CloseHandle(event) };

        let actual = res?;
        if actual != frame.len() {
            return Err(DeviceError::IncompleteWrite {
                expected: frame.len(),
                actual,
            });
        }

        Ok(actual)
    }

    /// Reads incoming USB packets until one complete container frame is received.
    pub fn read_frame(&mut self, timeout: Duration) -> Result<Vec<u8>, DeviceError> {
        let deadline = Instant::now() + timeout;

        loop {
            // Check if buffer already contains a full valid frame
            if self.read_buffer.len() >= FRAME_HEADER_LEN {
                match decode_frame(&self.read_buffer) {
                    Ok((payload, _)) => {
                        let payload_vec = payload.to_vec();
                        let total_len = FRAME_HEADER_LEN + payload.len();
                        self.read_buffer.drain(0..total_len);
                        return Ok(payload_vec);
                    }
                    Err(ProtocolError::BufferTooShort { .. }) => {
                        // Need more data
                    }
                    Err(e) => {
                        self.read_buffer.clear();
                        return Err(DeviceError::Protocol(e));
                    }
                }
            }

            let now = Instant::now();
            if now >= deadline {
                return Err(DeviceError::Timeout);
            }
            let remaining_time = deadline - now;

            if self.read_buffer.len() >= MAX_PAYLOAD_LIMIT {
                self.read_buffer.clear();
                return Err(DeviceError::BufferOverflow);
            }

            // Read next chunk (512 bytes = USB Bulk max packet size)
            let mut chunk = [0u8; 512];
            let bytes_read = self.read_chunk(&mut chunk, remaining_time)?;
            if bytes_read == 0 {
                return Err(DeviceError::ReadFailed(0));
            }
            self.read_buffer.extend_from_slice(&chunk[..bytes_read]);
        }
    }

    fn read_chunk(&self, chunk: &mut [u8], timeout: Duration) -> Result<usize, DeviceError> {
        self.read_chunk_until(chunk, timeout, None, None)
    }

    fn read_chunk_until(&self, chunk: &mut [u8], timeout: Duration, stop: Option<&StopEvent>, armed: Option<&mpsc::SyncSender<()>>) -> Result<usize, DeviceError> {
        let event = unsafe { CreateEventW(null(), 1, 0, null()) };
        if event.is_null() {
            return Err(DeviceError::ReadFailed(unsafe { GetLastError() }));
        }

        let mut overlapped = OVERLAPPED {
            h_event: event,
            ..Default::default()
        };

        let mut bytes_read = 0u32;
        let ok = unsafe {
            ReadFile(
                self.handle,
                chunk.as_mut_ptr(),
                chunk.len() as u32,
                &mut bytes_read,
                &mut overlapped,
            )
        };

        let res = if ok == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_IO_PENDING {
                if let Some(ready) = armed { let _ = ready.try_send(()); }
                let wait = match stop {
                    Some(stop) => unsafe {
                        WaitForMultipleObjects(2, [event, stop.0].as_ptr(), 0, INFINITE)
                    },
                    None => unsafe { WaitForSingleObject(event, timeout.as_millis() as u32) },
                };
                if wait == WAIT_TIMEOUT || (stop.is_some() && wait == WAIT_OBJECT_0 + 1) {
                    unsafe {
                        CancelIoEx(self.handle, &mut overlapped);
                        let mut transferred = 0u32;
                        GetOverlappedResult(self.handle, &mut overlapped, &mut transferred, 1);
                    }
                    Err(DeviceError::Timeout)
                } else if wait == WAIT_OBJECT_0 {
                    let mut transferred = 0u32;
                    if unsafe { GetOverlappedResult(self.handle, &mut overlapped, &mut transferred, 0) } != 0 {
                        Ok(transferred as usize)
                    } else {
                        Err(DeviceError::ReadFailed(unsafe { GetLastError() }))
                    }
                } else {
                    let err = unsafe { GetLastError() };
                    unsafe {
                        CancelIoEx(self.handle, &mut overlapped);
                        GetOverlappedResult(self.handle, &mut overlapped, &mut bytes_read, 1);
                    }
                    Err(DeviceError::ReadFailed(err))
                }
            } else {
                Err(DeviceError::ReadFailed(err))
            }
        } else {
            Ok(bytes_read as usize)
        };

        unsafe { CloseHandle(event) };
        res
    }

    /// Transmits a media file to device eMMC via FileTransmitBegin (400) -> FileTransmitData (401) -> FileTransmitEnd (402).
    pub fn transmit_file(&mut self, file_name: &str, data: &[u8], timeout: Duration) -> Result<(), DeviceError> {
        transmit_file_with(file_name, data, |frame| {
            self.write_frame(frame, timeout)?;
            self.read_frame(timeout)
        })
    }
}

impl Drop for PanoramaDevice {
    fn drop(&mut self) {
        if self.handle != INVALID_HANDLE_VALUE && !self.handle.is_null() {
            unsafe { CloseHandle(self.handle) };
            self.handle = INVALID_HANDLE_VALUE;
        }
    }
}

/// Display session with an idle Bulk IN outstanding between command transactions.
/// Uses the existing usbprint.sys handle; no driver or power-policy changes.
pub struct DisplaySession {
    device: Arc<PanoramaDevice>,
    input: mpsc::Receiver<Result<Vec<u8>, DeviceError>>,
    stop: Arc<StopEvent>,
    reader: Option<std::thread::JoinHandle<()>>,
}

struct StopEvent(Handle);
// An unnamed Win32 event can be waited on and signaled from different threads.
unsafe impl Send for StopEvent {}
unsafe impl Sync for StopEvent {}

impl StopEvent {
    fn new() -> Result<Self, DeviceError> {
        let event = unsafe { CreateEventW(null(), 1, 0, null()) };
        if event.is_null() { Err(DeviceError::ReadFailed(unsafe { GetLastError() })) } else { Ok(Self(event)) }
    }

    fn signaled(&self) -> bool {
        unsafe { WaitForSingleObject(self.0, 0) == WAIT_OBJECT_0 }
    }
}

impl Drop for StopEvent {
    fn drop(&mut self) { unsafe { CloseHandle(self.0); } }
}

impl DisplaySession {
    pub fn open_first() -> Result<Self, DeviceError> {
        let device = Arc::new(PanoramaDevice::open_first()?);
        let stop = Arc::new(StopEvent::new()?);
        let (tx, input) = mpsc::sync_channel(16);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let source = Arc::clone(&device);
        let cancelled = Arc::clone(&stop);
        let reader = std::thread::spawn(move || {
            let mut buffered = Vec::new();
            while !cancelled.signaled() {
                let mut chunk = [0u8; 512];
                match source.read_chunk_until(&mut chunk, Duration::ZERO, Some(&cancelled), Some(&ready_tx)) {
                    Ok(0) => { std::thread::sleep(Duration::from_millis(20)); }
                    Ok(n) => {
                        buffered.extend_from_slice(&chunk[..n]);
                        loop {
                            match take_session_frame(&mut buffered) {
                                Ok(Some(payload)) => { if tx.try_send(Ok(payload)).is_err() { return; } }
                                Ok(None) => break,
                                Err(e) => { let _ = tx.try_send(Err(e)); return; }
                            }
                        }
                    }
                    Err(_) if cancelled.signaled() => break,
                    Err(e) => { let _ = tx.try_send(Err(e)); return; }
                }
            }
        });
        let session = Self { device, input, stop, reader: Some(reader) };
        // Do not dispatch OUT before the reader has submitted its first pending IN.
        ready_rx.recv_timeout(Duration::from_secs(2)).map_err(|_| DeviceError::Timeout)?;
        Ok(session)
    }

    pub fn write_frame(&self, frame: &[u8], timeout: Duration) -> Result<usize, DeviceError> {
        self.device.write_frame(frame, timeout)
    }

    pub fn read_frame(&mut self, timeout: Duration) -> Result<Vec<u8>, DeviceError> {
        self.input.recv_timeout(timeout).map_err(|err| match err {
            mpsc::RecvTimeoutError::Timeout => DeviceError::Timeout,
            mpsc::RecvTimeoutError::Disconnected => DeviceError::ReadFailed(0),
        })?
    }

    pub fn display_power(&mut self) -> Result<lumien_protocol::DisplayPower, String> {
        let timeout = Duration::from_millis(500);
        self.write_frame(&lumien_protocol::encode_get_user_config_frame(), timeout)
            .map_err(|e| format!("Read display settings write: {e:?}"))?;
        let payload = self.read_frame(timeout).map_err(|e| format!("Read display settings: {e:?}"))?;
        match lumien_protocol::parse_device_response(&lumien_protocol::encode_frame(&payload)) {
            Ok(DeviceResponse::DisplayPower(power)) => Ok(power),
            Ok(DeviceResponse::Error { code, why }) => Err(format!("Read display settings rejected: {code} {why}").trim_end().into()),
            other => Err(format!("Invalid display settings response: {other:?}")),
        }
    }

    pub fn set_display_power(&mut self, power: lumien_protocol::DisplayPower) -> Result<(), String> {
        let timeout = Duration::from_millis(500);
        self.write_frame(&lumien_protocol::encode_display_power_frame(power.enabled, power.brightness), timeout)
            .map_err(|e| format!("Display power write: {e:?}"))?;
        let payload = self.read_frame(timeout).map_err(|e| format!("Display power response: {e:?}"))?;
        match lumien_protocol::parse_device_response(&lumien_protocol::encode_frame(&payload)) {
            Ok(DeviceResponse::Ack(ack)) if ack.ok => Ok(()),
            Ok(DeviceResponse::Error { code, why }) => Err(format!("Display power change rejected: {code} {why}").trim_end().into()),
            other => Err(format!("Invalid display power response: {other:?}")),
        }
    }

    /// Transmits a media file to device eMMC via FileTransmitBegin (400) -> FileTransmitData (401) -> FileTransmitEnd (402).
    pub fn transmit_file(&mut self, file_name: &str, data: &[u8], timeout: Duration) -> Result<(), DeviceError> {
        transmit_file_with(file_name, data, |frame| {
            self.write_frame(frame, timeout)?;
            self.read_frame(timeout)
        })
    }
}

impl Drop for DisplaySession {
    fn drop(&mut self) {
        unsafe { SetEvent(self.stop.0); }
        if let Some(reader) = self.reader.take() { let _ = reader.join(); }
    }
}

/// Shared FileTransmit transaction; device status codes (e.g. SpaceNotEnough, CRCFail) are kept in the error.
fn transmit_file_with(
    file_name: &str,
    data: &[u8],
    mut transact: impl FnMut(&[u8]) -> Result<Vec<u8>, DeviceError>,
) -> Result<(), DeviceError> {
    let mut exchange = |frame: Vec<u8>, stage: &str| -> Result<(), DeviceError> {
        match lumien_protocol::parse_device_response(&lumien_protocol::encode_frame(&transact(&frame)?))? {
            lumien_protocol::DeviceResponse::Ack(ack) if ack.ok => Ok(()),
            lumien_protocol::DeviceResponse::Error { code, why } => Err(DeviceError::Rejected(format!("{stage}: {code} {why}").trim().to_string())),
            _ => Err(DeviceError::Rejected(format!("{stage}: not acknowledged"))),
        }
    };
    exchange(lumien_protocol::encode_file_transmit_begin_frame(file_name, data.len() as u32), "begin")?;
    const CHUNK_SIZE: usize = 65536;
    for chunk in data.chunks(CHUNK_SIZE) {
        exchange(lumien_protocol::encode_file_transmit_data_frame(chunk), "data")?;
    }
    exchange(lumien_protocol::encode_file_transmit_end_frame("media", lumien_protocol::crc32c(data)), "end")
}

fn take_session_frame(buffered: &mut Vec<u8>) -> Result<Option<Vec<u8>>, DeviceError> {
    match decode_frame(buffered) {
        Ok((payload, remaining)) => {
            let used = buffered.len() - remaining.len();
            let payload = payload.to_vec();
            buffered.drain(..used);
            Ok(Some(payload))
        }
        Err(ProtocolError::BufferTooShort { .. }) if buffered.len() < MAX_PAYLOAD_LIMIT => Ok(None),
        Err(e) => Err(DeviceError::Protocol(e)),
    }
}

#[cfg(test)]
mod session_tests {
    use super::*;
    #[test]
    fn session_preserves_partial_and_coalesced_frames() {
        let first = lumien_protocol::encode_frame(b"one");
        let second = lumien_protocol::encode_frame(b"two");
        let mut buffer = first[..5].to_vec();
        assert!(take_session_frame(&mut buffer).unwrap().is_none());
        buffer.extend_from_slice(&first[5..]);
        buffer.extend_from_slice(&second);
        assert_eq!(take_session_frame(&mut buffer).unwrap(), Some(b"one".to_vec()));
        assert_eq!(take_session_frame(&mut buffer).unwrap(), Some(b"two".to_vec()));
        assert!(buffer.is_empty());
        buffer.extend_from_slice(b"BAD!1234");
        assert!(take_session_frame(&mut buffer).is_err());
    }
}
