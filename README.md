# OpenCode Usage

A compact Windows desktop panel for monitoring OpenCode Go usage.

## Features

- Shows rolling, weekly, and monthly quota usage.
- Tracks model usage and token totals.
- Displays monthly spend against a configurable budget.
- Supports multiple workspaces, tray hiding, always-on-top mode, and a global hotkey.
- Uses the OpenCode web login flow and stores cookies locally.

## Development

```bash
npm install
npm run tauri -- build
```

The Rust backend lives in `src-tauri/`; the frontend is plain HTML, CSS, and JavaScript under `src/`.

## Release 0.01

Initial public release with login, workspace switching, usage refresh, model statistics, budget tracking, alerts, tray support, and a custom app icon.
