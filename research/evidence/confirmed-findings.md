# Confirmed Findings

All findings listed below are backed by direct, reproducible hardware and binary evidence.

## EV-001: Hardware Identifiers and USB Topology
- **Evidence Sources**:
  - Live Win32 USB Hub descriptor query (`query_usb_descriptors.py` on root hub port 13).
  - Windows PnP entity properties (`DEVPKEY_Device_HardwareIds`).
- **Confirmed Facts**:
  - **Vendor ID (VID)**: `0x391A`
  - **Product ID (PID)**: `0x1021`
  - **Device Revision**: `0x0310` (v3.10)
  - **USB Speed**: HighSpeed (480 Mbps)
  - **Manufacturer String (Descriptor 1)**: `"RK"` (Rockchip)
  - **Product String (Descriptor 2)**: `"PASE"` (Panorama SE)
  - **Serial Number String (Descriptor 3)**: `"7da3448149d8e9da"`
  - **Device Configuration Value**: `1`
  - **Max Power**: 500 mA
  - **Interface Class**: `0x07` (USB Printer Class)
  - **Interface Subclass**: `0x01` (Printers)
  - **Interface Protocol**: `0x02` (Bi-directional)
  - **Endpoint 0x01**: OUT, Bulk, `wMaxPacketSize` = 512 bytes
  - **Endpoint 0x81**: IN, Bulk, `wMaxPacketSize` = 512 bytes

## EV-002: Windows Driver & Device Interface Binding
- **Evidence Sources**:
  - `Get-PnpDevice` and `HKLM\SYSTEM\CurrentControlSet\Control\DeviceClasses`.
  - `UDB.exe` log line: `[USB] Open ok: path=\\?\usb#vid_391a&pid_1021#7da3448149d8e9da#{a5dcbf10-6530-11d2-901f-00c04fb951ed}`.
  - Live `CreateFileW` hardware test opening handle `0x208` (`err = 0`).
- **Confirmed Facts**:
  - Device binds out-of-the-box to Microsoft's in-box `usbprint.sys` driver (`usbprint.inf`).
  - Active Device Interface GUID used by `UDB.exe`: `{a5dcbf10-6530-11d2-901f-00c04fb951ed}` (`GUID_DEVINTERFACE_USB_DEVICE`).
  - Device path: `\\?\usb#vid_391a&pid_1021#7da3448149d8e9da#{a5dcbf10-6530-11d2-901f-00c04fb951ed}`.
  - No WinUSB, libusb, or custom kernel driver installation is needed. Communication uses standard Win32 `CreateFileW`, `ReadFile`, and `WriteFile`.

## EV-003: Application Stack & Process Architecture
- **Evidence Sources**:
  - REA `inspect_managed_artifact` (`ev_0c341de71073c198ec511ebd90b1725cd108aac440a71d9dbf7bd8d240870464`).
  - REA `inspect_pe_imports` (`ev_407883a451d6c8e3`).
  - `app.asar` extraction and analysis (`package.json`, `out/main/index-1c57b0ec.js`).
- **Confirmed Facts**:
  - KANALI v2.6.0 is an Electron application (Vue 3 / Vite).
  - The UI does not communicate with the USB device directly.
  - Telemetry is handled by `Screen.exe` (.NET 4.7.2) using `LibreHardwareMonitorLib`.
  - Device USB access in KANALI is managed by `UDB.exe` (native 64-bit C++ daemon).

## EV-004: IPC and Protocol Encapsulation
- **Evidence Sources**:
  - Active Named Pipes on host (`\\.\pipe\TryxBridge`, `\\.\pipe\TryxScreenPipe`).
  - `UDB.exe` strings and symbols inspection (`dump_proto.py`).
  - Active runtime logs (`udb_2026-11-10.log`, `screen-2026-11-10.log`).
- **Confirmed Facts**:
  - Frontend communicates with `UDB.exe` via Named Pipe `\\.\pipe\TryxBridge`.
  - Named Pipe protocol is framed JSON (`{ "header": { "cmd": "...", "sn": "..." }, ... }`).
  - Device protocol over USB is Google Protocol Buffers (`Tryx.USBProtocol` / `Tryx.Config`).
  - `UDB.exe` translates high-level JSON commands to binary Protobuf messages, dispatches them over USB Bulk OUT (0x01), and receives Protobuf responses over Bulk IN (0x81).

## EV-005: Display Resolution and Media Specifications
- **Evidence Sources**:
  - `app.asar:out/main/index-1c57b0ec.js` (`screenProfile:"panorama2240"`).
  - `%APPDATA%\kanali\store.json` (`panoramaSESettingConfig`).
  - `MediaPushBeginPb` protobuf definition (`width = 3`, `height = 4`).
- **Confirmed Facts**:
  - Display logical and media canvas resolution: **2240 × 1080** pixels (aspect ratio ~28:9 curved ultrawide).
  - Physical panel: L-shaped curved AMOLED display with a 2240 × 1080 logical framebuffer.
  - Preset and custom media video container/codec: MP4 container with H.264 video stream.
  - Standard asset suffix: `.mp4.h264_2240x1080`.
  - Max framerate: 60 fps (30 fps for 1:1 cropped square display areas).

## EV-006: Device Handle Exclusivity Disproven
- **Evidence Sources**:
  - Disassembly of `CreateFileW` at `UDB.exe` RVA `0x56630` (`lea r8d, [r14 + 3]` -> `dwShareMode = FILE_SHARE_READ | FILE_SHARE_WRITE`).
  - Live hardware test execution (`check_device_handle_open2.py`) returning handle `0x208` (`err = 0`) concurrently while `UDB.exe` was actively running and communicating.
- **Confirmed Facts**:
  - Exclusivity was merely inferred in earlier notes, not required by driver or OS.
  - Multiple processes can concurrently open the device interface with shared read/write flags.
  - Lumien can open the device handle without terminating KANALI; however, Lumien should run standalone during operations to avoid interleaving USB transfers.

## EV-007: Wire Framing Specification
- **Evidence Sources**:
  - Disassembly of `SendReqPackage` at `UDB.exe` RVA `0x5aab0`:
    - `0x5ac9c`: `mov dword ptr [rbp - 8], 0x58595254` (ASCII `"TRYX"`).
    - `0x5acaa`–`0x5acbf`: uint32 little-endian length.
  - Disassembly of response frame extraction at `UDB.exe` RVA `0x668db`: skips 8-byte framing prefix and passes payload to `ParseFromArray`.
- **Confirmed Facts**:
  - All USB frames (request and response) share an 8-byte container:
    - Bytes 0..3: ASCII `"TRYX"` (`0x54, 0x52, 0x59, 0x58`).
    - Bytes 4..7: uint32 LE payload length $L$.
    - Bytes 8..(8+L-1): Serialized Protobuf payload.

## EV-008: End-to-End Brightness Transaction & Protobuf Schema
- **Evidence Sources**:
  - Disassembly of `CMD_Set_UserConfig` handler at `UDB.exe` RVA `0x56e60`.
  - Runtime logs in `udb_2026-11-10.log` and `udb_2026-11-09.log` correlating timestamps and brightness adjustments (10%, 28%, 42%, 50%, 100%).
  - Embedded `.proto` descriptor recovery in `UDB.exe` (`parse_descriptor2.py`).
- **Confirmed Facts**:
  - Brightness is represented in `DisplayConfigPb` field 2 (`backlight_brightness`, uint32 0..100) and field 1 (`backlight_enable`, bool).
  - Wrapped inside `UserConfigPb` field 5 (`display_config`).
  - Wrapped inside `ReqPackagePb` field 200 (`user_config`).
  - Frame length for standard brightness request is 21 bytes (8-byte header + 13-byte Protobuf payload).
  - Acknowledgment response is 21 bytes (`TRYX` + 13-byte `RspPackagePb` with `dummy_msg: "ok"`).

## EV-009: Independent Live Hardware Execution & Protobuf Merge Semantics
- **Evidence Sources**:
  - Live CLI execution with KANALI terminated (`lumien-cli get-info`, `auth`, `set-brightness 50`, `set-brightness 25`).
  - Disassembly of `UserConfigPb::MergeFrom` (`UDB.exe` RVA `0x4b350`) and `DisplayConfigPb::MergeFrom` (`UDB.exe` RVA `0x47f90`).
- **Confirmed Facts**:
  - **Read-Only Query Execution**: Sending 19-byte `get_device_info` frame (`54 52 59 58 0b 00 00 00 0a 02 08 01 a2 06 04 0a 02 4e 41`) successfully completed in 34.85 ms, returning 112-byte response containing `product_name: "PASE"`, `serial_number: "BYZLTRYX049869"`, `chip_id: "7da3448149d8e9da"`, `os_name: "Linux"`, `os_version: "kernel 5.1"`, `firmware_version: "v2.0.6.20260713"`, `app_version: "v2.0.4.20260804"`.
  - **Auth Execution**: Sending 17-byte `get_device_auth` frame completed in 8.06 ms, returning 21-byte response with `device_auth { auth: "ok" }`.
  - **Protobuf Merge Semantics**: Disassembly of `0x4b350` proves that omitted submessages (`poweron_config`, `standby_config`, `work_config`, `filter_config`, `light_config`) are checked with `test` and skipped when null. Disassembly of `0x47f90` proves omitted display fields (`ui_rotation`, `media_rotation`, `mirror`) are checked with `test` and skipped when zero. Therefore, isolated `DisplayConfigPb` commands do not overwrite active video playback or display orientation.
  - **Live Controlled Brightness Change**:
    - Dispatched 21-byte frame for 50% brightness (`54 52 59 58 0d 00 00 00 0a 02 08 01 c2 0c 06 2a 04 08 01 10 32`).
    - Win32 Write completed in 398.40 µs.
    - Win32 Read completed in 31.93 ms, returning 21-byte acknowledgment (`dummy_msg: "ok"`).
    - Visible backlight physically changed to 50% on hardware while video playback continued undisturbed.
  - **Reversible Restoration**:
    - Dispatched 21-byte restoration frame for 25% brightness (`54 52 59 58 0d 00 00 00 0a 02 08 01 c2 0c 06 2a 04 08 01 10 19`).
    - Write completed in 440.60 µs; Read completed in 31.35 ms with `dummy_msg: "ok"`.
    - Backlight physically restored to baseline 25%. Handle closed cleanly.
