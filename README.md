# Rapid Notes

Fast, lightweight, translucent Windows 11 scratchpad and local Markdown note-taking app built with Rust, `egui`, and [`egui_glass_ui`](https://github.com/Lephi0/egui_glass_ui).

## Features

- **Windows 11 DWM Acrylic Glass UI**:
  - Frameless translucent Acrylic window with `Dark`, `Light`, and `System` (auto-follows Windows theme) presets and configurable tint opacity (`10%`–`95%`)
  - Hover-reveal top and bottom toolbars (or always visible), draggable top header, bottom-right resize grip, and smooth slide transitions when switching notes
  - Tray-only background operation (`with_taskbar(false)`) with optional Always on Top, Start Hidden in System Tray, and Hide When Window Loses Focus
- **Instant Global Summon & Multi-Monitor Support**:
  - System-wide summon/hide hotkey (`Win+Shift+A` by default) with side-aware Left/Right modifier recording (`LWin`, `RWin`, `LCtrl`, `RCtrl`, `LAlt`, `RAlt`, `LShift`, `RShift`)
  - Opens on either the **Main Monitor** (restoring its last position) or **Where Mouse Is** (centered on the cursor's current monitor)
- **Markdown Editing & Interactive Preview**:
  - Live syntax highlighting for headings (`#`–`######`), **bold**, *italic*, ~~strikethrough~~, `inline code`, fenced code blocks, blockquotes, bullet/numbered lists, task checklists (`- [ ]` / `- [x]`), horizontal rules, Markdown links, and bare `http://`/`https://` URLs
  - Smart `Enter` continuation for bullet lists, auto-incrementing numbered lists, and checklists (pressing `Enter` on an empty list item exits the list)
  - Toggleable rendered Markdown preview mode (`Ctrl+Shift+P`) with clickable checkboxes that update the note directly
  - Configurable editor font (defaults to `Courier New`, selectable from installed system monospace and proportional fonts) and text size (`10`–`32 px`)
- **Live Inline Math & Calculations**:
  - Evaluates arithmetic (`+`, `-`, `*`/`×`, `/`/`÷`, `^`, parentheses), variables (`rent = 1,200`), `pi`, percentages (`20% of 150`, `100 + 15%`), currencies (`$`, `€`, `£`), and running aggregates (`sum`, `total`, `avg`, `mean`) while skipping headings, checklists, quotes, dates, and code fences
  - Shows right-aligned result pills in the editor: click a pill to copy its value, or press `Tab` on a line ending with `=` to insert the evaluated result directly
- **Navigation, Search & At-a-Glance Grid**:
  - Flip horizontally between notes with `Alt + Scroll` (modifier configurable: `Alt`, `Ctrl`, `Shift`, `Ctrl+Alt`), `Alt+Left` / `Alt+Right`, or bottom-bar pagination dots
  - Expanded **At-a-Glance Grid** overlay (`Alt+A`) with syntax-highlighted preview cards, live text filtering, arrow-key + `Enter` navigation, hover copy/delete actions, and a `+ New Note` card
  - Quick in-editor search strip (`Ctrl+F`) with jump pills to matching notes
  - One-step Undo Delete for the last deleted note, plus optional auto-deletion of empty notes when switching away
- **System Tray & Glass Settings Window**:
  - Left-click the tray icon to show/hide; right-click for Show/Hide, New Note, At-a-Glance Grid, Always on Top, Open Notes Folder, Settings, and Quit
  - Dedicated frameless glass **Settings** window (`Ctrl+,`) with **Editor**, **Appearance**, **Keybinds**, and **System** tabs

## Default Shortcuts

All global and in-app shortcuts can be recorded or customized in **Settings → Keybinds**.

| Action | Default Shortcut |
| --- | --- |
| Summon / Hide Rapid Notes (Global) | `Win+Shift+A` |
| Switch Notes Horizontally | `Alt + Mouse Wheel` |
| At-a-Glance Grid | `Alt+A` |
| Previous / Next Note | `Alt+Left` / `Alt+Right` |
| New Note | `Ctrl+N` |
| Delete Current Note | `Ctrl+Shift+D` |
| Search Notes | `Ctrl+F` |
| Toggle Markdown Preview | `Ctrl+Shift+P` |
| Copy Note to Clipboard | `Ctrl+Shift+C` |
| Toggle Checklist Item | `Ctrl+L` |
| Format Bold / Italic / Inline Code | `Ctrl+B` / `Ctrl+I` / `Ctrl+E` |
| Toggle Always on Top | `Alt+Shift+T` |
| Open Settings | `Ctrl+,` (`Ctrl+Comma`) |
| Hide to Tray / Close Search / Exit Grid | `Escape` |
| Insert Inline Math Result (after `=`) | `Tab` |

## Inline Math Example

```text
items = 3
$12.50 * items =
20% of 150 =
100 + 15%
sum
```

## Building & Running

Requires Windows 11 (for DWM Acrylic composition) and a Rust toolchain supporting Rust 2024 edition.

`Cargo.toml` depends on [`egui_glass_ui`](https://github.com/Lephi0/egui_glass_ui) as a sibling path dependency (`../egui_glass_ui`), so clone both repositories side by side:

```text
glass_apps/
├── egui_glass_ui/
└── rapid_notes/
```

```powershell
git clone https://github.com/Lephi0/egui_glass_ui.git
git clone https://github.com/Lephi0/rapid_notes.git
cd rapid_notes
cargo run --release
```

To run unit tests:

```powershell
cargo test
```

## Configuration & Storage

- **Notes (`notes/`)**: Stored as plain UTF-8 `.md` files (`note-1.md`, `note-2.md`, ...) in a `notes/` folder next to the executable by default. A custom folder can be configured in **Settings → Editor → Notes Storage** and opened directly in File Explorer.
- **Settings (`rapid_notes_config.json`)**: Automatically created next to the executable, persisting window size and position, glass theme and opacity, editor font settings, monitor/tray preferences, custom notes folder, note order, active note, and keybinds.

## License

Licensed under the [MIT License](LICENSE).
