# AGENTS.md — Developer & AI Agent Guide for Lumien

Lumien is an independent, open-source Windows 11 desktop controller for the **TRYX Panorama SE ARGB 360 LCD display** (Rockchip RK3568, USB `391A:1021`).

---

## 1. Project Principles & Architecture Ladder

Follow lazy senior developer principles (**Ponytail mode**):
1. **YAGNI**: No speculative abstractions, multi-platform stubs, or premature configurations.
2. **Stdlib / Platform Native First**: Use standard library and Windows in-box APIs before adding crates.
   - Transport uses native Win32 `usbprint.sys` via `CreateFileW` / `OVERLAPPED` FFI.
   - Framing and Protobuf codec use zero-dependency hand-written varint parsers.
3. **CLI Before UI**: Every protocol command must be independently exercised and validated in `lumien-cli` before exposing it to Tauri.
4. **Hardware Safety**:
   - Liquid cooler pump and fan curves are strictly **out of scope** to prevent thermal damage.
   - Touch only display backlight (`DisplayConfigPb`), orientation, and media playback.
   - Protobuf merge semantics: only send explicitly targeted submessages (`display_config`). Never overwrite running video or rotation with blank defaults.

---

## 2. Repository Structure

```
lumien/
├── Cargo.toml                       # Cargo workspace definition
├── AGENTS.md                        # Agent guide (this file)
├── README.md                        # Project summary
├── crates/
│   ├── lumien-protocol/             # Symmetrical 8-byte container + Protobuf codec
│   │   ├── Cargo.toml
│   │   └── src/lib.rs               # Pure Rust framing, varint codec, 7/7 unit tests
│   ├── lumien-device/               # Win32 asynchronous USB Bulk transport
│   │   ├── Cargo.toml
│   │   └── src/lib.rs               # SetupAPI enumeration, CreateFileW, ReadFile/WriteFile
│   └── lumien-cli/                  # Diagnostic and hardware verification CLI
│       ├── Cargo.toml
│       └── src/main.rs              # probe, check-prereqs, get-info, auth, set-brightness
└── research/                        # Reverse-engineering documentation
    ├── application-architecture.md  # KANALI v2.6.0 process & IPC decomposition
    ├── environment.md               # Host specs, USB topology, and device interface
    ├── protocol-specification.md    # End-to-end wire framing and hex breakdown
    ├── tool-capabilities.md         # REA and complementary tool analysis
    ├── progress.md                  # Milestone progress tracker
    └── evidence/
        ├── confirmed-findings.md    # Verified hardware evidence (EV-001..EV-009)
        └── unresolved-questions.md  # Resolved and active research questions
```

---

## 3. Hardware & Transport Constraints

- **Device Identifiers**: Vendor ID `0x391A`, Product ID `0x1021`, Revision `0x0310`.
- **Operating System Binding**: Windows in-box `usbprint.sys`. **Do not install WinUSB, libusb, or custom filter drivers.**
- **Device Interface GUID**: `{a5dcbf10-6530-11d2-901f-00c04fb951ed}` (`GUID_DEVINTERFACE_USB_DEVICE`).
  - Do not use `{f2f40381-f46d-4e51-bce7-62de6cf2d098}`; returns `ERROR_FILE_NOT_FOUND` (2).
- **Handle Access Flags**:
  - `dwDesiredAccess`: `GENERIC_READ | GENERIC_WRITE` (`0xC0000000`).
  - `dwShareMode`: `FILE_SHARE_READ | FILE_SHARE_WRITE` (`0x00000003`). Non-exclusive.
  - `dwCreationDisposition`: `OPEN_EXISTING` (`3`).
  - `dwFlagsAndAttributes`: `FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED` (`0x40000080`).
- **Endpoints**:
  - EP `0x01`: Bulk OUT (512-byte max packet size).
  - EP `0x81`: Bulk IN (512-byte max packet size).
- **Display Framebuffer**: 2240 × 1080 AMOLED (L-shaped curved panel).

---

## 4. Wire Framing & Protocol Format

All USB transfers share an 8-byte framing prefix:

```
[0x00..0x03] ASCII "TRYX" (0x54, 0x52, 0x59, 0x58)
[0x04..0x07] uint32 little-endian payload length (L)
[0x08..8+L-1] Serialized Protobuf message (ReqPackagePb / RspPackagePb)
```

### Verified Message Structures

| Command | Wire Size | Request Protobuf | Expected Response |
| :--- | :--- | :--- | :--- |
| **`get_device_info`** | 19 bytes | `HeaderPb(1)` + `DummyMsgPb(100, dummy="NA")` | 112 bytes: `DeviceInfoPb(500)` (OS, firmware, SN, chip ID) |
| **`get_device_auth`** | 17 bytes | `HeaderPb(1)` + `GetDeviceAuthPb(101, key=1)` | 21 bytes: `DeviceAuthPb(501, auth="ok")` |
| **`set_brightness`** | 21 bytes | `HeaderPb(1)` + `UserConfigPb(200)` -> `DisplayConfigPb(5)` -> `enable=1, brightness=N` | 21 bytes: `DummyMsgPb(600, dummy="ok")` |

---

## 5. Standard Developer & Agent Workflows

### Shell & Package Rules
- **Rust Token Killer (RTK)**: Always prefix shell commands with `rtk`:
  - `rtk cargo test`
  - `rtk cargo build`
  - `rtk cargo build --release`
  - `rtk git status`
  - `rtk git diff`
- **Package Manager**: Always use `pnpm` for all frontend JS/TS dependencies. Do not use `npm` or `yarn`.

### Running the Diagnostic CLI

```powershell
# Check running processes and verify host USB channel quiescence
.\target\debug\lumien-cli.exe check-prereqs

# Probe USB interface and verify handle acquisition
.\target\debug\lumien-cli.exe probe

# Read device OS, firmware version, serial number, and chip ID (read-only)
.\target\debug\lumien-cli.exe get-info

# Run authentication handshake query
.\target\debug\lumien-cli.exe auth

# Set display brightness (0..100)
.\target\debug\lumien-cli.exe set-brightness 50
```

### Pre-Flight Safety Checks Before Hardware Writes
1. Run `lumien-cli check-prereqs` to ensure `KANALI.exe`, `Screen.exe`, and `UDB.exe` are stopped to prevent USB packet interleaving.
2. Record current display brightness from `%APPDATA%\kanali\store.json` before modifying settings.
3. Keep brightness values within the verified safe range `0..100`.
4. Always restore original brightness and confirm clean handle closure after active tests.

---

## 6. Implementation Roadmap

- [x] **Phase 0**: Environment & Access Baseline (USB topology, device binding, process hashes).
- [x] **Phase 1**: KANALI Architecture Deconstruction (Named pipes, shared memory, display specs).
- [x] **Phase 2 & 3**: Protocol Extraction & Specification (Wire framing, Protobuf schemas, merge semantics).
- [x] **Phase 4**: Standalone Transport & CLI Verification (Live hardware brightness transaction verified and restored).
- [x] **Phase 5**: Display Rendering, Widget, & Media Pipeline Deconstruction:
  - Disassembled LVGL layout engine & schemas (`RunConfigPb` tag 201, `BatchGroupLabelUpdatePb` tag 300).
  - Proved 3-indicator limit is 100% Vue frontend checkbox restriction.
  - Proved all media transcoded to Annex B Raw H.264 Main@L4.1 (`.h264_2240x1080`); in-box FFmpeg bitstream-compatible.
  - Traced wire file transfer pipeline (`FileTransmitBeginPb`, `FileTransmitDataPb`, `FileTransmitEndPb`).
  - Mapped curved display geometry ($2240 \times 1080$, L-shaped 90° bend zone $X \in [1060, 1180]$) and authored editor architecture.
- [ ] **Phase 6**: Scaffolding Lumien Tauri Application:
  - Tauri v2 + React 19 + TypeScript frontend.
  - Native Rust backend commands without exposing arbitrary USB/shell writes.
