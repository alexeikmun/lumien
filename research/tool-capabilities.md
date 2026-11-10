# Reverse Engineering Tool Capabilities Matrix

## 1. REA Framework Assessment

- **Version**: REA 6.8.0 (CLI package 6.8.0, SDK 2.3.1)
- **Host Platform**: `win32 x64`
- **MCP Server Status**: Active, responsive via `call_mcp_tool` on `rea`

### 1.1 REA Provider & Capability Matrix on Windows 11

| Provider / Feature | Status | Limitations / Diagnostics on this Host |
| :--- | :--- | :--- |
| **`rea-dotnet-static`** | **Available** | Decodes PE/CLI metadata, assemblies, types, methods, fields, signatures, and CIL anchors. Tested on `Screen.exe` (`ev_0c341de71073c198ec511ebd90b1725cd108aac440a71d9dbf7bd8d240870464`). |
| **`rea-artifact-graph`** | **Available** | Safe artifact extraction and file graph inspection. |
| **`inspect_pe_imports`** | **Available** | Full PE import table and delay-load descriptor analysis. Tested on `UDB.exe` (`ev_407883a451d6c8e3`). |
| **`inspect_pe_load_config`** | **Available** | PE Security Directory, Load Config, and Control Flow Guard analysis. |
| **`inspect_pe_resources`** | **Available** | PE manifest, version, and icon resource decoding. |
| **`ghidra`** | **Unavailable** | `GHIDRA_INSTALL_DIR` is not set. Headless Ghidra decompilation unavailable until configured. |
| **`hopper`** | **Unavailable** | Host unsupported (`win32` platform; Hopper is macOS-only). |
| **`ida`** | **Unavailable** | `REA_IDA_MCP_CONFIG` not configured. |
| **`kuna`** | **Unavailable** | `REA_KUNA_COMMAND` not configured. |
| **`frida`** | **Configurable** | Frida 17.17.0 is installed in Python environment (`C:\Users\Alexis\AppData\Local\Programs\Python\Python312`). Can be attached if dynamic hooking is needed. |

---

## 2. Complementary Tool Mapping

Because native disassembly/decompilation backends in REA on this host are unconfigured, specific complementary tools and techniques are used to ensure 100% reproducible analysis without speculative leaps:

| Task | Primary REA Tool | Complementary Tool | Justification |
| :--- | :--- | :--- | :--- |
| **.NET Binary Analysis** (`Screen.exe`) | `inspect_managed_artifact`, `inspect_managed_members` | PowerShell reflection / IL inspection | REA provides metadata structure; decompiled C# can be confirmed via IL / metadata tokens. |
| **Native PE Import / Export Analysis** (`UDB.exe`, `MediaX.dll`) | `inspect_pe_imports`, `inspect_native_exports` | Python `pefile` / ctypes inspection | Rapid filtering and cross-correlation with Windows headers. |
| **USB Descriptor & Endpoint Mapping** | `inspect_pcap_capture` (passive) | Win32 `IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX` via `ctypes` | Queries hardware directly through Windows USB Hub driver without installing filter drivers. |
| **Electron / JS Bundle Inspection** (`app.asar`) | `inspect_artifact` | Custom stream unpacker (`read_asar.py`) | Inspects frontend IPC commands, Vue logic, and UI parameters directly from packed ASAR. |
| **Protobuf Message Schema Recovery** | `inspect_analysis_view` | Descriptor parser (`dump_proto.py`) | Recovers exact `.proto` schemas and message fields compiled into `UDB.exe`. |
| **Runtime IPC & Logging Tracing** | `observe_native_calls` | Live Windows NamedPipe queries & log parsing | Passive observation of ongoing commands between KANALI, Screen.exe, and UDB.exe. |
