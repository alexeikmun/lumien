# Evidence Index

| ID | Title | Confidence | Target / Path | Primary Method | Summary File |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **EV-001** | USB Topology & Descriptors | Confirmed (Hardware) | Physical Device (Port 13) | Win32 Hub IOCTL | [confirmed-findings.md](confirmed-findings.md#ev-001-hardware-identifiers-and-usb-topology) |
| **EV-002** | Windows Driver & Device Interface | Confirmed (Hardware) | `USB\VID_391A&PID_1021` | Windows PnP / Registry | [confirmed-findings.md](confirmed-findings.md#ev-002-windows-driver--device-interface-binding) |
| **EV-003** | Application Process Stack | Confirmed (Shipped) | `KANALI.exe`, `Screen.exe`, `UDB.exe` | REA Static & ASAR Analysis | [confirmed-findings.md](confirmed-findings.md#ev-003-application-stack--process-architecture) |
| **EV-004** | IPC & Protocol Architecture | Confirmed (Runtime) | `\\.\pipe\TryxBridge` | Named Pipe & Protobuf strings | [confirmed-findings.md](confirmed-findings.md#ev-004-ipc-and-protocol-encapsulation) |
| **EV-005** | Display Specs & Resolution | Confirmed (Shipped) | `panorama2240` Profile | `app.asar` Config | [confirmed-findings.md](confirmed-findings.md#ev-005-display-resolution-and-media-specifications) |
| **HYP-001** | Rockchip SoC & Partition Layout | Strong Hypothesis | Firmware scripts & Protobuf | Batch & String Analysis | [hypotheses.md](hypotheses.md#hyp-001-device-onboard-operating-system--architecture) |
| **HYP-002** | USB Binary Framing | Strong Hypothesis | `UDB.exe` | Runtime log analysis | [hypotheses.md](hypotheses.md#hyp-002-framing-and-header-wrapper-over-usb) |
| **HYP-003** | Local Storage vs Streaming | Strong Hypothesis | Telemetry logs | Runtime size analysis | [hypotheses.md](hypotheses.md#hyp-003-media-storage-vs-live-streaming) |
