# Environment Baseline & Device Inventory

## 1. Host System

| Component | Specification |
| :--- | :--- |
| **OS** | Windows 11 Pro 64-bit (Build 26100.9818) |
| **CPU** | AMD Ryzen 7 9800X3D 8-Core Processor |
| **GPU** | NVIDIA GeForce RTX 5080 (16 GB GDDR7) |
| **RAM** | 32 GB DDR5 (4800 MHz) |
| **Workspace** | `C:\Users\Alexis\code\lumien` |

---

## 2. Connected Device Hardware Inventory

- **Product Name**: TRYX Panorama SE ARGB 360 (AIO Cooler LCD Display)
- **Status**: Connected and functioning normally

### 2.1 Identifiers & PnP Registration

| Property | Value | Source / Evidence |
| :--- | :--- | :--- |
| **Vendor ID (VID)** | `0x391A` | PnP & USB Device Descriptor |
| **Product ID (PID)** | `0x1021` | PnP & USB Device Descriptor |
| **Device Revision (bcdDevice)** | `0x0310` (v3.10) | USB Device Descriptor |
| **USB Specification (bcdUSB)** | `0x0201` (USB 2.1) | USB Device Descriptor |
| **Device Instance ID** | `USB\VID_391A&PID_1021\7DA3448149D8E9DA` | Windows PnP Manager |
| **Device Serial Number** | `7DA3448149D8E9DA` | USB String Descriptor 3 |
| **Manufacturer String** | `RK` (Rockchip) | USB String Descriptor 1 |
| **Product String** | `PASE` (Panorama SE) | USB String Descriptor 2 |
| **Bus Reported Device Desc** | `PASE` | `DEVPKEY_Device_BusReportedDeviceDesc` |
| **Speed** | HighSpeed (480 Mbps) | `IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX` |
| **Port / Hub Location** | Port 13 on Hub `USB\ROOT_HUB30\7&3898D510&0&0` | Windows PnP & USB Hub Driver |
| **Assigned USB Device Address** | `0x0009` (9) | USB Hub Query |
| **Power Consumption** | 500 mA (`bMaxPower` = 250) | USB Configuration Descriptor |

### 2.2 USB Driver & Interfaces

| Property | Value | Evidence / Notes |
| :--- | :--- | :--- |
| **Windows Driver Service** | `usbprint` | In-box Windows USB Printing Support driver (`usbprint.inf:USBPRINT_Inst.NT`) |
| **Driver Version** | `10.0.26100.9818` | Microsoft in-box driver |
| **Device Class GUID** | `{36FC9E60-C465-11CF-8056-444553540000}` | `GUID_DEVCLASS_USB` |
| **Device Interface GUID** | `{F2F40381-F46D-4E51-BCE7-62DE6CF2D098}` | Active device interface under `HKLM\SYSTEM\CurrentControlSet\Control\DeviceClasses` |
| **Symbolic Link / Device Path** | `\\?\USB#VID_391A&PID_1021#7da3448149d8e9da#{f2f40381-f46d-4e51-bce7-62de6cf2d098}` | Direct path for `CreateFileW` Win32 API access |

### 2.3 USB Descriptor Topology

- **Device Class**: `0x00` (composite / defined at interface level)
- **SubClass / Protocol**: `0x00` / `0x00`
- **MaxPacketSize0**: 64 bytes
- **Configurations**: 1 (`bConfigurationValue` = 1)
- **Interface 0 (Alt 0)**:
  - **Interface Class**: `0x07` (USB Printer Class)
  - **Interface SubClass**: `0x01` (Printers)
  - **Interface Protocol**: `0x02` (Bi-directional interface)
  - **Endpoints**: 2
    - **Endpoint `0x81`**: IN, Bulk transfer, `wMaxPacketSize` = 512 bytes, Interval = 0
    - **Endpoint `0x01`**: OUT, Bulk transfer, `wMaxPacketSize` = 512 bytes, Interval = 0

> [!IMPORTANT]
> The display device uses the standard USB Printer Class (Class 07h, Subclass 01h, Protocol 02h) rather than WinUSB or HID. This allows KANALI and Lumien to communicate directly with the device using standard Windows `CreateFileW`, `ReadFile`, and `WriteFile` against the active device interface without installing any third-party or filter drivers (such as Zadig/WinUSB).

---

## 3. Current Device Firmware & Application State

Recovered from live store configuration (`%APPDATA%\kanali\store.json`) and active session logs:

| Item | Value | Evidence Source |
| :--- | :--- | :--- |
| **Firmware Version** | `V1.0.8` | `store.json:deviceFirewareVersion` |
| **Hardware Version** | `V1.1` | `store.json:deviceHardwareVersion` |
| **Reported Device SN** | `BYZLTRYX049869` / `BYZLTEST005` | `udb_2026-11-10.log` & `store.json` |
| **Display Rotation** | 270° | `store.json:waterBlockScreenRotate` |
| **Display Brightness** | 50 (stored), 48 (live config) | `store.json:panoramaSESettingConfig` |
| **Current Display Mode** | `presetMode` | `store.json:panoramaSESettingConfig.mode` |
| **Current Active Media** | `download_52.mp4.h264_2240x1080` | `store.json:panoramaSESettingConfig.presetMedia` |
| **Resolution Indicated** | 2240 × 1080 | Preset media asset filename tag |

---

## 4. Running KANALI Processes & Services

- **Windows Services**: No KANALI-specific Windows services registered.
- **Active Processes Observed**:
  1. `KANALI.exe` (PIDs: 1284, 19072, 19140, 19280, 19424) — Electron application (Main, Renderer, GPU process, utility processes).
  2. `Screen.exe` (PID: 19856) — .NET Framework 4.7.2 helper process (`C:\Program Files\KANALI\resources\Connect\Screen.exe`).
  3. `UDB.exe` (PID: 19788) — Native C++ USB daemon (`C:\Program Files\KANALI\resources\Connect\tm\UDB.exe`).

### 4.1 Active IPC Endpoints

- Named Pipe `\\.\pipe\TryxBridge` (Owned by `UDB.exe`, used for JSON/Protobuf command dispatch).
- Named Pipe `\\.\pipe\TryxScreenPipe` (Used by `Screen.exe`).
- Shared Memory `Local\TryxMediaPush_<...>` (Used for video frame push).
