# Lumien

**Lumien** is an independent, open-source Windows 11 desktop application for controlling the **TRYX Panorama SE ARGB 360 LCD display**.

Designed as a modern, lightweight, and clean alternative to the proprietary TRYX KANALI software stack, Lumien will be built with:
- **Core / Protocol / Device Layer**: Rust, communicating natively with the display via Windows in-box USB printing driver (`usbprint.sys`).
- **Desktop Application Shell**: Tauri (latest stable), with WebView2.
- **Frontend**: React + TypeScript.

---

## Current Status: Reverse Engineering & Protocol Recovery

Lumien is currently in the protocol discovery and reverse engineering phase. No speculative code or unstable dependencies are included.

Detailed research documentation:
- [Environment Baseline & Device Inventory](research/environment.md)
- [Tool Capabilities Matrix](research/tool-capabilities.md)
- [Application Architecture](research/application-architecture.md)
- [Progress Log](research/progress.md)
- [Confirmed Findings](research/evidence/confirmed-findings.md)
- [Hypotheses](research/evidence/hypotheses.md)
- [Unresolved Questions](research/evidence/unresolved-questions.md)
