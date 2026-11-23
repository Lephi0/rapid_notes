#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod icons;
mod keybinds;
mod markdown;
mod math;
mod notes;
mod settings_ui;
mod tray;

use config::{AppConfig, OpenMonitor};
use egui::{
    text::{CCursor, CCursorRange},
    Align, Align2, Color32, Context, CornerRadius, CursorIcon, FontFamily, FontId, Frame, Id,
    Key, Margin, PointerButton, Pos2, Rect, RichText, Sense, Stroke, TextEdit, Vec2,
    ViewportBuilder, ViewportCommand, ViewportId, WindowLevel,
};
use egui_glass_ui::{
    apply_glass_visuals, compute_overlay_rect_in_work_area, configure_tray_only_window,
    draw_glass_surface, draw_glass_tab_bar_with_icons, extract_hwnd,
    get_cursor_monitor_work_area_px, get_foreground_hwnd, get_monitor_work_area_for_hwnd,
    get_primary_monitor_work_area_px, get_window_pos_px, glass_button, glass_grid_card,
    glass_icon_button, glass_pagination_dots, glass_primary_button, glass_toast_pill,
    hide_window, hover_chrome_opacity, install_glass_fonts, is_pos_on_any_monitor,
    is_window_foreground, is_windows_light_theme, position_window_px_no_activate,
    resize_window_px, resolve_window_pos_in_work_area, set_window_cloaked,
    set_window_pos_px_checked, show_window_at_px, show_window_to_front,
    trim_process_working_set, unregister_raw_keyboard_input, BackgroundMode, GlassPalette,
    GlassStyle, GlassTintPreset, GlassWindow, GlassWindowBackdrop, GlassWindowConfig,
    EDITOR_FONT_FAMILY,
};
use icons::{
    draw_chevron_left_icon, draw_chevron_right_icon, draw_copy_icon, draw_edit_icon,
    draw_eye_icon, draw_gear_icon, draw_grid_icon, draw_hide_icon, draw_mini_app_badge,
    draw_pin_icon, draw_plus_icon, draw_search_icon, draw_trash_icon, draw_undo_icon,
};
use keybinds::{
    capture_shortcut, normalize_shortcut, poll_in_app_action, GlobalHotkeyController, InAppAction,
    ScrollNoteSwitcher, SharedWakeState,
};
use markdown::{
    build_markdown_layout_job, handle_smart_enter, render_markdown_preview,
    toggle_checklist_at_cursor, wrap_selection,
};
use math::{evaluate_note_math, LineMathResult};
use notes::{format_relative_time, word_and_char_count, NoteStore};
use settings_ui::{
    draw_settings_tab_icon, render_appearance_tab, render_editor_tab, render_keybinds_tab,
    render_system_tab, RecordingTarget, SettingsAction, SettingsBuffers, SettingsTab,
    OPTIONS_WINDOW_HEIGHT, OPTIONS_WINDOW_TITLE, OPTIONS_WINDOW_WIDTH,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tray::{generate_icon_rgba, TrayAction, TrayController};

const APP_WINDOW_TITLE: &str = "Rapid Notes";
const EDITOR_WIDGET_ID: &str = "rapid_notes_editor";
const SEARCH_WIDGET_ID: &str = "rapid_notes_search";
const TOP_BAR_HEIGHT: f32 = 44.0;
const BOTTOM_BAR_HEIGHT: f32 = 42.0;
const TOP_BAR_BUTTON_SIZE: f32 = 32.0;
const BOTTOM_BAR_BUTTON_SIZE: f32 = 30.0;
const GRID_HEADER_HEIGHT: f32 = 48.0;
const SLIDE_ANIM_SECS: f32 = 0.16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Editor,
    Grid,
}

fn main() -> eframe::Result<()> {
    let config = AppConfig::load();
    let notes = NoteStore::load(
        config.effective_notes_dir(),
        &config.note_order,
        config.active_note_file.as_deref(),
    );

    let (icon_rgba, icon_w, icon_h) = generate_icon_rgba();
    let app_icon = egui::IconData {
        rgba: icon_rgba,
        width: icon_w,
        height: icon_h,
    };

    let viewport = ViewportBuilder::default()
        .with_title(APP_WINDOW_TITLE)
        .with_inner_size(config.window_size)
        .with_min_inner_size(config.window_size)
        .with_max_inner_size(config.window_size)
        .with_decorations(false)
        .with_minimize_button(false)
        .with_maximize_button(false)
        .with_close_button(false)
        .with_transparent(true)
        .with_taskbar(false)
        .with_resizable(false)
        .with_visible(!config.start_hidden)
        .with_icon(app_icon.clone())
        .with_window_level(if config.always_on_top {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        });

    let native_options = eframe::NativeOptions {
        viewport,
        depth_buffer: 0,
        stencil_buffer: 0,
        multisampling: 0,
        vsync: true,
        ..Default::default()
    };

    eframe::run_native(
        APP_WINDOW_TITLE,
        native_options,
        Box::new(|cc| Ok(Box::new(RapidNotesApp::new(cc, config, notes, app_icon)))),
    )
}

struct RapidNotesApp {
    config: AppConfig,
    notes: NoteStore,
    hwnd: Option<isize>,
    windows_is_light: bool,
    backdrop: GlassWindowBackdrop,
    last_applied_size: Option<(Vec2, [i32; 2])>,
    startup_frames: u8,

    // Visibility & OS integration
    wake_state: Arc<SharedWakeState>,
    hotkey_controller: GlobalHotkeyController,
    tray_controller: TrayController,
    window_hidden: bool,
    had_focus_since_show: bool,
    foreground_at_summon: Option<isize>,
    focus_retry_remaining: u8,
    shown_at: Instant,

    // Persistence tracking
    last_observed_pos_px: Option<[i32; 2]>,
    pos_dirty_since: Option<Instant>,
    notes_dirty_since: Option<Instant>,

    // View & Grid overlay state
    view_mode: ViewMode,
    grid_overlay_logical_size: Option<Vec2>,
    saved_editor_pos_px: Option<[i32; 2]>,
    pending_cloak_rect_px: Option<([i32; 2], [i32; 2])>,
    uncloak_frames_remaining: u8,
    preview_mode: bool,
    search_open: bool,
    search_query: String,
    grid_selected_idx: usize,
    editor_focus_pending: bool,
    search_focus_pending: bool,
    pending_cursor_pos: Option<(usize, usize)>,

    // Animations & feedback
    slide_started_at: Option<Instant>,
    slide_dir: f32,
    scroll_note_switcher: ScrollNoteSwitcher,
    pending_scroll_switch_dir: i32,
    nav_flash_until: Option<Instant>,
    toast: Option<(String, Instant)>,

    // Resize grip state
    resizing_drag: bool,
    resize_start_cursor: Pos2,
    resize_start_size: Vec2,

    // Secondary Settings window
    settings_window: Option<GlassWindow>,
    active_settings_tab: SettingsTab,
    settings_buffers: SettingsBuffers,
}

impl RapidNotesApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        mut config: AppConfig,
        notes: NoteStore,
        app_icon: egui::IconData,
    ) -> Self {
        let resolved_font = install_glass_fonts(&cc.egui_ctx, &config.editor_font);
        config.editor_font = resolved_font;
        config.note_order = notes.note_order();
        config.active_note_file = notes.active_filename();

        let hwnd = extract_hwnd(cc);
        configure_tray_only_window(hwnd);
        // Remove winit's unused HID_USAGE_GENERIC_KEYBOARD Raw Input registration so Windows 11
        // does not bypass WH_KEYBOARD_LL hooks (e.g., PowerToys Keyboard Manager) on first focus.
        let _ = unregister_raw_keyboard_input();
        let windows_is_light = is_windows_light_theme();

        let wake_state = SharedWakeState::new();
        wake_state.set_ctx(cc.egui_ctx.clone());
        wake_state.set_hwnd(hwnd);
        wake_state.set_always_on_top(config.always_on_top);
        wake_state.set_hidden(config.start_hidden);

        let hotkey_controller =
            GlobalHotkeyController::new(&config.keybinds.global_toggle_app, Arc::clone(&wake_state));
        let tray_controller = TrayController::new(config.always_on_top, Arc::clone(&wake_state));

        let ppp = cc.egui_ctx.pixels_per_point().max(1.0);
        let size_px = [
            (config.window_size[0] * ppp).round() as i32,
            (config.window_size[1] * ppp).round() as i32,
        ];
        let initial_pos_px = Self::compute_open_pos_px(&config, size_px);
        let target_px =
            set_window_pos_px_checked(hwnd, Some(initial_pos_px), size_px[0], size_px[1]);
        config.window_pos = Some([target_px[0] as f32, target_px[1] as f32]);
        let _ = config.save();

        if config.start_hidden {
            hide_window(hwnd);
        } else {
            show_window_to_front(hwnd, config.always_on_top);
        }

        let settings_window = Some(GlassWindow::new(
            GlassWindowConfig::new(
                "rapid_notes_settings_viewport",
                OPTIONS_WINDOW_TITLE,
                Vec2::new(OPTIONS_WINDOW_WIDTH, OPTIONS_WINDOW_HEIGHT),
            )
            .with_taskbar(false)
            .with_always_on_top(true)
            .with_icon(app_icon),
        ));

        let active_dir_str = notes.notes_dir.to_string_lossy().to_string();
        let mut settings_buffers = SettingsBuffers::from_config(&config, &active_dir_str);
        if let Some(ref err) = hotkey_controller.status_error {
            settings_buffers.global_hotkey_feedback = Some((err.clone(), false));
        } else {
            settings_buffers.global_hotkey_feedback = Some((
                format!(
                    "Global shortcut active: {}",
                    hotkey_controller.active_shortcut_str
                ),
                true,
            ));
        }

        let grid_selected_idx = notes.active_idx;
        let window_hidden = config.start_hidden;

        let mut app = Self {
            config,
            notes,
            hwnd,
            windows_is_light,
            backdrop: GlassWindowBackdrop::new(),
            last_applied_size: None,
            startup_frames: 0,
            wake_state,
            hotkey_controller,
            tray_controller,
            window_hidden,
            had_focus_since_show: !window_hidden && is_window_foreground(hwnd),
            foreground_at_summon: None,
            focus_retry_remaining: 0,
            shown_at: Instant::now(),
            last_observed_pos_px: Some(target_px),
            pos_dirty_since: None,
            notes_dirty_since: None,
            view_mode: ViewMode::Editor,
            grid_overlay_logical_size: None,
            saved_editor_pos_px: None,
            pending_cloak_rect_px: None,
            uncloak_frames_remaining: 0,
            preview_mode: false,
            search_open: false,
            search_query: String::new(),
            grid_selected_idx,
            editor_focus_pending: !window_hidden,
            search_focus_pending: false,
            pending_cursor_pos: None,
            slide_started_at: None,
            slide_dir: 0.0,
            scroll_note_switcher: ScrollNoteSwitcher::default(),
            pending_scroll_switch_dir: 0,
            nav_flash_until: None,
            toast: None,
            resizing_drag: false,
            resize_start_cursor: Pos2::ZERO,
            resize_start_size: Vec2::ZERO,
            settings_window,
            active_settings_tab: SettingsTab::default(),
            settings_buffers,
        };

        app.ensure_dwm_synced();
        app
    }

    fn compute_open_pos_px(config: &AppConfig, size_px: [i32; 2]) -> [i32; 2] {
        match config.open_monitor {
            OpenMonitor::MainMonitor => {
                let primary_work = get_primary_monitor_work_area_px();
                let saved_px = config
                    .window_pos
                    .map(|[x, y]| [x.round() as i32, y.round() as i32]);
                resolve_window_pos_in_work_area(saved_px, size_px, primary_work)
            }
            OpenMonitor::MouseMonitor => {
                let mouse_work = get_cursor_monitor_work_area_px();
                resolve_window_pos_in_work_area(None, size_px, mouse_work)
            }
        }
    }

    fn editor_logical_size(&self) -> Vec2 {
        Vec2::new(self.config.window_size[0], self.config.window_size[1])
    }

    fn desired_window_size(&self) -> Vec2 {
        match self.view_mode {
            ViewMode::Editor => self.editor_logical_size(),
            ViewMode::Grid => self
                .grid_overlay_logical_size
                .unwrap_or_else(|| self.editor_logical_size()),
        }
    }

    fn current_glass_style(&self) -> GlassStyle {
        GlassStyle::new(
            self.config.glass_tint_preset,
            self.config.glass_tint_opacity,
            self.windows_is_light,
        )
    }

    fn ensure_dwm_synced(&mut self) {
        if self.config.glass_tint_preset == GlassTintPreset::System {
            self.windows_is_light = is_windows_light_theme();
        }
        let is_light = self.current_glass_style().is_light();
        self.backdrop
            .sync(self.hwnd, BackgroundMode::FrostedGlass, is_light);
    }

    fn ensure_window_size_synced(&mut self, ctx: &Context) {
        let target = self.desired_window_size();
        let ppp = ctx.pixels_per_point().max(1.0);
        let target_px = [
            (target.x * ppp).round() as i32,
            (target.y * ppp).round() as i32,
        ];
        if self.last_applied_size != Some((target, target_px)) {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::MinInnerSize(target));
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::MaxInnerSize(target));
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::InnerSize(target));
            resize_window_px(self.hwnd, target_px[0], target_px[1]);
            self.last_applied_size = Some((target, target_px));
            self.backdrop.invalidate();
        }
    }

    fn apply_viewport_geometry_cloaked(
        &mut self,
        ctx: &Context,
        logical_size: Vec2,
        pos_px: [i32; 2],
        size_px: [i32; 2],
    ) {
        set_window_cloaked(self.hwnd, true);
        self.pending_cloak_rect_px = Some((pos_px, size_px));
        self.uncloak_frames_remaining = 2;
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::MinInnerSize(logical_size));
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::MaxInnerSize(logical_size));
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::InnerSize(logical_size));
        position_window_px_no_activate(self.hwnd, pos_px, size_px);
        self.last_applied_size = Some((logical_size, size_px));
        self.backdrop.invalidate();
        ctx.request_repaint();
    }

    fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn sync_config_note_metadata(&mut self) {
        self.config.note_order = self.notes.note_order();
        self.config.active_note_file = self.notes.active_filename();
    }

    fn flush_dirty_notes_and_config(&mut self) {
        self.notes.save_all_dirty();
        self.notes_dirty_since = None;
        self.sync_config_note_metadata();
        let _ = self.config.save();
    }

    fn hide_to_tray(&mut self, ctx: &Context) {
        self.flush_dirty_notes_and_config();

        // Reset Grid overlay back to Editor geometry cleanly while hiding so next summon opens Editor
        if self.view_mode == ViewMode::Grid {
            self.view_mode = ViewMode::Editor;
            self.grid_overlay_logical_size = None;
            self.search_open = false;
            self.search_query.clear();
            let editor_size = self.editor_logical_size();
            let ppp = ctx.pixels_per_point().max(1.0);
            let size_px = [
                (editor_size.x * ppp).round() as i32,
                (editor_size.y * ppp).round() as i32,
            ];
            let restore_px = self
                .saved_editor_pos_px
                .take()
                .unwrap_or_else(|| Self::compute_open_pos_px(&self.config, size_px));
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::MinInnerSize(editor_size));
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::MaxInnerSize(editor_size));
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::InnerSize(editor_size));
            position_window_px_no_activate(self.hwnd, restore_px, size_px);
            self.last_applied_size = Some((editor_size, size_px));
            self.last_observed_pos_px = Some(restore_px);
            self.backdrop.invalidate();
        }

        self.uncloak_frames_remaining = 0;
        self.pending_cloak_rect_px = None;
        self.window_hidden = true;
        self.had_focus_since_show = false;
        self.foreground_at_summon = None;
        self.focus_retry_remaining = 0;
        self.wake_state.set_hidden(true);
        hide_window(self.hwnd);
        set_window_cloaked(self.hwnd, false);
    }

    fn show_and_focus_window(&mut self, ctx: &Context) {
        self.window_hidden = false;
        self.shown_at = Instant::now();
        self.wake_state.set_hidden(false);

        let editor_size = self.editor_logical_size();
        let ppp = ctx.pixels_per_point().max(1.0);
        let size_px = [
            (editor_size.x * ppp).round() as i32,
            (editor_size.y * ppp).round() as i32,
        ];
        let pos_px = Self::compute_open_pos_px(&self.config, size_px);
        self.last_observed_pos_px = Some(pos_px);
        self.last_applied_size = Some((editor_size, size_px));

        let prev_fg = get_foreground_hwnd().filter(|&fg| Some(fg) != self.hwnd);

        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::MinInnerSize(editor_size));
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::MaxInnerSize(editor_size));
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::InnerSize(editor_size));
        configure_tray_only_window(self.hwnd);
        show_window_at_px(self.hwnd, pos_px, size_px, self.config.always_on_top);
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);

        let got_focus = is_window_foreground(self.hwnd);
        self.had_focus_since_show = got_focus;
        self.foreground_at_summon = if got_focus { None } else { prev_fg };
        self.focus_retry_remaining = if got_focus { 0 } else { 3 };

        self.backdrop.invalidate();
        self.ensure_dwm_synced();
        if self.view_mode == ViewMode::Editor && !self.preview_mode {
            self.editor_focus_pending = true;
        }
        ctx.request_repaint();
    }

    fn toggle_visibility_from_summon(&mut self, ctx: &Context) {
        if !self.window_hidden
            && is_window_foreground(self.hwnd)
            && self.shown_at.elapsed() >= Duration::from_millis(180)
        {
            self.hide_to_tray(ctx);
        } else {
            self.show_and_focus_window(ctx);
        }
    }

    fn enter_grid_mode(&mut self, ctx: &Context) {
        if self.view_mode == ViewMode::Grid {
            return;
        }
        self.notes.save_all_dirty();
        self.saved_editor_pos_px = get_window_pos_px(self.hwnd).or(self.last_observed_pos_px);
        let work_rect = get_monitor_work_area_for_hwnd(self.hwnd, self.saved_editor_pos_px);
        let ppp = ctx.pixels_per_point().max(1.0);
        let min_w_px = (680.0 * ppp).round() as i32;
        let min_h_px = (440.0 * ppp).round() as i32;
        let margin_px = (24.0 * ppp).round() as i32;

        let (overlay_pos_px, overlay_size_px) =
            compute_overlay_rect_in_work_area(work_rect, 0.72, 0.70, [min_w_px, min_h_px], margin_px);
        let overlay_logical = Vec2::new(
            (overlay_size_px[0] as f32 / ppp).round(),
            (overlay_size_px[1] as f32 / ppp).round(),
        );

        self.view_mode = ViewMode::Grid;
        self.grid_overlay_logical_size = Some(overlay_logical);
        self.grid_selected_idx = self.notes.active_idx;
        self.search_open = false;
        self.search_query.clear();
        self.pos_dirty_since = None;

        self.apply_viewport_geometry_cloaked(ctx, overlay_logical, overlay_pos_px, overlay_size_px);
    }

    fn exit_grid_mode(&mut self, ctx: &Context, focus_editor: bool) {
        if self.view_mode != ViewMode::Grid {
            return;
        }

        let editor_size = self.editor_logical_size();
        let ppp = ctx.pixels_per_point().max(1.0);
        let size_px = [
            (editor_size.x * ppp).round() as i32,
            (editor_size.y * ppp).round() as i32,
        ];
        let restored_px = match self.saved_editor_pos_px.take() {
            Some([x, y]) if is_pos_on_any_monitor(x, y, size_px[0], size_px[1]) => [x, y],
            _ => Self::compute_open_pos_px(&self.config, size_px),
        };

        self.view_mode = ViewMode::Editor;
        self.grid_overlay_logical_size = None;
        self.search_open = false;
        self.search_query.clear();
        self.last_observed_pos_px = Some(restored_px);
        self.pos_dirty_since = None;

        if focus_editor && !self.preview_mode {
            self.editor_focus_pending = true;
        }

        self.apply_viewport_geometry_cloaked(ctx, editor_size, restored_px, size_px);
    }

    fn set_always_on_top(&mut self, ctx: &Context, enabled: bool) {
        self.config.always_on_top = enabled;
        self.wake_state.set_always_on_top(enabled);
        let level = if enabled {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        };
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::WindowLevel(level));
        self.backdrop.invalidate();
        self.ensure_dwm_synced();
        self.tray_controller.sync_always_on_top(enabled);
        let _ = self.config.save();
    }

    fn center_window_on_screen(&mut self, ctx: &Context) {
        if self.view_mode == ViewMode::Grid {
            self.exit_grid_mode(ctx, false);
        }
        let size = self.editor_logical_size();
        let ppp = ctx.pixels_per_point().max(1.0);
        let size_px = [
            (size.x * ppp).round() as i32,
            (size.y * ppp).round() as i32,
        ];
        let work_rect = match self.config.open_monitor {
            OpenMonitor::MainMonitor => get_primary_monitor_work_area_px(),
            OpenMonitor::MouseMonitor => get_cursor_monitor_work_area_px(),
        };
        let centered = resolve_window_pos_in_work_area(None, size_px, work_rect);
        let actual_px = set_window_pos_px_checked(self.hwnd, Some(centered), size_px[0], size_px[1]);
        self.last_observed_pos_px = Some(actual_px);
        self.last_applied_size = None;
        self.backdrop.invalidate();
        self.config.window_pos = Some([actual_px[0] as f32, actual_px[1] as f32]);
        let _ = self.config.save();
        if !self.window_hidden {
            show_window_to_front(self.hwnd, self.config.always_on_top);
        }
    }

    fn open_notes_folder_in_explorer(&self) {
        let _ = std::fs::create_dir_all(&self.notes.notes_dir);
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("explorer.exe")
                .arg(&self.notes.notes_dir)
                .spawn();
        }
    }

    fn open_settings_window(&mut self, ctx: &Context) {
        let saved_px = self
            .config
            .settings_window_pos
            .map(|[x, y]| [x.round() as i32, y.round() as i32]);
        if let Some(win) = &mut self.settings_window {
            win.open(ctx, self.hwnd, self.last_observed_pos_px, saved_px);
        }
    }

    fn persist_settings_window_pos(&mut self) {
        if let Some(pos_px) = self
            .settings_window
            .as_ref()
            .and_then(|win| win.current_pos_px())
        {
            self.config.settings_window_pos = Some([pos_px[0] as f32, pos_px[1] as f32]);
        }
    }

    fn create_and_focus_new_note(&mut self, ctx: &Context) {
        self.notes.create_note();
        self.sync_config_note_metadata();
        let _ = self.config.save();
        if self.view_mode == ViewMode::Grid {
            self.exit_grid_mode(ctx, true);
        }
        self.preview_mode = false;
        self.grid_selected_idx = self.notes.active_idx;
        self.pending_cursor_pos = Some((0, 0));
        self.editor_focus_pending = true;
        self.slide_dir = 1.0;
        self.slide_started_at = Some(Instant::now());
        self.nav_flash_until = Some(Instant::now() + Duration::from_millis(1400));
        self.show_toast("New note created");
        ctx.request_repaint();
    }

    fn switch_to_note(&mut self, ctx: &Context, idx: usize, dir_hint: f32) {
        if self.notes.switch_to(idx, self.config.auto_delete_empty) {
            self.sync_config_note_metadata();
            let _ = self.config.save();
            self.grid_selected_idx = self.notes.active_idx;
            self.pending_cursor_pos = Some((0, 0));
            if self.view_mode == ViewMode::Editor && !self.preview_mode {
                self.editor_focus_pending = true;
            }
            self.slide_dir = dir_hint;
            self.slide_started_at = Some(Instant::now());
            self.nav_flash_until = Some(Instant::now() + Duration::from_millis(1400));
            ctx.request_repaint();
        }
    }

    fn step_note_horizontal(&mut self, ctx: &Context, delta: isize) {
        let prev = self.notes.active_idx;
        if let Some(new_idx) = self.notes.step_note(delta, self.config.auto_delete_empty) {
            self.sync_config_note_metadata();
            let _ = self.config.save();
            self.grid_selected_idx = new_idx;
            self.pending_cursor_pos = Some((0, 0));
            if self.view_mode == ViewMode::Editor && !self.preview_mode {
                self.editor_focus_pending = true;
            }
            self.slide_dir = if new_idx > prev { 1.0 } else { -1.0 };
            self.slide_started_at = Some(Instant::now());
            self.nav_flash_until = Some(Instant::now() + Duration::from_millis(1400));
            ctx.request_repaint();
        } else {
            self.nav_flash_until = Some(Instant::now() + Duration::from_millis(900));
        }
    }

    fn delete_note_at(&mut self, ctx: &Context, idx: usize) {
        if self.notes.delete_note(idx) {
            self.sync_config_note_metadata();
            let _ = self.config.save();
            self.grid_selected_idx = self.notes.active_idx;
            self.pending_cursor_pos = Some((0, 0));
            if self.view_mode == ViewMode::Editor && !self.preview_mode {
                self.editor_focus_pending = true;
            }
            self.show_toast("Note deleted");
            ctx.request_repaint();
        }
    }

    fn shutdown_and_close(&mut self, ctx: &Context) {
        if self.view_mode == ViewMode::Editor {
            if let Some(pos_px) = get_window_pos_px(self.hwnd).or(self.last_observed_pos_px) {
                self.config.window_pos = Some([pos_px[0] as f32, pos_px[1] as f32]);
            }
        } else if let Some(pos_px) = self.saved_editor_pos_px {
            self.config.window_pos = Some([pos_px[0] as f32, pos_px[1] as f32]);
        }
        self.persist_settings_window_pos();
        if let Some(win) = &self.settings_window {
            win.hide();
        }
        self.flush_dirty_notes_and_config();
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
    }

    fn handle_os_events(&mut self, ctx: &Context) -> bool {
        if self.wake_state.is_quit_requested() {
            self.shutdown_and_close(ctx);
            return true;
        }

        if self.hotkey_controller.poll_triggered() {
            self.toggle_visibility_from_summon(ctx);
        }

        for action in self.tray_controller.poll_actions() {
            match action {
                TrayAction::ToggleWindow => {
                    self.toggle_visibility_from_summon(ctx);
                }
                TrayAction::NewNote => {
                    self.show_and_focus_window(ctx);
                    self.create_and_focus_new_note(ctx);
                }
                TrayAction::OpenGrid => {
                    self.show_and_focus_window(ctx);
                    self.enter_grid_mode(ctx);
                }
                TrayAction::ToggleAlwaysOnTop => {
                    let next = !self.config.always_on_top;
                    self.set_always_on_top(ctx, next);
                }
                TrayAction::OpenNotesFolder => {
                    self.open_notes_folder_in_explorer();
                }
                TrayAction::OpenSettings => {
                    self.show_and_focus_window(ctx);
                    self.open_settings_window(ctx);
                }
                TrayAction::Quit => {
                    self.shutdown_and_close(ctx);
                    return true;
                }
            }
        }

        if self.window_hidden {
            hide_window(self.hwnd);
            set_window_cloaked(self.hwnd, false);
        }

        false
    }

    fn should_hide_on_focus_loss(
        hide_on_focus_loss: bool,
        is_focused: bool,
        had_focus_since_show: bool,
        shown_elapsed: Duration,
        startup_frames: u8,
        uncloak_frames_remaining: u8,
        resizing_drag: bool,
        settings_open: bool,
        self_hwnd: Option<isize>,
        foreground_at_summon: Option<isize>,
        current_foreground: Option<isize>,
    ) -> bool {
        if !hide_on_focus_loss
            || is_focused
            || startup_frames < 5
            || uncloak_frames_remaining > 0
            || resizing_drag
            || settings_open
        {
            return false;
        }

        if had_focus_since_show {
            // Brief settle window so an activation bounce during reveal cannot close immediately.
            return shown_elapsed >= Duration::from_millis(120);
        }

        // If Windows blocked initial activation (e.g., summoning over an elevated UAC window),
        // stay open unless the user actively switches to a different third window after the grace period.
        shown_elapsed >= Duration::from_millis(800)
            && foreground_at_summon.is_some()
            && current_foreground
                .is_some_and(|fg| Some(fg) != self_hwnd && Some(fg) != foreground_at_summon)
    }

    fn track_persistence_and_focus(&mut self, ctx: &Context) {
        if !self.window_hidden {
            let mut is_focused = if self.hwnd.is_some() {
                is_window_foreground(self.hwnd)
            } else {
                ctx.input(|i| i.viewport().focused == Some(true))
            };

            if is_focused {
                self.had_focus_since_show = true;
                self.focus_retry_remaining = 0;
                self.foreground_at_summon = None;
            } else if self.focus_retry_remaining > 0
                && self.uncloak_frames_remaining == 0
                && self.shown_at.elapsed() >= Duration::from_millis(30)
            {
                self.focus_retry_remaining -= 1;
                show_window_to_front(self.hwnd, self.config.always_on_top);
                ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
                is_focused = if self.hwnd.is_some() {
                    is_window_foreground(self.hwnd)
                } else {
                    ctx.input(|i| i.viewport().focused == Some(true))
                };
                if is_focused {
                    self.had_focus_since_show = true;
                    self.focus_retry_remaining = 0;
                    self.foreground_at_summon = None;
                } else {
                    ctx.request_repaint_after(Duration::from_millis(35));
                }
            }

            if !is_focused
                && Self::should_hide_on_focus_loss(
                    self.config.hide_on_focus_loss,
                    is_focused,
                    self.had_focus_since_show,
                    self.shown_at.elapsed(),
                    self.startup_frames,
                    self.uncloak_frames_remaining,
                    self.resizing_drag,
                    self.settings_window.as_ref().is_some_and(|w| w.is_open()),
                    self.hwnd,
                    self.foreground_at_summon,
                    get_foreground_hwnd(),
                )
            {
                self.hide_to_tray(ctx);
                return;
            }
        }

        // Only persist physical position when in normal Editor mode
        if !self.window_hidden && self.view_mode == ViewMode::Editor && self.uncloak_frames_remaining == 0 {
            if let Some(pos_px) = get_window_pos_px(self.hwnd) {
                let changed = match self.last_observed_pos_px {
                    Some(prev) => (prev[0] - pos_px[0]).abs() > 1 || (prev[1] - pos_px[1]).abs() > 1,
                    None => {
                        self.last_observed_pos_px = Some(pos_px);
                        false
                    }
                };
                if changed {
                    self.last_observed_pos_px = Some(pos_px);
                    self.pos_dirty_since = Some(Instant::now());
                }
            }
        }

        if let Some(dirty_at) = self.pos_dirty_since {
            let is_dragging = ctx.input(|i| i.pointer.primary_down());
            if !is_dragging && !self.resizing_drag && dirty_at.elapsed() >= Duration::from_millis(350) {
                if let Some(pos_px) = self.last_observed_pos_px {
                    self.config.window_pos = Some([pos_px[0] as f32, pos_px[1] as f32]);
                    let _ = self.config.save();
                }
                self.pos_dirty_since = None;
            }
        }

        if let Some(dirty_at) = self.notes_dirty_since {
            if dirty_at.elapsed() >= Duration::from_millis(450) {
                self.flush_dirty_notes_and_config();
            }
        }
    }

    fn execute_in_app_action(&mut self, ctx: &Context, action: InAppAction) {
        match action {
            InAppAction::ToggleGridView => match self.view_mode {
                ViewMode::Editor => self.enter_grid_mode(ctx),
                ViewMode::Grid => self.exit_grid_mode(ctx, true),
            },
            InAppAction::PrevNote => {
                if self.view_mode == ViewMode::Editor {
                    self.step_note_horizontal(ctx, -1);
                }
            }
            InAppAction::NextNote => {
                if self.view_mode == ViewMode::Editor {
                    self.step_note_horizontal(ctx, 1);
                }
            }
            InAppAction::NewNote => self.create_and_focus_new_note(ctx),
            InAppAction::DeleteNote => {
                let target = if self.view_mode == ViewMode::Grid {
                    self.grid_selected_idx
                } else {
                    self.notes.active_idx
                };
                self.delete_note_at(ctx, target);
            }
            InAppAction::OpenSettings => self.open_settings_window(ctx),
            InAppAction::HideWindow => {
                if self.search_open {
                    self.search_open = false;
                    self.search_query.clear();
                    if self.view_mode == ViewMode::Editor && !self.preview_mode {
                        self.editor_focus_pending = true;
                    }
                } else if self.view_mode == ViewMode::Grid {
                    self.exit_grid_mode(ctx, true);
                } else {
                    self.hide_to_tray(ctx);
                }
            }
            InAppAction::ToggleAlwaysOnTop => {
                let next = !self.config.always_on_top;
                self.set_always_on_top(ctx, next);
                self.show_toast(if next {
                    "Pinned on top"
                } else {
                    "Unpinned from top"
                });
            }
            InAppAction::TogglePreview => {
                if self.view_mode == ViewMode::Grid {
                    self.exit_grid_mode(ctx, false);
                }
                self.preview_mode = !self.preview_mode;
                if !self.preview_mode {
                    self.editor_focus_pending = true;
                }
            }
            InAppAction::CopyNote => {
                let idx = if self.view_mode == ViewMode::Grid {
                    self.grid_selected_idx
                } else {
                    self.notes.active_idx
                };
                if let Some(note) = self.notes.notes.get(idx) {
                    ctx.copy_text(note.content.clone());
                    self.show_toast("Copied note to clipboard");
                }
            }
            InAppAction::ToggleChecklist => {
                if self.view_mode == ViewMode::Editor && !self.preview_mode {
                    let editor_id = Id::new(EDITOR_WIDGET_ID);
                    let cursor_idx = TextEdit::load_state(ctx, editor_id)
                        .and_then(|s| s.cursor.char_range())
                        .map_or(0, |r| r.primary.index);
                    if let Some(note) = self.notes.active_note_mut() {
                        let new_idx = toggle_checklist_at_cursor(&mut note.content, cursor_idx);
                        note.dirty = true;
                        self.notes_dirty_since = Some(Instant::now());
                        self.pending_cursor_pos = Some((new_idx, new_idx));
                    }
                }
            }
            InAppAction::SearchNotes => {
                self.search_open = !self.search_open;
                if self.search_open {
                    self.search_focus_pending = true;
                } else {
                    self.search_query.clear();
                    if self.view_mode == ViewMode::Editor && !self.preview_mode {
                        self.editor_focus_pending = true;
                    }
                }
            }
            InAppAction::FormatBold | InAppAction::FormatItalic | InAppAction::FormatCode => {
                if self.view_mode == ViewMode::Editor && !self.preview_mode {
                    let delim = match action {
                        InAppAction::FormatBold => "**",
                        InAppAction::FormatItalic => "*",
                        _ => "`",
                    };
                    let editor_id = Id::new(EDITOR_WIDGET_ID);
                    let (a, b) = TextEdit::load_state(ctx, editor_id)
                        .and_then(|s| s.cursor.char_range())
                        .map_or((0, 0), |r| (r.primary.index, r.secondary.index));
                    if let Some(note) = self.notes.active_note_mut() {
                        let (na, nb) = wrap_selection(&mut note.content, a, b, delim, delim);
                        note.dirty = true;
                        self.notes_dirty_since = Some(Instant::now());
                        self.pending_cursor_pos = Some((na, nb));
                    }
                }
            }
        }
    }

    fn handle_editor_key_enhancements(&mut self, ctx: &Context, math_results: &[LineMathResult]) {
        if self.view_mode != ViewMode::Editor || self.preview_mode {
            return;
        }
        let editor_id = Id::new(EDITOR_WIDGET_ID);
        if !ctx.memory(|m| m.has_focus(editor_id)) {
            return;
        }
        let Some(state) = TextEdit::load_state(ctx, editor_id) else {
            return;
        };
        let Some(range) = state.cursor.char_range() else {
            return;
        };
        if range.primary.index != range.secondary.index {
            return;
        }
        let cursor_idx = range.primary.index;

        let (enter_pressed, tab_pressed) = ctx.input(|i| {
            let plain = !i.modifiers.ctrl && !i.modifiers.command && !i.modifiers.alt && !i.modifiers.shift;
            (
                plain && i.key_pressed(Key::Enter),
                plain && i.key_pressed(Key::Tab),
            )
        });

        if enter_pressed {
            if let Some(note) = self.notes.active_note_mut() {
                if let Some(new_idx) = handle_smart_enter(&mut note.content, cursor_idx) {
                    ctx.input_mut(|i| {
                        i.events.retain(|ev| {
                            !matches!(ev, egui::Event::Key { key: Key::Enter, pressed: true, .. })
                        });
                    });
                    note.dirty = true;
                    self.notes_dirty_since = Some(Instant::now());
                    self.pending_cursor_pos = Some((new_idx, new_idx));
                }
            }
            return;
        }

        if tab_pressed && self.config.inline_math {
            if let Some(note) = self.notes.active_note_mut() {
                let byte_pos = note
                    .content
                    .char_indices()
                    .nth(cursor_idx)
                    .map_or(note.content.len(), |(b, _)| b);
                let line_idx = note.content[..byte_pos].matches('\n').count();
                let line_end = note.content[byte_pos..]
                    .find('\n')
                    .map_or(note.content.len(), |p| byte_pos + p);
                let line_start = note.content[..byte_pos].rfind('\n').map_or(0, |p| p + 1);
                let line_trim = note.content[line_start..line_end].trim_end();

                if line_trim.ends_with('=') {
                    if let Some(math_res) = math_results.iter().find(|m| m.line_idx == line_idx) {
                        let insert_str = if note.content[..line_end].ends_with(' ') {
                            math_res.formatted.clone()
                        } else {
                            format!(" {}", math_res.formatted)
                        };
                        note.content.insert_str(line_end, &insert_str);
                        let new_byte = line_end + insert_str.len();
                        let new_char = note.content[..new_byte].chars().count();
                        ctx.input_mut(|i| {
                            i.events.retain(|ev| {
                                !matches!(ev, egui::Event::Key { key: Key::Tab, pressed: true, .. })
                            });
                        });
                        note.dirty = true;
                        self.notes_dirty_since = Some(Instant::now());
                        self.pending_cursor_pos = Some((new_char, new_char));
                    }
                }
            }
        }
    }

    fn current_slide_offset(&mut self, ctx: &Context) -> f32 {
        let Some(started) = self.slide_started_at else {
            return 0.0;
        };
        let t = (started.elapsed().as_secs_f32() / SLIDE_ANIM_SECS).clamp(0.0, 1.0);
        if t >= 1.0 {
            self.slide_started_at = None;
            return 0.0;
        }
        ctx.request_repaint();
        let eased = 1.0 - (1.0 - t).powi(3);
        self.slide_dir * 38.0 * (1.0 - eased)
    }

    fn render_top_hover_bar(
        &mut self,
        ctx: &Context,
        ui: &mut egui::Ui,
        palette: &GlassPalette,
        bar_rect: Rect,
        alpha: f32,
    ) {
        let mut new_clicked = false;
        let mut grid_clicked = false;
        let mut preview_clicked = false;
        let mut copy_clicked = false;
        let mut pin_clicked = false;
        let mut settings_clicked = false;
        let mut hide_clicked = false;

        let controls_w = 7.0 * TOP_BAR_BUTTON_SIZE + 6.0 * 4.0 + 14.0;
        let drag_rect = Rect::from_min_max(
            bar_rect.min,
            Pos2::new((bar_rect.max.x - controls_w).max(bar_rect.min.x + 60.0), bar_rect.max.y),
        );

        let start_drag = ctx.input(|i| {
            i.pointer.button_pressed(PointerButton::Primary)
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| drag_rect.contains(p))
        });
        if start_drag {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }

        if alpha > 0.01 {
            let indicator = format!(
                "Note {} / {}",
                self.notes.active_idx + 1,
                self.notes.notes.len()
            );
            ui.painter().text(
                Pos2::new(bar_rect.min.x + 14.0, bar_rect.center().y),
                Align2::LEFT_CENTER,
                &indicator,
                FontId::new(11.5, FontFamily::Proportional),
                palette.muted_text.gamma_multiply(alpha),
            );
        }

        let icons_rect = Rect::from_min_max(
            Pos2::new((bar_rect.max.x - controls_w).max(bar_rect.min.x), bar_rect.min.y),
            bar_rect.max,
        );
        let mut child = ui.new_child(egui::UiBuilder::new().id_salt("rn_top_icons").max_rect(icons_rect));
        child.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(10.0);
            ui.spacing_mut().item_spacing.x = 4.0;

            if glass_icon_button(
                ui,
                "btn_hide",
                palette,
                TOP_BAR_BUTTON_SIZE,
                false,
                alpha,
                &format!("Hide to Tray ({})", self.config.keybinds.hide_window),
                draw_hide_icon,
            )
            .clicked()
            {
                hide_clicked = true;
            }

            if glass_icon_button(
                ui,
                "btn_settings",
                palette,
                TOP_BAR_BUTTON_SIZE,
                self.settings_window.as_ref().is_some_and(|w| w.is_open()),
                alpha,
                &format!("Settings ({})", self.config.keybinds.open_settings),
                draw_gear_icon,
            )
            .clicked()
            {
                settings_clicked = true;
            }

            if glass_icon_button(
                ui,
                "btn_pin",
                palette,
                TOP_BAR_BUTTON_SIZE,
                self.config.always_on_top,
                alpha,
                &format!("Always on Top ({})", self.config.keybinds.toggle_always_on_top),
                draw_pin_icon,
            )
            .clicked()
            {
                pin_clicked = true;
            }

            if glass_icon_button(
                ui,
                "btn_copy",
                palette,
                TOP_BAR_BUTTON_SIZE,
                false,
                alpha,
                &format!("Copy Note ({})", self.config.keybinds.copy_note),
                draw_copy_icon,
            )
            .clicked()
            {
                copy_clicked = true;
            }

            let preview_icon = if self.preview_mode {
                draw_edit_icon
            } else {
                draw_eye_icon
            };
            if glass_icon_button(
                ui,
                "btn_preview",
                palette,
                TOP_BAR_BUTTON_SIZE,
                self.preview_mode,
                alpha,
                &format!("Toggle Preview ({})", self.config.keybinds.toggle_preview),
                preview_icon,
            )
            .clicked()
            {
                preview_clicked = true;
            }

            if glass_icon_button(
                ui,
                "btn_grid",
                palette,
                TOP_BAR_BUTTON_SIZE,
                false,
                alpha,
                &format!("At-a-Glance Grid ({})", self.config.keybinds.toggle_grid_view),
                draw_grid_icon,
            )
            .clicked()
            {
                grid_clicked = true;
            }

            if glass_icon_button(
                ui,
                "btn_new",
                palette,
                TOP_BAR_BUTTON_SIZE,
                false,
                alpha,
                &format!("New Note ({})", self.config.keybinds.new_note),
                draw_plus_icon,
            )
            .clicked()
            {
                new_clicked = true;
            }
        });

        if new_clicked {
            self.create_and_focus_new_note(ctx);
        }
        if grid_clicked {
            self.enter_grid_mode(ctx);
        }
        if preview_clicked {
            self.execute_in_app_action(ctx, InAppAction::TogglePreview);
        }
        if copy_clicked {
            self.execute_in_app_action(ctx, InAppAction::CopyNote);
        }
        if pin_clicked {
            self.execute_in_app_action(ctx, InAppAction::ToggleAlwaysOnTop);
        }
        if settings_clicked {
            self.open_settings_window(ctx);
        }
        if hide_clicked {
            self.hide_to_tray(ctx);
        }
    }

    fn render_bottom_hover_bar(
        &mut self,
        ctx: &Context,
        ui: &mut egui::Ui,
        palette: &GlassPalette,
        bar_rect: Rect,
        alpha: f32,
    ) {
        if alpha <= 0.01 {
            return;
        }

        let total = self.notes.notes.len();
        let active_idx = self.notes.active_idx;
        let has_undo = self.notes.last_deleted.is_some();
        let (_lines, words, chars) = self
            .notes
            .active_note()
            .map(|n| word_and_char_count(&n.content))
            .unwrap_or((0, 0, 0));

        let mut step_delta = 0isize;
        let mut jump_to: Option<usize> = None;
        let mut delete_clicked = false;
        let mut undo_clicked = false;
        let mut search_clicked = false;

        let inner = Rect::from_min_max(
            Pos2::new(bar_rect.min.x + 10.0, bar_rect.min.y + 2.0),
            Pos2::new(bar_rect.max.x - 16.0, bar_rect.max.y - 2.0),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().id_salt("rn_bottom_nav").max_rect(inner));
        child.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;

            if glass_icon_button(
                ui,
                "nav_prev",
                palette,
                BOTTOM_BAR_BUTTON_SIZE,
                false,
                alpha,
                &format!("Previous Note ({})", self.config.keybinds.prev_note),
                draw_chevron_left_icon,
            )
            .clicked()
            {
                step_delta = -1;
            }

            ui.label(
                RichText::new(format!("{} / {}", active_idx + 1, total))
                    .size(11.5)
                    .color(palette.secondary_text.gamma_multiply(alpha)),
            );

            if glass_icon_button(
                ui,
                "nav_next",
                palette,
                BOTTOM_BAR_BUTTON_SIZE,
                false,
                alpha,
                &format!("Next Note ({})", self.config.keybinds.next_note),
                draw_chevron_right_icon,
            )
            .clicked()
            {
                step_delta = 1;
            }

            ui.add_space(4.0);
            if let Some(clicked_idx) =
                glass_pagination_dots(ui, "nav_dots", palette, total, active_idx, alpha)
            {
                jump_to = Some(clicked_idx);
            }

            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;

                if glass_icon_button(
                    ui,
                    "nav_del",
                    palette,
                    BOTTOM_BAR_BUTTON_SIZE,
                    false,
                    alpha,
                    &format!("Delete Note ({})", self.config.keybinds.delete_note),
                    draw_trash_icon,
                )
                .clicked()
                {
                    delete_clicked = true;
                }

                if has_undo
                    && glass_icon_button(
                        ui,
                        "nav_undo_del",
                        palette,
                        BOTTOM_BAR_BUTTON_SIZE,
                        false,
                        alpha,
                        "Restore Last Deleted Note",
                        draw_undo_icon,
                    )
                    .clicked()
                {
                    undo_clicked = true;
                }

                if glass_icon_button(
                    ui,
                    "nav_search",
                    palette,
                    BOTTOM_BAR_BUTTON_SIZE,
                    self.search_open,
                    alpha,
                    &format!("Search Notes ({})", self.config.keybinds.search_notes),
                    draw_search_icon,
                )
                .clicked()
                {
                    search_clicked = true;
                }

                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("{words}w · {chars}c"))
                        .size(11.0)
                        .color(palette.muted_text.gamma_multiply(alpha)),
                );
            });
        });

        if step_delta != 0 {
            self.step_note_horizontal(ctx, step_delta);
        }
        if let Some(target) = jump_to {
            let dir = if target > active_idx { 1.0 } else { -1.0 };
            self.switch_to_note(ctx, target, dir);
        }
        if undo_clicked && self.notes.undo_delete() {
            self.sync_config_note_metadata();
            let _ = self.config.save();
            self.show_toast("Restored deleted note");
        }
        if delete_clicked {
            self.delete_note_at(ctx, active_idx);
        }
        if search_clicked {
            self.execute_in_app_action(ctx, InAppAction::SearchNotes);
        }
    }

    fn render_search_strip(&mut self, ctx: &Context, ui: &mut egui::Ui, palette: &GlassPalette) {
        let query_trimmed = self.search_query.trim().to_ascii_lowercase();
        let mut jump_to: Option<usize> = None;

        Frame::NONE
            .fill(palette.card_fill)
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::symmetric(10, 6))
            .stroke(Stroke::new(1.0_f32, palette.card_stroke))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let edit_w = (ui.available_width() - 64.0).max(100.0);
                    let resp = ui.add(
                        TextEdit::singleline(&mut self.search_query)
                            .id(Id::new(SEARCH_WIDGET_ID))
                            .hint_text("Search notes...")
                            .desired_width(edit_w),
                    );
                    if self.search_focus_pending {
                        resp.request_focus();
                        self.search_focus_pending = false;
                    }
                    if glass_button(ui, palette, "Close").clicked() {
                        self.search_open = false;
                        self.search_query.clear();
                    }
                });

                if !query_trimmed.is_empty() && self.view_mode == ViewMode::Editor {
                    let matches: Vec<(usize, String)> = self
                        .notes
                        .notes
                        .iter()
                        .enumerate()
                        .filter(|(_, n)| n.content.to_ascii_lowercase().contains(&query_trimmed))
                        .take(5)
                        .map(|(i, n)| (i, n.compact_excerpt(22)))
                        .collect();

                    if !matches.is_empty() {
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                            for (idx, excerpt) in matches {
                                let label = format!("#{} {excerpt}", idx + 1);
                                if glass_button(ui, palette, &label).clicked() {
                                    jump_to = Some(idx);
                                }
                            }
                        });
                    }
                }
            });

        if let Some(target) = jump_to {
            let dir = if target > self.notes.active_idx { 1.0 } else { -1.0 };
            self.switch_to_note(ctx, target, dir);
        }
    }

    fn render_editor_body(
        &mut self,
        ctx: &Context,
        ui: &mut egui::Ui,
        palette: &GlassPalette,
        content_rect: Rect,
    ) {
        let slide_x = self.current_slide_offset(ctx);
        let shifted = content_rect.translate(Vec2::new(slide_x, 0.0));
        let mut child = ui.new_child(egui::UiBuilder::new().id_salt("rn_editor_body").max_rect(shifted));
        child.set_clip_rect(content_rect);

        if self.search_open {
            self.render_search_strip(ctx, &mut child, palette);
            child.add_space(6.0);
        }

        let math_results = if self.config.inline_math {
            self.notes
                .active_note()
                .map(|n| evaluate_note_math(&n.content))
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        self.handle_editor_key_enhancements(ctx, &math_results);

        let font_size = self.config.editor_font_size;
        let highlight = self.config.markdown_highlighting;
        let palette_copy = *palette;

        let scroll_h = child.available_height().max(80.0);
        child.allocate_ui(Vec2::new(child.available_width(), scroll_h), |ui| {
            ui.set_min_size(Vec2::new(ui.available_width(), scroll_h));
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.preview_mode {
                        if let Some(note) = self.notes.active_note_mut() {
                            if render_markdown_preview(ui, &mut note.content, font_size, &palette_copy) {
                                note.dirty = true;
                                self.notes_dirty_since = Some(Instant::now());
                            }
                        }
                        return;
                    }

                    let mut layouter = |ui: &egui::Ui, text: &str, wrap_width: f32| {
                        let effective_wrap = if !math_results.is_empty() {
                            (wrap_width - 78.0).max(140.0)
                        } else {
                            wrap_width
                        };
                        let job = build_markdown_layout_job(
                            text,
                            effective_wrap,
                            font_size,
                            &palette_copy,
                            highlight,
                        );
                        ui.fonts(|f| f.layout_job(job))
                    };

                    let avail_w = ui.available_width().max(120.0);
                    let min_rows = ((scroll_h / (font_size * 1.35)).floor() as usize).max(6);

                    if let Some(note) = self.notes.active_note_mut() {
                        let editor_id = Id::new(EDITOR_WIDGET_ID);
                        let mut edit = TextEdit::multiline(&mut note.content)
                            .id(editor_id)
                            .frame(false)
                            .desired_width(avail_w)
                            .desired_rows(min_rows)
                            .hint_text("Start typing...")
                            .font(FontId::new(
                                font_size,
                                FontFamily::Name(EDITOR_FONT_FAMILY.into()),
                            ))
                            .layouter(&mut layouter);

                        if self.config.inline_math {
                            edit = edit.margin(Margin {
                                left: 2,
                                right: 8,
                                top: 2,
                                bottom: 10,
                            });
                        }

                        let mut output = edit.show(ui);

                        if self.editor_focus_pending {
                            output.response.request_focus();
                            self.editor_focus_pending = false;
                        }

                        if let Some((a, b)) = self.pending_cursor_pos.take() {
                            output
                                .state
                                .cursor
                                .set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
                            output.state.store(ctx, editor_id);
                            output.response.request_focus();
                        }

                        if output.response.changed() {
                            note.dirty = true;
                            self.notes_dirty_since = Some(Instant::now());
                        }

                        if !math_results.is_empty() {
                            let mut copied_badge: Option<String> = None;
                            let mut line_char_ends: Vec<usize> = Vec::new();
                            let mut running_chars = 0usize;
                            for raw_line in note.content.split('\n') {
                                running_chars += raw_line.chars().count();
                                line_char_ends.push(running_chars);
                                running_chars += 1;
                            }

                            for res in &math_results {
                                let Some(&end_char) = line_char_ends.get(res.line_idx) else {
                                    continue;
                                };
                                let cursor = output.galley.from_ccursor(CCursor::new(end_char));
                                let cursor_rect = output.galley.pos_from_cursor(&cursor);
                                let center_y = output.galley_pos.y + cursor_rect.center().y;

                                let badge_text = if res.has_trailing_equals {
                                    res.formatted.clone()
                                } else {
                                    format!("= {}", res.formatted)
                                };
                                let badge_font = FontId::new(
                                    (font_size * 0.80).clamp(10.0, 14.0),
                                    FontFamily::Name(EDITOR_FONT_FAMILY.into()),
                                );
                                let galley = ui.painter().layout_no_wrap(
                                    badge_text,
                                    badge_font,
                                    palette_copy.math_badge_text,
                                );
                                let pill_size = galley.size() + Vec2::new(12.0, 4.0);
                                let pill_rect = Rect::from_min_size(
                                    Pos2::new(
                                        output.response.rect.max.x - pill_size.x - 4.0,
                                        center_y - pill_size.y * 0.5,
                                    ),
                                    pill_size,
                                );
                                let badge_resp = ui
                                    .interact(
                                        pill_rect,
                                        Id::new(("math_badge", res.line_idx)),
                                        Sense::click(),
                                    )
                                    .on_hover_cursor(CursorIcon::PointingHand)
                                    .on_hover_text("Click to copy result (or press Tab after =)");

                                let fill = if badge_resp.hovered() {
                                    palette_copy.accent_fill.gamma_multiply(0.32)
                                } else {
                                    palette_copy.math_badge_fill
                                };
                                ui.painter()
                                    .rect_filled(pill_rect, CornerRadius::same(6), fill);
                                ui.painter().galley(
                                    Pos2::new(
                                        pill_rect.min.x + 6.0,
                                        pill_rect.center().y - galley.size().y * 0.5,
                                    ),
                                    galley,
                                    palette_copy.math_badge_text,
                                );

                                if badge_resp.clicked() {
                                    copied_badge = Some(res.formatted.clone());
                                }
                            }

                            if let Some(val) = copied_badge {
                                ctx.copy_text(val.clone());
                                self.show_toast(format!("Copied {val}"));
                            }
                        }
                    }
                });
        });
    }

    fn render_grid_overlay(
        &mut self,
        ctx: &Context,
        ui: &mut egui::Ui,
        palette: &GlassPalette,
        full_rect: Rect,
    ) {
        let header_rect = Rect::from_min_max(
            full_rect.min,
            Pos2::new(full_rect.max.x, full_rect.min.y + GRID_HEADER_HEIGHT),
        );
        let grid_rect = Rect::from_min_max(
            Pos2::new(full_rect.min.x + 20.0, header_rect.max.y + 12.0),
            Pos2::new(full_rect.max.x - 20.0, full_rect.max.y - 16.0),
        );

        // Header bar background & divider
        let header_fill = if palette.is_light {
            Color32::from_rgba_unmultiplied(255, 255, 255, 48)
        } else {
            Color32::from_rgba_unmultiplied(12, 14, 22, 72)
        };
        ui.painter().rect_filled(
            header_rect,
            CornerRadius {
                nw: 9,
                ne: 9,
                sw: 0,
                se: 0,
            },
            header_fill,
        );
        ui.painter().line_segment(
            [
                Pos2::new(header_rect.min.x + 14.0, header_rect.max.y),
                Pos2::new(header_rect.max.x - 14.0, header_rect.max.y),
            ],
            Stroke::new(1.0_f32, palette.divider),
        );

        let controls_w = 430.0_f32.min(header_rect.width() * 0.65);
        let drag_rect = Rect::from_min_max(
            header_rect.min,
            Pos2::new((header_rect.max.x - controls_w).max(header_rect.min.x + 120.0), header_rect.max.y),
        );
        let start_drag = ctx.input(|i| {
            i.pointer.button_pressed(PointerButton::Primary)
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| drag_rect.contains(p))
        });
        if start_drag {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }

        let mut create_new = false;
        let mut open_settings = false;
        let mut exit_grid = false;

        let mut header_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("rn_grid_header")
                .max_rect(header_rect.shrink2(Vec2::new(16.0, 0.0))),
        );
        header_ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.label(
                RichText::new("At a Glance")
                    .size(14.5)
                    .strong()
                    .color(palette.primary_text),
            );
            Frame::NONE
                .fill(palette.control_fill)
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::symmetric(8, 2))
                .stroke(Stroke::new(1.0_f32, palette.card_stroke))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("{} notes", self.notes.notes.len()))
                            .size(11.0)
                            .color(palette.secondary_text),
                    );
                });

            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if glass_button(ui, palette, "Back to Note").clicked() {
                    exit_grid = true;
                }
                if glass_icon_button(
                    ui,
                    "grid_settings_btn",
                    palette,
                    TOP_BAR_BUTTON_SIZE,
                    self.settings_window.as_ref().is_some_and(|w| w.is_open()),
                    1.0,
                    &format!("Settings ({})", self.config.keybinds.open_settings),
                    draw_gear_icon,
                )
                .clicked()
                {
                    open_settings = true;
                }
                if glass_primary_button(ui, palette, "+ New Note").clicked() {
                    create_new = true;
                }
                let search_w = (ui.available_width() - 12.0).clamp(120.0, 220.0);
                let search_resp = ui.add(
                    TextEdit::singleline(&mut self.search_query)
                        .id(Id::new(SEARCH_WIDGET_ID))
                        .hint_text("Filter notes...")
                        .desired_width(search_w),
                );
                if self.search_focus_pending {
                    search_resp.request_focus();
                    self.search_focus_pending = false;
                }
            });
        });

        if exit_grid {
            self.exit_grid_mode(ctx, true);
            return;
        }
        if open_settings {
            self.open_settings_window(ctx);
        }
        if create_new {
            self.create_and_focus_new_note(ctx);
            return;
        }

        let query = self.search_query.trim().to_ascii_lowercase();
        let filtered_indices: Vec<usize> = self
            .notes
            .notes
            .iter()
            .enumerate()
            .filter(|(_, n)| query.is_empty() || n.content.to_ascii_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect();

        let gap = 14.0;
        let avail_w = (grid_rect.width() - 6.0).max(240.0);
        let cols = ((avail_w + gap) / 270.0).floor().clamp(2.0, 4.0) as usize;
        let card_w = ((avail_w - gap * ((cols - 1) as f32)) / (cols as f32)).max(190.0);
        let card_h = 196.0;
        let card_size = Vec2::new(card_w, card_h);

        // Arrow + Enter navigation when search box doesn't own focus
        let search_focused = ctx.memory(|m| m.has_focus(Id::new(SEARCH_WIDGET_ID)));
        if !search_focused && !filtered_indices.is_empty() {
            let (left, right, up, down, enter) = ctx.input(|i| {
                let no_mod = !i.modifiers.ctrl && !i.modifiers.alt && !i.modifiers.shift;
                (
                    no_mod && i.key_pressed(Key::ArrowLeft),
                    no_mod && i.key_pressed(Key::ArrowRight),
                    no_mod && i.key_pressed(Key::ArrowUp),
                    no_mod && i.key_pressed(Key::ArrowDown),
                    no_mod && i.key_pressed(Key::Enter),
                )
            });

            if let Some(pos) = filtered_indices
                .iter()
                .position(|&i| i == self.grid_selected_idx)
                .or(Some(0))
            {
                let next_pos = if left {
                    pos.saturating_sub(1)
                } else if right {
                    (pos + 1).min(filtered_indices.len() - 1)
                } else if up {
                    pos.saturating_sub(cols)
                } else if down {
                    (pos + cols).min(filtered_indices.len() - 1)
                } else {
                    pos
                };
                self.grid_selected_idx = filtered_indices[next_pos];

                if enter {
                    let chosen = self.grid_selected_idx;
                    self.switch_to_note(ctx, chosen, 0.0);
                    self.exit_grid_mode(ctx, true);
                    return;
                }
            }
        }

        let mut pick_note: Option<usize> = None;
        let mut delete_idx: Option<usize> = None;
        let mut copy_idx: Option<usize> = None;
        let mut create_from_card = false;
        let highlight = self.config.markdown_highlighting;

        let mut body_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("rn_grid_overlay_body")
                .max_rect(grid_rect),
        );
        body_ui.set_clip_rect(grid_rect);

        let scroll_h = body_ui.available_height().max(120.0);
        body_ui.allocate_ui(Vec2::new(body_ui.available_width(), scroll_h), |ui| {
            ui.set_min_size(Vec2::new(ui.available_width(), scroll_h));
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let total_cards = filtered_indices.len() + 1;
                    let rows = total_cards.div_ceil(cols);

                    for row in 0..rows {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            for col in 0..cols {
                                let slot = row * cols + col;
                                if slot < filtered_indices.len() {
                                    let note_idx = filtered_indices[slot];
                                    let note = &self.notes.notes[note_idx];
                                    let preview_lines = note.preview_lines(9);
                                    let is_empty = note.content.trim().is_empty();
                                    let (_, words, chars) = word_and_char_count(&note.content);
                                    let rel_time = format_relative_time(note.modified_epoch);
                                    let selected = note_idx == self.grid_selected_idx;
                                    let is_active = note_idx == self.notes.active_idx;

                                    let mut card_action_clicked = false;
                                    let card_resp = glass_grid_card(
                                        ui,
                                        ("grid_overlay_card", note_idx),
                                        palette,
                                        card_size,
                                        selected,
                                        |ui, inner, hovered| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    RichText::new(format!(
                                                        "#{} · {}",
                                                        note_idx + 1,
                                                        rel_time
                                                    ))
                                                    .size(11.0)
                                                    .color(if is_active {
                                                        palette.accent_text
                                                    } else {
                                                        palette.muted_text
                                                    }),
                                                );
                                                if hovered {
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(Align::Center),
                                                        |ui| {
                                                            ui.spacing_mut().item_spacing.x = 3.0;
                                                            if glass_icon_button(
                                                                ui,
                                                                ("g_del", note_idx),
                                                                palette,
                                                                26.0,
                                                                false,
                                                                0.95,
                                                                "Delete Note",
                                                                draw_trash_icon,
                                                            )
                                                            .clicked()
                                                            {
                                                                delete_idx = Some(note_idx);
                                                                card_action_clicked = true;
                                                            }
                                                            if glass_icon_button(
                                                                ui,
                                                                ("g_copy", note_idx),
                                                                palette,
                                                                26.0,
                                                                false,
                                                                0.95,
                                                                "Copy Note",
                                                                draw_copy_icon,
                                                            )
                                                            .clicked()
                                                            {
                                                                copy_idx = Some(note_idx);
                                                                card_action_clicked = true;
                                                            }
                                                        },
                                                    );
                                                }
                                            });

                                            ui.add_space(4.0);
                                            if is_empty {
                                                ui.label(
                                                    RichText::new("Empty note")
                                                        .size(12.0)
                                                        .color(palette.muted_text),
                                                );
                                            } else {
                                                let preview_text = preview_lines.join("\n");
                                                let job = build_markdown_layout_job(
                                                    &preview_text,
                                                    (inner.width() - 4.0).max(80.0),
                                                    12.0,
                                                    palette,
                                                    highlight,
                                                );
                                                ui.label(job);
                                            }

                                            ui.painter().text(
                                                Pos2::new(inner.min.x, inner.max.y),
                                                Align2::LEFT_BOTTOM,
                                                format!("{words}w · {chars}c"),
                                                FontId::new(10.5, FontFamily::Proportional),
                                                palette.muted_text,
                                            );
                                            if is_active {
                                                ui.painter().text(
                                                    Pos2::new(inner.max.x, inner.max.y),
                                                    Align2::RIGHT_BOTTOM,
                                                    "Current",
                                                    FontId::new(10.5, FontFamily::Proportional),
                                                    palette.accent_text,
                                                );
                                            }
                                        },
                                    );

                                    if card_resp.clicked() && !card_action_clicked {
                                        pick_note = Some(note_idx);
                                    }
                                } else if slot == filtered_indices.len() {
                                    let new_resp = glass_grid_card(
                                        ui,
                                        "grid_overlay_new_card",
                                        palette,
                                        card_size,
                                        false,
                                        |ui, inner, _| {
                                            draw_plus_icon(
                                                ui.painter(),
                                                inner.center() - Vec2::new(0.0, 12.0),
                                                palette.accent_text,
                                            );
                                            ui.painter().text(
                                                inner.center() + Vec2::new(0.0, 14.0),
                                                Align2::CENTER_CENTER,
                                                "New Note",
                                                FontId::new(13.5, FontFamily::Proportional),
                                                palette.secondary_text,
                                            );
                                        },
                                    );
                                    if new_resp.clicked() {
                                        create_from_card = true;
                                    }
                                }
                            }
                        });
                        ui.add_space(gap);
                    }
                });
        });

        if let Some(idx) = copy_idx {
            if let Some(n) = self.notes.notes.get(idx) {
                ctx.copy_text(n.content.clone());
                self.show_toast("Copied note to clipboard");
            }
        }
        if let Some(idx) = delete_idx {
            self.delete_note_at(ctx, idx);
        } else if create_from_card {
            self.create_and_focus_new_note(ctx);
        } else if let Some(idx) = pick_note {
            self.switch_to_note(ctx, idx, 0.0);
            self.exit_grid_mode(ctx, true);
        }
    }

    fn handle_resize_grip(
        &mut self,
        ctx: &Context,
        ui: &mut egui::Ui,
        full_rect: Rect,
        palette: &GlassPalette,
    ) {
        let grip_rect = Rect::from_min_max(
            Pos2::new(full_rect.max.x - 16.0, full_rect.max.y - 16.0),
            full_rect.max,
        );
        let resp = ui
            .interact(grip_rect, Id::new("rn_resize_grip"), Sense::drag())
            .on_hover_cursor(CursorIcon::ResizeNwSe);

        if resp.hovered() || self.resizing_drag {
            let p = ui.painter();
            let c = palette.muted_text.gamma_multiply(0.7);
            let s = Stroke::new(1.1_f32, c);
            p.line_segment(
                [
                    Pos2::new(full_rect.max.x - 10.0, full_rect.max.y - 4.0),
                    Pos2::new(full_rect.max.x - 4.0, full_rect.max.y - 10.0),
                ],
                s,
            );
            p.line_segment(
                [
                    Pos2::new(full_rect.max.x - 6.0, full_rect.max.y - 4.0),
                    Pos2::new(full_rect.max.x - 4.0, full_rect.max.y - 6.0),
                ],
                s,
            );
        }

        if resp.drag_started() {
            if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                self.resizing_drag = true;
                self.resize_start_cursor = pos;
                self.resize_start_size = self.editor_logical_size();
            }
        }

        if self.resizing_drag {
            if ctx.input(|i| i.pointer.primary_down()) {
                if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    let delta = pos - self.resize_start_cursor;
                    self.config.window_size[0] =
                        (self.resize_start_size.x + delta.x).clamp(360.0, 1200.0).round();
                    self.config.window_size[1] =
                        (self.resize_start_size.y + delta.y).clamp(280.0, 900.0).round();
                    self.ensure_window_size_synced(ctx);
                }
            } else {
                self.resizing_drag = false;
                let _ = self.config.save();
            }
        }
    }

    fn show_settings_viewport(&mut self, root_ctx: &Context) {
        let Some(mut win) = self.settings_window.take() else {
            return;
        };

        let glass_style = self.current_glass_style();
        let allow_esc_close = self.settings_buffers.recording_target.is_none();
        let active_tab = std::cell::Cell::new(self.active_settings_tab);
        let mut collected_actions: Vec<SettingsAction> = Vec::new();
        let mut captured_shortcut: Option<(RecordingTarget, String)> = None;

        let resp = win.show_framed(
            root_ctx,
            &glass_style,
            allow_esc_close,
            Some(&draw_mini_app_badge),
            |_opt_ctx, ui, palette| {
                Frame::NONE
                    .inner_margin(Margin::symmetric(16, 0))
                    .show(ui, |ui| {
                        let mut tab = active_tab.get();
                        if draw_glass_tab_bar_with_icons(
                            ui,
                            "rn_settings_tabs",
                            palette,
                            &SettingsTab::TABS,
                            &mut tab,
                            draw_settings_tab_icon,
                        ) {
                            active_tab.set(tab);
                        }
                    });
                ui.add_space(10.0);
            },
            |opt_ctx, ui, palette| {
                if let Some(target) = self.settings_buffers.recording_target {
                    let distinguish_sides = matches!(target, RecordingTarget::GlobalToggle);
                    if let Some(raw) = capture_shortcut(opt_ctx, distinguish_sides) {
                        captured_shortcut = Some((target, raw));
                    }
                }

                let actions = match active_tab.get() {
                    SettingsTab::Editor => {
                        render_editor_tab(ui, palette, &mut self.config, &mut self.settings_buffers)
                    }
                    SettingsTab::Appearance => {
                        render_appearance_tab(ui, palette, &mut self.config)
                    }
                    SettingsTab::Keybinds => {
                        render_keybinds_tab(ui, palette, &mut self.config, &mut self.settings_buffers)
                    }
                    SettingsTab::System => render_system_tab(ui, palette, &mut self.config),
                };
                collected_actions.extend(actions);
            },
        );

        self.active_settings_tab = active_tab.get();
        self.settings_window = Some(win);

        if let Some((target, raw)) = captured_shortcut {
            self.settings_buffers.recording_target = None;
            self.hotkey_controller.set_paused(false);
            if raw != "ESCAPE_CANCEL" {
                match target {
                    RecordingTarget::GlobalToggle => {
                        self.settings_buffers.global_hotkey_input = raw.clone();
                        collected_actions.push(SettingsAction::ApplyGlobalHotkey(raw));
                    }
                    RecordingTarget::InApp(slot) => {
                        self.settings_buffers
                            .in_app_keybind_inputs
                            .insert(slot, raw.clone());
                        collected_actions.push(SettingsAction::ApplyInAppKeybind(slot, raw));
                    }
                }
            }
        }

        let is_recording_global =
            self.settings_buffers.recording_target == Some(RecordingTarget::GlobalToggle);
        self.hotkey_controller.set_paused(is_recording_global);

        for action in collected_actions {
            self.handle_settings_action(root_ctx, action);
        }

        if let Some(pos_px) = resp.persisted_pos_px {
            let pos_f = [pos_px[0] as f32, pos_px[1] as f32];
            if self.config.settings_window_pos != Some(pos_f) {
                self.config.settings_window_pos = Some(pos_f);
                let _ = self.config.save();
            }
        }

        if resp.closed {
            self.settings_buffers.recording_target = None;
            self.hotkey_controller.set_paused(false);
            self.shown_at = Instant::now();
            self.had_focus_since_show = false;
            if !self.window_hidden {
                show_window_to_front(self.hwnd, self.config.always_on_top);
            }
            let _ = self.config.save();
        }
    }

    fn handle_settings_action(&mut self, root_ctx: &Context, action: SettingsAction) {
        match action {
            SettingsAction::FontChanged(font_name) => {
                let resolved = install_glass_fonts(root_ctx, &font_name);
                self.config.editor_font = resolved;
                let _ = self.config.save();
                root_ctx.request_repaint();
            }
            SettingsAction::FontSizeChanged => {
                self.config.clamp_values();
                let _ = self.config.save();
                root_ctx.request_repaint();
            }
            SettingsAction::ThemeOrVisualChanged => {
                self.ensure_dwm_synced();
                let _ = self.config.save();
                root_ctx.request_repaint();
            }
            SettingsAction::WindowSizeChanged => {
                self.config.clamp_values();
                if self.view_mode == ViewMode::Editor {
                    self.ensure_window_size_synced(root_ctx);
                }
                let _ = self.config.save();
                root_ctx.request_repaint();
            }
            SettingsAction::CenterWindow => {
                self.center_window_on_screen(root_ctx);
                root_ctx.request_repaint();
            }
            SettingsAction::ApplyNotesDir(path_str) => {
                let trimmed = path_str.trim();
                self.config.notes_dir = (!trimmed.is_empty()).then(|| trimmed.to_string());
                let new_dir = self.config.effective_notes_dir();
                self.notes.reload_dir(new_dir, &self.config.note_order);
                self.settings_buffers.notes_dir_input =
                    self.notes.notes_dir.to_string_lossy().to_string();
                self.sync_config_note_metadata();
                let _ = self.config.save();
                root_ctx.request_repaint();
            }
            SettingsAction::OpenNotesFolder => {
                self.open_notes_folder_in_explorer();
            }
            SettingsAction::ApplyGlobalHotkey(raw) => {
                match self.hotkey_controller.set_shortcut(&raw) {
                    Ok(()) => {
                        let norm = self.hotkey_controller.active_shortcut_str.clone();
                        self.config.keybinds.global_toggle_app = norm.clone();
                        self.settings_buffers.global_hotkey_input = norm.clone();
                        self.settings_buffers.global_hotkey_feedback =
                            Some((format!("Registered global shortcut: {norm}"), true));
                        let _ = self.config.save();
                    }
                    Err(err) => {
                        self.settings_buffers.global_hotkey_feedback = Some((err, false));
                    }
                }
            }
            SettingsAction::ApplyInAppKeybind(slot, raw) => match normalize_shortcut(&raw) {
                Ok(norm) => {
                    self.config.keybinds.set(slot, norm.clone());
                    self.settings_buffers
                        .in_app_keybind_inputs
                        .insert(slot, norm.clone());
                    self.settings_buffers.in_app_keybind_feedback =
                        Some((format!("Updated '{}': {norm}", slot.label()), true));
                    let _ = self.config.save();
                }
                Err(err) => {
                    self.settings_buffers.in_app_keybind_feedback = Some((err, false));
                }
            },
            SettingsAction::ResetAllKeybinds => {
                self.config.keybinds.reset_all();
                self.settings_buffers
                    .sync_keybind_inputs_from_config(&self.config);
                let global_def = self.config.keybinds.global_toggle_app.clone();
                let _ = self.hotkey_controller.set_shortcut(&global_def);
                self.settings_buffers.global_hotkey_feedback =
                    Some((format!("Reset all shortcuts (Global: {global_def})"), true));
                self.settings_buffers.in_app_keybind_feedback = None;
                let _ = self.config.save();
            }
            SettingsAction::AlwaysOnTopChanged(aot) => {
                self.set_always_on_top(root_ctx, aot);
                root_ctx.request_repaint();
            }
            SettingsAction::ConfigSaved => {
                let _ = self.config.save();
                root_ctx.request_repaint();
            }
        }
    }
}

impl eframe::App for RapidNotesApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn raw_input_hook(&mut self, _ctx: &Context, raw_input: &mut egui::RawInput) {
        if raw_input.viewport_id != ViewportId::ROOT {
            return;
        }
        if self.window_hidden
            || self.view_mode != ViewMode::Editor
            || self.settings_buffers.recording_target.is_some()
        {
            self.scroll_note_switcher.reset();
            self.pending_scroll_switch_dir = 0;
            return;
        }

        let dir = self.scroll_note_switcher.consume_raw_input(
            raw_input,
            self.config.keybinds.scroll_switch_modifier,
            Instant::now(),
        );
        if dir != 0 {
            self.pending_scroll_switch_dir = dir;
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.view_mode == ViewMode::Editor {
            if let Some(pos_px) = get_window_pos_px(self.hwnd).or(self.last_observed_pos_px) {
                self.config.window_pos = Some([pos_px[0] as f32, pos_px[1] as f32]);
            }
        } else if let Some(pos_px) = self.saved_editor_pos_px {
            self.config.window_pos = Some([pos_px[0] as f32, pos_px[1] as f32]);
        }
        self.persist_settings_window_pos();
        if let Some(win) = &self.settings_window {
            win.hide();
        }
        self.flush_dirty_notes_and_config();
    }

    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        if self.handle_os_events(ctx) {
            return;
        }

        if self.uncloak_frames_remaining > 0 {
            self.uncloak_frames_remaining -= 1;
            if self.uncloak_frames_remaining == 0 {
                if let Some((pos, size_px)) = self.pending_cloak_rect_px.take() {
                    if !self.window_hidden {
                        position_window_px_no_activate(self.hwnd, pos, size_px);
                    }
                }
                if !self.window_hidden {
                    self.backdrop.invalidate();
                    self.ensure_dwm_synced();
                    show_window_to_front(self.hwnd, self.config.always_on_top);
                }
            }
        } else {
            self.ensure_window_size_synced(ctx);
        }

        self.ensure_dwm_synced();
        let glass_style = self.current_glass_style();
        apply_glass_visuals(ctx, &glass_style);
        let palette = glass_style.palette();

        self.track_persistence_and_focus(ctx);

        let scroll_switch_dir = std::mem::take(&mut self.pending_scroll_switch_dir);
        if !self.window_hidden && self.settings_buffers.recording_target.is_none() {
            if self.view_mode == ViewMode::Editor && scroll_switch_dir != 0 {
                ctx.input_mut(|i| {
                    i.raw_scroll_delta = Vec2::ZERO;
                    i.smooth_scroll_delta = Vec2::ZERO;
                });
                self.step_note_horizontal(ctx, scroll_switch_dir as isize);
            }

            if let Some(action) = poll_in_app_action(ctx, &self.config.keybinds) {
                self.execute_in_app_action(ctx, action);
            }
        }

        let win_size = self.desired_window_size();
        let full_rect = Rect::from_min_size(Pos2::ZERO, win_size);

        let mut top_alpha = 0.0_f32;
        let mut bottom_alpha = 0.0_f32;
        let mut nav_flashing = false;

        if self.view_mode == ViewMode::Editor {
            let pointer_pos = ctx.input(|i| i.pointer.hover_pos().or(i.pointer.interact_pos()));
            let top_zone = Rect::from_min_max(
                full_rect.min,
                Pos2::new(full_rect.max.x, full_rect.min.y + TOP_BAR_HEIGHT + 4.0),
            );
            let bottom_zone = Rect::from_min_max(
                Pos2::new(full_rect.min.x, full_rect.max.y - BOTTOM_BAR_HEIGHT - 4.0),
                full_rect.max,
            );

            let top_hovered = pointer_pos.is_some_and(|p| top_zone.contains(p));
            let bottom_hovered = pointer_pos.is_some_and(|p| bottom_zone.contains(p));
            nav_flashing = self
                .nav_flash_until
                .is_some_and(|until| Instant::now() < until);
            if !nav_flashing {
                self.nav_flash_until = None;
            }

            top_alpha = hover_chrome_opacity(
                ctx,
                Id::new("rn_top_hover_anim"),
                self.config.always_show_chrome || top_hovered,
            );
            bottom_alpha = hover_chrome_opacity(
                ctx,
                Id::new("rn_bottom_hover_anim"),
                self.config.always_show_chrome || bottom_hovered || nav_flashing,
            );
        }

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::TRANSPARENT))
            .show(ctx, |ui| {
                draw_glass_surface(ui.painter(), full_rect, &glass_style);

                match self.view_mode {
                    ViewMode::Editor => {
                        let top_bar_rect = Rect::from_min_max(
                            full_rect.min,
                            Pos2::new(full_rect.max.x, full_rect.min.y + TOP_BAR_HEIGHT),
                        );
                        let bottom_bar_rect = Rect::from_min_max(
                            Pos2::new(full_rect.min.x, full_rect.max.y - BOTTOM_BAR_HEIGHT),
                            full_rect.max,
                        );
                        let content_rect = Rect::from_min_max(
                            Pos2::new(full_rect.min.x + 16.0, top_bar_rect.max.y + 4.0),
                            Pos2::new(full_rect.max.x - 16.0, bottom_bar_rect.min.y - 4.0),
                        );

                        self.render_editor_body(ctx, ui, &palette, content_rect);
                        self.render_top_hover_bar(ctx, ui, &palette, top_bar_rect, top_alpha);
                        self.render_bottom_hover_bar(ctx, ui, &palette, bottom_bar_rect, bottom_alpha);
                        self.handle_resize_grip(ctx, ui, full_rect, &palette);
                    }
                    ViewMode::Grid => {
                        self.render_grid_overlay(ctx, ui, &palette, full_rect);
                    }
                }

                if let Some((ref msg, created_at)) = self.toast {
                    let age = created_at.elapsed().as_secs_f32();
                    if age < 1.8 {
                        let fade = if age > 1.3 {
                            1.0 - ((age - 1.3) / 0.5).clamp(0.0, 1.0)
                        } else {
                            1.0
                        };
                        let toast_y = if self.view_mode == ViewMode::Grid {
                            full_rect.max.y - 28.0
                        } else {
                            full_rect.max.y - BOTTOM_BAR_HEIGHT - 16.0
                        };
                        glass_toast_pill(
                            ui.painter(),
                            Pos2::new(full_rect.center().x, toast_y),
                            &palette,
                            msg,
                            fade,
                        );
                    } else {
                        self.toast = None;
                    }
                }
            });

        if self.settings_window.as_ref().is_some_and(|w| w.is_open()) {
            self.show_settings_viewport(ctx);
        }

        if self.startup_frames < 5 {
            if self.startup_frames == 1 {
                self.backdrop.invalidate();
                self.ensure_dwm_synced();
            }
            self.startup_frames += 1;
            if self.startup_frames == 4 {
                trim_process_working_set();
            }
            ctx.request_repaint();
        }

        let animating = self.uncloak_frames_remaining > 0
            || self.focus_retry_remaining > 0
            || self.slide_started_at.is_some()
            || self.toast.is_some()
            || (top_alpha > 0.01 && top_alpha < 0.99)
            || (bottom_alpha > 0.01 && bottom_alpha < 0.99)
            || self
                .settings_window
                .as_ref()
                .is_some_and(|w| w.is_activating());

        if animating {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else if !self.window_hidden && self.config.hide_on_focus_loss {
            ctx.request_repaint_after(Duration::from_millis(160));
        } else if nav_flashing
            || self.notes_dirty_since.is_some()
            || self.pos_dirty_since.is_some()
            || self
                .settings_window
                .as_ref()
                .is_some_and(|w| w.is_open())
        {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Duration, RapidNotesApp};

    #[test]
    fn test_should_hide_never_closes_when_initial_focus_not_yet_granted_over_summon_window() {
        assert!(!RapidNotesApp::should_hide_on_focus_loss(
            true,
            false,
            false,
            Duration::from_millis(1500),
            5,
            0,
            false,
            false,
            Some(100),
            Some(200),
            Some(200),
        ));
        assert!(!RapidNotesApp::should_hide_on_focus_loss(
            true,
            false,
            false,
            Duration::from_millis(1500),
            5,
            0,
            false,
            false,
            Some(100),
            None,
            Some(300),
        ));
    }

    #[test]
    fn test_should_hide_respects_settle_window_then_closes_after_focus_loss() {
        // Inside initial 120ms settle window
        assert!(!RapidNotesApp::should_hide_on_focus_loss(
            true,
            false,
            true,
            Duration::from_millis(80),
            5,
            0,
            false,
            false,
            Some(100),
            Some(200),
            Some(300),
        ));
        // Once had focus and >= 120ms elapsed, losing focus closes
        assert!(RapidNotesApp::should_hide_on_focus_loss(
            true,
            false,
            true,
            Duration::from_millis(120),
            5,
            0,
            false,
            false,
            Some(100),
            None,
            Some(300),
        ));
    }

    #[test]
    fn test_should_hide_closes_if_third_window_activated_even_without_initial_focus() {
        assert!(!RapidNotesApp::should_hide_on_focus_loss(
            true,
            false,
            false,
            Duration::from_millis(799),
            5,
            0,
            false,
            false,
            Some(100),
            Some(200),
            Some(300),
        ));
        assert!(RapidNotesApp::should_hide_on_focus_loss(
            true,
            false,
            false,
            Duration::from_millis(800),
            5,
            0,
            false,
            false,
            Some(100),
            Some(200),
            Some(300),
        ));
    }

    #[test]
    fn test_should_hide_guards_disabled_settings_resize_uncloak_and_startup() {
        // Disabled config
        assert!(!RapidNotesApp::should_hide_on_focus_loss(
            false,
            false,
            true,
            Duration::from_millis(500),
            5,
            0,
            false,
            false,
            Some(100),
            None,
            Some(300),
        ));
        // Currently focused
        assert!(!RapidNotesApp::should_hide_on_focus_loss(
            true,
            true,
            true,
            Duration::from_millis(500),
            5,
            0,
            false,
            false,
            Some(100),
            None,
            Some(100),
        ));
        // Settings open
        assert!(!RapidNotesApp::should_hide_on_focus_loss(
            true,
            false,
            true,
            Duration::from_millis(500),
            5,
            0,
            false,
            true,
            Some(100),
            None,
            Some(300),
        ));
        // Resizing drag active
        assert!(!RapidNotesApp::should_hide_on_focus_loss(
            true,
            false,
            true,
            Duration::from_millis(500),
            5,
            0,
            true,
            false,
            Some(100),
            None,
            Some(300),
        ));
        // Uncloak frames remaining
        assert!(!RapidNotesApp::should_hide_on_focus_loss(
            true,
            false,
            true,
            Duration::from_millis(500),
            5,
            1,
            false,
            false,
            Some(100),
            None,
            Some(300),
        ));
        // Startup frames < 5
        assert!(!RapidNotesApp::should_hide_on_focus_loss(
            true,
            false,
            true,
            Duration::from_millis(500),
            4,
            0,
            false,
            false,
            Some(100),
            None,
            Some(300),
        ));
    }
}
