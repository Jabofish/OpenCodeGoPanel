# OpenCode Usage

A compact Windows desktop panel for monitoring OpenCode Go usage.

## Features

- Shows rolling, weekly, and monthly quota usage.
- Tracks model usage and token totals.
- Displays monthly spend against a configurable budget.
- Supports multiple workspaces, tray hiding, always-on-top mode, and a global hotkey.
- Uses the OpenCode web login flow and stores cookies locally.
- **New:** Mini badge mode with double-click expansion for compact desktop presence.

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
