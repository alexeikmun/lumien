# Unresolved Questions & Next Validations

## Resolved Questions

### [RESOLVED] UQ-001: Exact Binary USB Framing Structure
- **Resolution**: Both request and response wire packets are framed with an 8-byte container header:
  - Bytes `0x00`..`0x03`: ASCII `"TRYX"` (`0x54, 0x52, 0x59, 0x58`)
  - Bytes `0x04`..`0x07`: `uint32` little-endian Protobuf payload length $L$
  - Bytes `0x08`..`0x08+L-1`: Serialized Protobuf message (`ReqPackagePb` / `RspPackagePb`)
- **Evidence**: `UDB.exe` RVA `0x5aca0` (outbound framing) and RVA `0x668db` (inbound parsing). Documented in [`protocol-specification.md`](../protocol-specification.md).

### [RESOLVED] UQ-004: Direct Lumien Connection & Exclusivity
- **Resolution**: `UDB.exe` opens the device interface `\\?\usb#vid_391a&pid_1021#7da3448149d8e9da#{a5dcbf10-6530-11d2-901f-00c04fb951ed}` with `FILE_SHARE_READ | FILE_SHARE_WRITE` (`0x00000003`). On 2026-11-10, an independent test process concurrently called `CreateFileW` and received handle `0x208` (`err = 0`) while `UDB.exe` was actively communicating with the display.
- **Evidence**: `UDB.exe` RVA `0x56630`, runtime logs, and live hardware test in `check_device_handle_open2.py`.

### [RESOLVED] UQ-002: Minimum Prerequisites for Standalone Brightness Command
- **Resolution**: No preceding handshake or authentication is strictly required by the device firmware to accept a brightness command. A standalone 21-byte `UserConfigPb` carrying only `display_config` is accepted directly and acknowledged with `dummy_msg: "ok"`. Furthermore, disassembly of `UserConfigPb::MergeFrom` (RVA `0x4b350`) and `DisplayConfigPb::MergeFrom` (RVA `0x47f90`) confirms protobuf merge semantics: omitted submessages (`work_config`, `poweron_config`, `standby_config`, `filter_config`, `light_config`) and zeroed rotation fields are skipped and not overwritten.
- **Evidence**: Live hardware tests on 2026-11-10 via `lumien-cli set-brightness 50` and `lumien-cli set-brightness 25`, and disassembly of `UDB.exe` merge routines. Documented in [EV-009](confirmed-findings.md#ev-009-independent-live-hardware-execution--protobuf-merge-semantics).

---

## Active Unresolved Questions

### UQ-003: Video Encoding Parameters and Constraints
- **Question**: What are the exact H.264 profile, level, GOP size, pixel format (YUV420p vs NV12), and bitrate constraints accepted by the onboard hardware decoder for video playback?
- **Status**: Partially recovered (resolution 2240 × 1080, container MP4, codec H.264, fps 60/30).
- **Next Action**: Inspect an actual processed media file from `%APPDATA%\kanali\media` using FFprobe/FFmpeg.

### UQ-005: Sensor Telemetry Text Update Pipeline
- **Question**: How does `CMD_Update_UIItemText` map label IDs (`labelId: 102`, `103`, etc.) to visual canvas locations during runtime telemetry display?
- **Status**: Partially analyzed (`RunConfigPb` establishes `label_groups`, `CMD_Update_UIItemText` updates values dynamically).
- **Next Action**: Decompile `RunConfigPb` structure in `UDB.exe` and observe live telemetry packets during runtime.
