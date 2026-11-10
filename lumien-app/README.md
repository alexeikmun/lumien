# Lumien editor

From this directory:

```powershell
pnpm install
pnpm tauri dev
```

Browser preview: `pnpm dev`. If that server is already running, launch desktop with `pnpm tauri dev --config reuse-server.json`.
Checks: `pnpm build`, `pnpm test`.

Demo mode only: no USB writes. Six local widget previews; all metrics simulated. Drag palette widgets or click to insert. Drag selection to move; corner handle resizes. Arrow keys nudge 1 pixel, Shift+arrow 10. Ctrl+D duplicates; Delete removes. Ctrl+Z / Ctrl+Shift+Z undo/redo, Ctrl+S exports. Drag empty workspace or Alt+drag to pan.

Save exports versioned JSON with embedded images; Open validates and restores it. Images: PNG/JPEG/WebP up to 4 MB; layouts up to 15 MB and 100 widgets (editor limits, not hardware limits). Preview fonts and bend guides are not physical calibration.
