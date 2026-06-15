# OpenCode Usage

A compact Windows desktop panel for monitoring OpenCode Go usage.

## Features

- Shows rolling, weekly, and monthly quota usage.
- Tracks model usage and token totals.
- Displays monthly spend against a configurable budget.
- Supports multiple workspaces, tray hiding, always-on-top mode, and a global hotkey.
- Uses the OpenCode web login flow and stores cookies locally.
- **New:** Mini badge mode with double-click expansion for compact desktop presence.

## Release 0.0.3

- Mini badge now expands only on double-click, keeping single-click drag behavior reliable.
- Expanded mini badge stays open while the pointer remains inside the full panel, so settings can be edited without the window collapsing.
- Removed badge hover scaling, shadows, and cursor changes that clipped or visually dirtied the compact badge.
- Hidden the main window from the Windows taskbar while keeping tray access.
- Persisted usage, model, and cost cache per workspace, with automatic migration from the old single-workspace cache format.
- Workspace switching now shows cached data immediately and refreshes in the background.
- Fixed HOTKEY setting layout so the label and default shortcut are fully visible.

## Release 0.0.2

- Mini badge mode with expandable panel
- App icons for all platforms
- Encrypted cookie storage

## Release 0.0.1

Initial public release with login, workspace switching, usage refresh, model statistics, budget tracking, alerts, tray support, and a custom app icon.
