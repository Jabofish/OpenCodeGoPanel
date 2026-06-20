# OpenCode Usage

A compact Windows desktop panel for monitoring OpenCode Go usage.

## Features

- Shows rolling, weekly, and monthly quota usage.
- Tracks model usage and token totals.
- Displays monthly spend against a configurable budget.
- Supports multiple workspaces, tray hiding, always-on-top mode, and a global hotkey.
- Uses the OpenCode web login flow and stores cookies locally.
- **New:** Mini badge mode with double-click expansion for compact desktop presence.
- Includes local data backup, export, storage status, and health diagnostics.

## Development

```bash
npm run test:rust
```

## Release 0.1.4

- **Badge bubble notifications:** Collapsed badge now shows a WeChat-style chat bubble popping out to the right when toasts are triggered, replacing invisible inline toasts and intrusive OS notifications.
- **Startup notification suppression:** Threshold, budget, and cost-spike notifications are suppressed on the first refresh after startup — no more annoying popups every time the app launches.
- **Window boundary detection:** Badge expansion and bubble display now detect screen edges and reposition automatically to stay within the monitor bounds.
- **Theme switching fix:** Fixed missing `setTheme` action that caused dark/light/system theme switching to silently fail with a TypeError.
- **Badge position stability:** Fixed position shift and semi-transparent background artifacts in bubble mode by avoiding `setResizable` and ensuring both `html` and `body` fill the transparent viewport.
- **Toast cleanup on collapse:** Residual toasts are instantly removed when the badge collapses, preventing lingering animation artifacts.
- **CI cache:** Added `shared-key: "stable"` to `swatinem/rust-cache` for incremental builds across version bumps.

## Release 0.1.3

- **Update dialog now shows immediately** in expanded badge mode instead of being silently deferred.
- **Badge no longer auto-collapses** while the update dialog, download progress, or install prompt is visible.
- **Fixed download 404:** Removed space from `productName` to align the NSIS installer filename with the URL in `latest.json`.

## Release 0.1.2

- **Update check timeout:** Update checks now time out after 15 seconds instead of hanging indefinitely on slow networks, with a clear error message.
- **Loading indicator:** A "Checking for updates…" toast appears immediately when an update check starts and is dismissed automatically when the result arrives.

## Release 0.1.1

- **Auto-update:** Built-in update checker powered by `tauri-plugin-updater` — checks for new versions on startup and on demand, downloads with progress tracking, and installs with a single click. Ed25519-signed releases hosted on GitHub Releases.
- **Badge-mode update handling:** Update dialogs and error toasts are deferred when the mini badge is active or collapsed, preventing clipped/invisible popups. Pending update info is shown when the badge expands to full panel.
- **Usage reports:** New `report_generator.rs` produces Markdown reports covering quota usage, cost breakdown by model, and trend analysis for configurable periods. Reports are saved to the local data directory.
- **Settings additions:** Auto-update toggle and "Check for updates now" button in the Updates settings group.
- **Permission audit test:** `all_commands_have_capability_permissions` cross-references `build.rs` command registrations with capability JSON permissions, catching missing `allow-*` entries at compile time.
- **Release pipeline:** GitHub Actions workflow now signs artifacts with Ed25519 keys, generates `latest.json`, and uploads `.exe`, `.sig`, and `latest.json` to GitHub Releases automatically.
- **Stability:** Scheduler exponential backoff for consecutive refresh failures, improved notification cooldown logic, and various clippy fixes.

## Release 0.1.0

- **Local Data & Health panel:** Settings now shows cache/history/settings/auth/export storage, data folder path, health status, and explicit refresh/check actions.
- **Maintenance module split:** Frontend maintenance state moved to `src/js/maintenance.js`; settings diagnostics rendering moved to `src/js/settings-diagnostics.js`.
- **Backend maintenance service:** Local data status, backup, cleanup, exports folder opening, and health checks moved behind `src-tauri/src/maintenance.rs`, leaving Tauri commands as thin wrappers.
- **Stability pass:** Improved cache/auth/history/scheduler edge handling, hotkey registration consistency, refresh interval normalization, quiet-hours parsing, and local file diagnostics.
- **Test coverage:** Added focused Rust tests for maintenance, cache migration, history calculations, notification windows, settings normalization, and auth edge cases.
- **Models tab:** Preserve search filter input focus during typing for better UX.

## Release 0.0.5

- **Persistent Settings Store:** All user preferences now survive app restarts via Tauri's `plugin-persisted-store` — no more lost configurations.
- **Trends Tab:** New third tab with 7/30/90-day rolling line charts for rolling/weekly/monthly quota percentages, plus period cost summary.
- **Usage Insights Engine:** `insights.js` derives actionable alerts — budget projection pacing, cost spike detection, quota threshold warnings, and 7-day trend surge analysis.
- **Quick Peek Overlay:** Press any hotkey to summon a compact overlay showing rolling percent, month cost, projected cost, and quick actions (refresh, switch tabs, jump workspaces).
- **Interactive Hotkey Recording:** Record custom shortcuts in Settings with real-time key capture (press any combo to set, Esc to cancel).
- **Data Export:** Export usage history, model statistics, and daily costs to CSV from the Settings tab. All exports open in your OS file manager.
- **Configurable Refresh Intervals:** Adjust auto-refresh rates separately for visible (15s-60s) and hidden (5m-30m) windows. Option to disable background refresh entirely.
- **Manual Refresh with Spinner:** New refresh button with loading state and inline feedback — no more guessing when data is stale.
- **Notification Rules:** Fine-grained alert controls — quota warnings, budget projection pacing, refresh failures, configurable quiet hours (22:00-08:00), and cooldown periods.
- **Workspace Profiles:** Rename workspaces with custom aliases, mark favorites, and set per-workspace mini-badge source preferences.
- **Local Data Management:** Backup/restore all local data (settings, history), view storage usage, and selectively clear exports or history.
- **Model Filtering:** Filter the Models tab by name for quick lookup of specific models.
- **Toast Notification System:** Consistent non-intrusive user feedback for actions and errors.
- **Enhanced Empty States:** Better UX and copy when no data is available.

## Release 0.0.4

- **Smart pointer-watch auto-collapse:** The expanded mini badge now tracks the actual cursor position instead of a blind timer — stays open as long as the pointer remains inside the full panel, and collapses ~180 ms after the pointer leaves. This makes editing settings or glancing at details much more reliable.
- **Daily cost chart:** The Usage tab now shows a bar chart of daily spend for the current month with a cumulative line overlay, powered by Chart.js. A total cost header is displayed above the chart.
- **Configurable mini badge source:** A new dropdown in Settings lets you choose which usage period the mini badge displays — Auto (max of all), Rolling, Weekly, or Monthly — instead of always showing the maximum.
- **Cache hit rate in Models tab:** Each model row now includes a cache-hit percentage (e.g. "CACHE 123K · 45.2%") alongside the cache-read token count.
- **Window resize reliability:** Replaced direct JS `setSize`/`setMinSize`/`setMaxSize` calls with proper Tauri `plugin:window` invoke commands and `Logical` size format, fixing window-resize failures on some Windows configurations.
- Expanded Tauri window capabilities (`cursor-position`, `inner-position`, `inner-size`, `set-max-size`, `set-min-size`, `set-resizable`, `set-shadow`, `set-size`) to support the new pointer-watch and resize flows.
- **Settings dropdown widget:** Added a reusable select (dropdown) input for settings, used for the mini badge source picker.
- Guarded the F12 devtools toggle to avoid runtime errors when `toggleDevtools` is not available in the current Tauri build.

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
