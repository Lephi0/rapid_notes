use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub use egui_glass_ui::{GlassTintPreset, DEFAULT_EDITOR_FONT};

const CONFIG_FILE_NAME: &str = "rapid_notes_config.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ScrollModifier {
    #[default]
    Alt,
    Ctrl,
    Shift,
    CtrlAlt,
}

impl ScrollModifier {
    pub fn label(self) -> &'static str {
        match self {
            ScrollModifier::Alt => "Alt",
            ScrollModifier::Ctrl => "Ctrl",
            ScrollModifier::Shift => "Shift",
            ScrollModifier::CtrlAlt => "Ctrl+Alt",
        }
    }

    pub fn matches(self, m: egui::Modifiers) -> bool {
        let ctrl = m.ctrl || m.command;
        match self {
            ScrollModifier::Alt => m.alt && !ctrl && !m.shift,
            ScrollModifier::Ctrl => ctrl && !m.alt && !m.shift,
            ScrollModifier::Shift => m.shift && !ctrl && !m.alt,
            ScrollModifier::CtrlAlt => ctrl && m.alt && !m.shift,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeybindSlot {
    ToggleGridView,
    PrevNote,
    NextNote,
    NewNote,
    DeleteNote,
    OpenSettings,
    HideWindow,
    ToggleAlwaysOnTop,
    TogglePreview,
    CopyNote,
    ToggleChecklist,
    SearchNotes,
    FormatBold,
    FormatItalic,
    FormatCode,
}

impl KeybindSlot {
    pub const ALL_IN_APP: [Self; 15] = [
        Self::ToggleGridView,
        Self::PrevNote,
        Self::NextNote,
        Self::NewNote,
        Self::DeleteNote,
        Self::OpenSettings,
        Self::HideWindow,
        Self::ToggleAlwaysOnTop,
        Self::TogglePreview,
        Self::CopyNote,
        Self::ToggleChecklist,
        Self::SearchNotes,
        Self::FormatBold,
        Self::FormatItalic,
        Self::FormatCode,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ToggleGridView => "At-a-Glance Grid",
            Self::PrevNote => "Previous Note",
            Self::NextNote => "Next Note",
            Self::NewNote => "New Note",
            Self::DeleteNote => "Delete Current Note",
            Self::OpenSettings => "Open Settings",
            Self::HideWindow => "Hide to Tray / Exit Grid",
            Self::ToggleAlwaysOnTop => "Toggle Always on Top",
            Self::TogglePreview => "Toggle Markdown Preview",
            Self::CopyNote => "Copy Note to Clipboard",
            Self::ToggleChecklist => "Toggle Checklist Item",
            Self::SearchNotes => "Search Notes",
            Self::FormatBold => "Format Bold",
            Self::FormatItalic => "Format Italic",
            Self::FormatCode => "Format Inline Code",
        }
    }

    pub fn default_shortcut(self) -> &'static str {
        match self {
            Self::ToggleGridView => "Alt+A",
            Self::PrevNote => "Alt+Left",
            Self::NextNote => "Alt+Right",
            Self::NewNote => "Ctrl+N",
            Self::DeleteNote => "Ctrl+Shift+D",
            Self::OpenSettings => "Ctrl+Comma",
            Self::HideWindow => "Escape",
            Self::ToggleAlwaysOnTop => "Alt+Shift+T",
            Self::TogglePreview => "Ctrl+Shift+P",
            Self::CopyNote => "Ctrl+Shift+C",
            Self::ToggleChecklist => "Ctrl+L",
            Self::SearchNotes => "Ctrl+F",
            Self::FormatBold => "Ctrl+B",
            Self::FormatItalic => "Ctrl+I",
            Self::FormatCode => "Ctrl+E",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Keybinds {
    pub global_toggle_app: String,
    pub toggle_grid_view: String,
    pub scroll_switch_modifier: ScrollModifier,
    pub prev_note: String,
    pub next_note: String,
    pub new_note: String,
    pub delete_note: String,
    pub open_settings: String,
    pub hide_window: String,
    pub toggle_always_on_top: String,
    pub toggle_preview: String,
    pub copy_note: String,
    pub toggle_checklist: String,
    pub search_notes: String,
    pub format_bold: String,
    pub format_italic: String,
    pub format_code: String,
}

impl Default for Keybinds {
    fn default() -> Self {
        Self {
            global_toggle_app: "Win+Shift+A".to_string(),
            toggle_grid_view: KeybindSlot::ToggleGridView.default_shortcut().to_string(),
            scroll_switch_modifier: ScrollModifier::Alt,
            prev_note: KeybindSlot::PrevNote.default_shortcut().to_string(),
            next_note: KeybindSlot::NextNote.default_shortcut().to_string(),
            new_note: KeybindSlot::NewNote.default_shortcut().to_string(),
            delete_note: KeybindSlot::DeleteNote.default_shortcut().to_string(),
            open_settings: KeybindSlot::OpenSettings.default_shortcut().to_string(),
            hide_window: KeybindSlot::HideWindow.default_shortcut().to_string(),
            toggle_always_on_top: KeybindSlot::ToggleAlwaysOnTop.default_shortcut().to_string(),
            toggle_preview: KeybindSlot::TogglePreview.default_shortcut().to_string(),
            copy_note: KeybindSlot::CopyNote.default_shortcut().to_string(),
            toggle_checklist: KeybindSlot::ToggleChecklist.default_shortcut().to_string(),
            search_notes: KeybindSlot::SearchNotes.default_shortcut().to_string(),
            format_bold: KeybindSlot::FormatBold.default_shortcut().to_string(),
            format_italic: KeybindSlot::FormatItalic.default_shortcut().to_string(),
            format_code: KeybindSlot::FormatCode.default_shortcut().to_string(),
        }
    }
}

impl Keybinds {
    pub fn get(&self, slot: KeybindSlot) -> &str {
        match slot {
            KeybindSlot::ToggleGridView => &self.toggle_grid_view,
            KeybindSlot::PrevNote => &self.prev_note,
            KeybindSlot::NextNote => &self.next_note,
            KeybindSlot::NewNote => &self.new_note,
            KeybindSlot::DeleteNote => &self.delete_note,
            KeybindSlot::OpenSettings => &self.open_settings,
            KeybindSlot::HideWindow => &self.hide_window,
            KeybindSlot::ToggleAlwaysOnTop => &self.toggle_always_on_top,
            KeybindSlot::TogglePreview => &self.toggle_preview,
            KeybindSlot::CopyNote => &self.copy_note,
            KeybindSlot::ToggleChecklist => &self.toggle_checklist,
            KeybindSlot::SearchNotes => &self.search_notes,
            KeybindSlot::FormatBold => &self.format_bold,
            KeybindSlot::FormatItalic => &self.format_italic,
            KeybindSlot::FormatCode => &self.format_code,
        }
    }

    pub fn set(&mut self, slot: KeybindSlot, value: String) {
        match slot {
            KeybindSlot::ToggleGridView => self.toggle_grid_view = value,
            KeybindSlot::PrevNote => self.prev_note = value,
            KeybindSlot::NextNote => self.next_note = value,
            KeybindSlot::NewNote => self.new_note = value,
            KeybindSlot::DeleteNote => self.delete_note = value,
            KeybindSlot::OpenSettings => self.open_settings = value,
            KeybindSlot::HideWindow => self.hide_window = value,
            KeybindSlot::ToggleAlwaysOnTop => self.toggle_always_on_top = value,
            KeybindSlot::TogglePreview => self.toggle_preview = value,
            KeybindSlot::CopyNote => self.copy_note = value,
            KeybindSlot::ToggleChecklist => self.toggle_checklist = value,
            KeybindSlot::SearchNotes => self.search_notes = value,
            KeybindSlot::FormatBold => self.format_bold = value,
            KeybindSlot::FormatItalic => self.format_italic = value,
            KeybindSlot::FormatCode => self.format_code = value,
        }
    }

    pub fn reset_all(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OpenMonitor {
    #[default]
    MainMonitor,
    MouseMonitor,
}

impl OpenMonitor {
    pub fn label(self) -> &'static str {
        match self {
            OpenMonitor::MainMonitor => "Main Monitor",
            OpenMonitor::MouseMonitor => "Where Mouse Is",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub window_pos: Option<[f32; 2]>,
    pub window_size: [f32; 2],
    pub settings_window_pos: Option<[f32; 2]>,
    pub open_monitor: OpenMonitor,
    pub always_on_top: bool,
    pub start_hidden: bool,
    pub hide_on_focus_loss: bool,
    pub glass_tint_preset: GlassTintPreset,
    pub glass_tint_opacity: f32,
    pub editor_font: String,
    pub editor_font_size: f32,
    pub markdown_highlighting: bool,
    pub inline_math: bool,
    pub always_show_chrome: bool,
    pub auto_delete_empty: bool,
    pub notes_dir: Option<String>,
    pub note_order: Vec<String>,
    pub active_note_file: Option<String>,
    pub keybinds: Keybinds,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            window_pos: None,
            window_size: [580.0, 440.0],
            settings_window_pos: None,
            open_monitor: OpenMonitor::MainMonitor,
            always_on_top: true,
            start_hidden: false,
            hide_on_focus_loss: true,
            glass_tint_preset: GlassTintPreset::System,
            glass_tint_opacity: 0.56,
            editor_font: DEFAULT_EDITOR_FONT.to_string(),
            editor_font_size: 15.0,
            markdown_highlighting: true,
            inline_math: true,
            always_show_chrome: false,
            auto_delete_empty: false,
            notes_dir: None,
            note_order: Vec::new(),
            active_note_file: None,
            keybinds: Keybinds::default(),
        }
    }
}

impl AppConfig {
    pub fn exe_dir() -> PathBuf {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(dir) = exe_path.parent() {
                return dir.to_path_buf();
            }
        }
        PathBuf::from(".")
    }

    pub fn config_path() -> PathBuf {
        Self::exe_dir().join(CONFIG_FILE_NAME)
    }

    pub fn default_notes_dir() -> PathBuf {
        Self::exe_dir().join("notes")
    }

    pub fn effective_notes_dir(&self) -> PathBuf {
        if let Some(ref custom) = self.notes_dir {
            let trimmed = custom.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed);
            }
        }
        Self::default_notes_dir()
    }

    pub fn clamp_values(&mut self) {
        self.window_size[0] = self.window_size[0].clamp(340.0, 1400.0);
        self.window_size[1] = self.window_size[1].clamp(260.0, 1000.0);
        self.glass_tint_opacity = self.glass_tint_opacity.clamp(0.05, 0.95);
        self.editor_font_size = self.editor_font_size.clamp(10.0, 32.0);
        if self.editor_font.trim().is_empty() {
            self.editor_font = DEFAULT_EDITOR_FONT.to_string();
        }
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(contents) = fs::read_to_string(&path) {
            if let Ok(mut cfg) = serde_json::from_str::<AppConfig>(&contents) {
                cfg.clamp_values();
                return cfg;
            }
        }
        let default_cfg = Self::default();
        let _ = default_cfg.save();
        default_cfg
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults_and_roundtrip() {
        let mut cfg = AppConfig::default();
        assert_eq!(cfg.editor_font, "Courier New");
        assert_eq!(cfg.keybinds.global_toggle_app, "Win+Shift+A");
        assert_eq!(cfg.keybinds.toggle_grid_view, "Alt+A");
        assert_eq!(cfg.keybinds.scroll_switch_modifier, ScrollModifier::Alt);
        assert!(cfg.hide_on_focus_loss);
        assert_eq!(cfg.open_monitor, OpenMonitor::MainMonitor);

        cfg.editor_font = "Consolas".to_string();
        cfg.editor_font_size = 18.0;
        cfg.open_monitor = OpenMonitor::MouseMonitor;
        cfg.keybinds.toggle_grid_view = "Ctrl+G".to_string();
        let json = serde_json::to_string(&cfg).expect("serialize config");
        let parsed: AppConfig = serde_json::from_str(&json).expect("deserialize config");
        assert_eq!(parsed.editor_font, "Consolas");
        assert_eq!(parsed.editor_font_size, 18.0);
        assert_eq!(parsed.open_monitor, OpenMonitor::MouseMonitor);
        assert_eq!(parsed.keybinds.toggle_grid_view, "Ctrl+G");

        // Legacy partial JSON deserializes cleanly with defaults
        let partial: AppConfig =
            serde_json::from_str(r#"{"editor_font_size": 99.0}"#).expect("partial json");
        let mut clamped = partial;
        clamped.clamp_values();
        assert_eq!(clamped.editor_font_size, 32.0);
        assert_eq!(clamped.keybinds.global_toggle_app, "Win+Shift+A");
    }
}
