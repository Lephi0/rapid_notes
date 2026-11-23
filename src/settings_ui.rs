use crate::config::{AppConfig, KeybindSlot, OpenMonitor, ScrollModifier};
use egui::{Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, RichText, Stroke, Ui, Vec2};
use egui_glass_ui::{
    discover_installed_fonts, draw_status_pill, glass_button, glass_card, glass_keybind_row,
    glass_list_row, glass_segmented_control, glass_slider_row, glass_toggle_row, GlassPalette,
    GlassTintPreset, KeybindRowAction, SystemFontOption, EDITOR_FONT_FAMILY,
};
use std::collections::HashMap;

pub const OPTIONS_WINDOW_TITLE: &str = "Rapid Notes Settings";
pub const OPTIONS_WINDOW_WIDTH: f32 = 524.0;
pub const OPTIONS_WINDOW_HEIGHT: f32 = 568.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Editor,
    Appearance,
    Keybinds,
    System,
}

impl SettingsTab {
    pub const TABS: [(Self, &'static str); 4] = [
        (Self::Editor, "Editor"),
        (Self::Appearance, "Appearance"),
        (Self::Keybinds, "Keybinds"),
        (Self::System, "System"),
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingTarget {
    GlobalToggle,
    InApp(KeybindSlot),
}

pub struct SettingsBuffers {
    pub installed_fonts: Vec<SystemFontOption>,
    pub notes_dir_input: String,
    pub global_hotkey_input: String,
    pub global_hotkey_feedback: Option<(String, bool)>,
    pub in_app_keybind_inputs: HashMap<KeybindSlot, String>,
    pub in_app_keybind_feedback: Option<(String, bool)>,
    pub recording_target: Option<RecordingTarget>,
}

impl SettingsBuffers {
    pub fn from_config(config: &AppConfig, active_notes_dir: &str) -> Self {
        let mut in_app_keybind_inputs = HashMap::new();
        for slot in KeybindSlot::ALL_IN_APP {
            in_app_keybind_inputs.insert(slot, config.keybinds.get(slot).to_string());
        }
        Self {
            installed_fonts: discover_installed_fonts(),
            notes_dir_input: active_notes_dir.to_string(),
            global_hotkey_input: config.keybinds.global_toggle_app.clone(),
            global_hotkey_feedback: None,
            in_app_keybind_inputs,
            in_app_keybind_feedback: None,
            recording_target: None,
        }
    }

    pub fn sync_keybind_inputs_from_config(&mut self, config: &AppConfig) {
        self.global_hotkey_input = config.keybinds.global_toggle_app.clone();
        for slot in KeybindSlot::ALL_IN_APP {
            self.in_app_keybind_inputs
                .insert(slot, config.keybinds.get(slot).to_string());
        }
    }
}

pub fn draw_settings_tab_icon(painter: &Painter, center: Pos2, tab: SettingsTab, color: Color32) {
    let stroke = Stroke::new(1.25_f32, color);
    match tab {
        SettingsTab::Editor => {
            let page = Rect::from_center_size(center, Vec2::new(8.5, 10.0));
            painter.rect_stroke(page, CornerRadius::same(2), stroke, egui::StrokeKind::Middle);
            painter.line_segment(
                [Pos2::new(center.x - 2.3, center.y - 2.0), Pos2::new(center.x + 2.3, center.y - 2.0)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(center.x - 2.3, center.y + 1.2), Pos2::new(center.x + 1.5, center.y + 1.2)],
                stroke,
            );
        }
        SettingsTab::Appearance => {
            let back = Rect::from_center_size(center + Vec2::new(2.0, -1.5), Vec2::splat(7.2));
            let front = Rect::from_center_size(center + Vec2::new(-2.0, 1.5), Vec2::splat(7.2));
            painter.rect_stroke(back, CornerRadius::same(2), stroke, egui::StrokeKind::Middle);
            painter.rect_filled(front, CornerRadius::same(2), color.gamma_multiply(0.25));
            painter.rect_stroke(front, CornerRadius::same(2), stroke, egui::StrokeKind::Middle);
        }
        SettingsTab::Keybinds => {
            let cap = Rect::from_center_size(center, Vec2::new(10.8, 8.2));
            painter.rect_stroke(cap, CornerRadius::same(2), stroke, egui::StrokeKind::Middle);
            painter.line_segment(
                [Pos2::new(center.x - 2.6, center.y), Pos2::new(center.x + 2.6, center.y)],
                stroke,
            );
        }
        SettingsTab::System => {
            let y1 = center.y - 2.5;
            let y2 = center.y + 2.5;
            painter.line_segment([Pos2::new(center.x - 5.0, y1), Pos2::new(center.x + 5.0, y1)], stroke);
            painter.line_segment([Pos2::new(center.x - 5.0, y2), Pos2::new(center.x + 5.0, y2)], stroke);
            painter.circle_filled(Pos2::new(center.x + 2.0, y1), 1.8, color);
            painter.circle_filled(Pos2::new(center.x - 2.0, y2), 1.8, color);
        }
    }
}

pub enum SettingsAction {
    FontChanged(String),
    FontSizeChanged,
    ThemeOrVisualChanged,
    WindowSizeChanged,
    CenterWindow,
    ApplyNotesDir(String),
    OpenNotesFolder,
    ApplyGlobalHotkey(String),
    ApplyInAppKeybind(KeybindSlot, String),
    ResetAllKeybinds,
    AlwaysOnTopChanged(bool),
    ConfigSaved,
}

pub fn render_editor_tab(
    ui: &mut Ui,
    palette: &GlassPalette,
    config: &mut AppConfig,
    buffers: &mut SettingsBuffers,
) -> Vec<SettingsAction> {
    let mut actions = Vec::new();

    glass_card(ui, palette, "Typography & Font", |ui| {
        ui.label(
            RichText::new(format!("Active Font: {}", config.editor_font))
                .size(12.0)
                .color(palette.secondary_text),
        );
        ui.add_space(4.0);

        for (idx, font_opt) in buffers.installed_fonts.iter().enumerate() {
            let is_active = config.editor_font.eq_ignore_ascii_case(&font_opt.name);
            let meta = if is_active {
                "Active"
            } else if font_opt.is_monospace {
                "Monospace"
            } else {
                "Proportional"
            };
            if glass_list_row(
                ui,
                ("font_choice", idx),
                palette,
                &font_opt.name,
                Some(meta),
            ) && !is_active
            {
                config.editor_font = font_opt.name.clone();
                actions.push(SettingsAction::FontChanged(font_opt.name.clone()));
            }
        }

        ui.add_space(8.0);
        if glass_slider_row(
            ui,
            palette,
            "Editor Text Size",
            &mut config.editor_font_size,
            10.0..=32.0,
            1.0,
            " px",
        ) {
            actions.push(SettingsAction::FontSizeChanged);
        }

        ui.add_space(6.0);
        egui::Frame::NONE
            .fill(palette.input_fill)
            .corner_radius(CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .stroke(Stroke::new(1.0_f32, palette.card_stroke))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new("Preview: # Rapid Notes — **bold** *italic* `2 + 2 = 4`")
                        .font(FontId::new(
                            config.editor_font_size.clamp(11.0, 20.0),
                            FontFamily::Name(EDITOR_FONT_FAMILY.into()),
                        ))
                        .color(palette.primary_text),
                );
            });
    });

    ui.add_space(10.0);

    glass_card(ui, palette, "Markdown & Inline Tools", |ui| {
        if glass_toggle_row(
            ui,
            "opt_md_hl",
            palette,
            "Markdown Syntax Highlighting",
            Some("Highlight headings, code, checklists, quotes, and links"),
            &mut config.markdown_highlighting,
        ) {
            actions.push(SettingsAction::ConfigSaved);
        }
        if glass_toggle_row(
            ui,
            "opt_inline_math",
            palette,
            "Live Inline Math & Conversions",
            Some("Evaluate arithmetic, variables, percentages, and sum/avg inline"),
            &mut config.inline_math,
        ) {
            actions.push(SettingsAction::ConfigSaved);
        }
    });

    ui.add_space(10.0);

    glass_card(ui, palette, "Notes Storage (.md Files)", |ui| {
        ui.label(
            RichText::new("Notes Folder")
                .size(12.0)
                .color(palette.secondary_text),
        );
        ui.horizontal(|ui| {
            let input_w = (ui.available_width() - 142.0).max(140.0);
            ui.add(
                egui::TextEdit::singleline(&mut buffers.notes_dir_input)
                    .desired_width(input_w),
            );
            if glass_button(ui, palette, "Apply").clicked() {
                actions.push(SettingsAction::ApplyNotesDir(
                    buffers.notes_dir_input.trim().to_string(),
                ));
            }
            if glass_button(ui, palette, "Open").clicked() {
                actions.push(SettingsAction::OpenNotesFolder);
            }
        });
    });

    actions
}

pub fn render_appearance_tab(
    ui: &mut Ui,
    palette: &GlassPalette,
    config: &mut AppConfig,
) -> Vec<SettingsAction> {
    let mut actions = Vec::new();

    glass_card(ui, palette, "Glass Backdrop", |ui| {
        ui.label(
            RichText::new("Glass Theme")
                .size(12.0)
                .color(palette.secondary_text),
        );
        if let Some(preset) = glass_segmented_control(
            ui,
            "rn_glass_preset",
            palette,
            &[
                (GlassTintPreset::Dark, GlassTintPreset::Dark.label()),
                (GlassTintPreset::Light, GlassTintPreset::Light.label()),
                (GlassTintPreset::System, GlassTintPreset::System.label()),
            ],
            Some(config.glass_tint_preset),
        ) {
            config.glass_tint_preset = preset;
            actions.push(SettingsAction::ThemeOrVisualChanged);
        }

        ui.add_space(8.0);
        let mut pct = (config.glass_tint_opacity * 100.0).round();
        if glass_slider_row(
            ui,
            palette,
            "Glass Tint Opacity",
            &mut pct,
            10.0..=95.0,
            1.0,
            "%",
        ) {
            config.glass_tint_opacity = (pct / 100.0).clamp(0.05, 0.95);
            actions.push(SettingsAction::ThemeOrVisualChanged);
        }

        ui.add_space(4.0);
        if glass_toggle_row(
            ui,
            "rn_always_show_chrome",
            palette,
            "Always Show Top & Bottom Bars",
            Some("Keep toolbars visible instead of revealing on hover"),
            &mut config.always_show_chrome,
        ) {
            actions.push(SettingsAction::ConfigSaved);
        }
    });

    ui.add_space(10.0);

    glass_card(ui, palette, "Window Geometry", |ui| {
        if glass_slider_row(
            ui,
            palette,
            "Window Width",
            &mut config.window_size[0],
            360.0..=1200.0,
            10.0,
            " px",
        ) {
            actions.push(SettingsAction::WindowSizeChanged);
        }

        ui.add_space(4.0);
        if glass_slider_row(
            ui,
            palette,
            "Window Height",
            &mut config.window_size[1],
            280.0..=900.0,
            10.0,
            " px",
        ) {
            actions.push(SettingsAction::WindowSizeChanged);
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Move window back to primary screen center")
                    .size(11.5)
                    .color(palette.muted_text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if glass_button(ui, palette, "Center Window").clicked() {
                    actions.push(SettingsAction::CenterWindow);
                }
            });
        });
    });

    actions
}

pub fn render_keybinds_tab(
    ui: &mut Ui,
    palette: &GlassPalette,
    config: &mut AppConfig,
    buffers: &mut SettingsBuffers,
) -> Vec<SettingsAction> {
    let mut actions = Vec::new();

    glass_card(ui, palette, "Global Summon & Scroll Switching", |ui| {
        let is_rec_global = buffers.recording_target == Some(RecordingTarget::GlobalToggle);
        match glass_keybind_row(
            ui,
            "kb_global_toggle",
            palette,
            "Summon / Hide Rapid Notes (Global)",
            Some("System-wide; recording distinguishes Left/Right modifiers (LShift, RShift, ...)"),
            &mut buffers.global_hotkey_input,
            is_rec_global,
            "Win+Shift+A",
        ) {
            KeybindRowAction::Apply(val) => {
                buffers.recording_target = None;
                actions.push(SettingsAction::ApplyGlobalHotkey(val));
            }
            KeybindRowAction::ToggleRecord => {
                buffers.recording_target = if is_rec_global {
                    None
                } else {
                    Some(RecordingTarget::GlobalToggle)
                };
            }
            KeybindRowAction::Reset => {
                buffers.recording_target = None;
                buffers.global_hotkey_input = "Win+Shift+A".to_string();
                actions.push(SettingsAction::ApplyGlobalHotkey("Win+Shift+A".to_string()));
            }
            KeybindRowAction::None => {}
        }

        if let Some((ref msg, ok)) = buffers.global_hotkey_feedback {
            ui.add_space(4.0);
            let dot = if ok {
                palette.success_text
            } else {
                palette.danger_text
            };
            draw_status_pill(ui, palette, dot, msg);
        }

        ui.add_space(8.0);
        ui.label(
            RichText::new("Horizontal Scroll-Switch Modifier (+ Mouse Wheel)")
                .size(12.0)
                .color(palette.secondary_text),
        );
        if let Some(m) = glass_segmented_control(
            ui,
            "kb_scroll_mod",
            palette,
            &[
                (ScrollModifier::Alt, ScrollModifier::Alt.label()),
                (ScrollModifier::Ctrl, ScrollModifier::Ctrl.label()),
                (ScrollModifier::Shift, ScrollModifier::Shift.label()),
                (ScrollModifier::CtrlAlt, ScrollModifier::CtrlAlt.label()),
            ],
            Some(config.keybinds.scroll_switch_modifier),
        ) {
            config.keybinds.scroll_switch_modifier = m;
            actions.push(SettingsAction::ConfigSaved);
        }
    });

    ui.add_space(10.0);

    glass_card(ui, palette, "In-App Shortcuts", |ui| {
        if let Some((ref msg, ok)) = buffers.in_app_keybind_feedback {
            let dot = if ok {
                palette.success_text
            } else {
                palette.danger_text
            };
            draw_status_pill(ui, palette, dot, msg);
            ui.add_space(6.0);
        }

        for slot in KeybindSlot::ALL_IN_APP {
            let is_rec = buffers.recording_target == Some(RecordingTarget::InApp(slot));
            let buf = buffers
                .in_app_keybind_inputs
                .entry(slot)
                .or_insert_with(|| config.keybinds.get(slot).to_string());

            match glass_keybind_row(
                ui,
                ("kb_in_app", slot),
                palette,
                slot.label(),
                None,
                buf,
                is_rec,
                slot.default_shortcut(),
            ) {
                KeybindRowAction::Apply(val) => {
                    buffers.recording_target = None;
                    actions.push(SettingsAction::ApplyInAppKeybind(slot, val));
                }
                KeybindRowAction::ToggleRecord => {
                    buffers.recording_target = if is_rec {
                        None
                    } else {
                        Some(RecordingTarget::InApp(slot))
                    };
                }
                KeybindRowAction::Reset => {
                    buffers.recording_target = None;
                    let def = slot.default_shortcut().to_string();
                    *buf = def.clone();
                    actions.push(SettingsAction::ApplyInAppKeybind(slot, def));
                }
                KeybindRowAction::None => {}
            }
            ui.add_space(6.0);
        }

        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if glass_button(ui, palette, "Reset All Keybinds").clicked() {
                    buffers.recording_target = None;
                    actions.push(SettingsAction::ResetAllKeybinds);
                }
            });
        });
    });

    actions
}

pub fn render_system_tab(
    ui: &mut Ui,
    palette: &GlassPalette,
    config: &mut AppConfig,
) -> Vec<SettingsAction> {
    let mut actions = Vec::new();

    glass_card(ui, palette, "Window & Tray Behavior", |ui| {
        ui.label(
            RichText::new("Open App On")
                .size(12.5)
                .color(palette.primary_text),
        );
        ui.label(
            RichText::new("Choose which monitor centers Rapid Notes when summoned")
                .size(10.5)
                .color(palette.muted_text),
        );
        ui.add_space(3.0);
        if let Some(mon) = glass_segmented_control(
            ui,
            "rn_sys_open_mon",
            palette,
            &[
                (OpenMonitor::MainMonitor, OpenMonitor::MainMonitor.label()),
                (OpenMonitor::MouseMonitor, OpenMonitor::MouseMonitor.label()),
            ],
            Some(config.open_monitor),
        ) {
            config.open_monitor = mon;
            actions.push(SettingsAction::ConfigSaved);
        }

        ui.add_space(6.0);
        let mut aot = config.always_on_top;
        if glass_toggle_row(
            ui,
            "rn_sys_aot",
            palette,
            "Always on Top",
            Some("Float Rapid Notes above other windows when summoned"),
            &mut aot,
        ) {
            actions.push(SettingsAction::AlwaysOnTopChanged(aot));
        }

        if glass_toggle_row(
            ui,
            "rn_sys_start_hidden",
            palette,
            "Start Hidden in System Tray",
            Some("Launch quietly in the tray until summoned with your global hotkey"),
            &mut config.start_hidden,
        ) {
            actions.push(SettingsAction::ConfigSaved);
        }

        if glass_toggle_row(
            ui,
            "rn_sys_hide_blur",
            palette,
            "Hide When Window Loses Focus",
            Some("Hide to background tray waiting for your open keybind when clicking elsewhere"),
            &mut config.hide_on_focus_loss,
        ) {
            actions.push(SettingsAction::ConfigSaved);
        }

        if glass_toggle_row(
            ui,
            "rn_sys_auto_del",
            palette,
            "Auto-Delete Empty Notes on Switch",
            Some("Clean up blank scratchpad notes when switching away"),
            &mut config.auto_delete_empty,
        ) {
            actions.push(SettingsAction::ConfigSaved);
        }
    });

    actions
}
