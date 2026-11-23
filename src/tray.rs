use crate::keybinds::SharedWakeState;
use std::sync::{
    mpsc::{self, Receiver, Sender},
    Arc, Mutex, OnceLock,
};
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    ToggleWindow,
    NewNote,
    OpenGrid,
    ToggleAlwaysOnTop,
    OpenNotesFolder,
    OpenSettings,
    Quit,
}

#[derive(Clone)]
struct TrayMenuIds {
    toggle: MenuId,
    new_note: MenuId,
    open_grid: MenuId,
    toggle_aot: MenuId,
    open_folder: MenuId,
    settings: MenuId,
    quit: MenuId,
}

struct TrayForwarder {
    tx: Sender<TrayAction>,
    wake: Arc<SharedWakeState>,
    ids: TrayMenuIds,
}

static TRAY_FORWARDER: OnceLock<Mutex<Option<TrayForwarder>>> = OnceLock::new();

pub struct TrayController {
    _tray_icon: Option<TrayIcon>,
    aot_check_item: Option<CheckMenuItem>,
    rx: Receiver<TrayAction>,
}

impl TrayController {
    pub fn new(always_on_top: bool, wake_state: Arc<SharedWakeState>) -> Self {
        let (tx, rx) = mpsc::channel();
        let ids = TrayMenuIds {
            toggle: MenuId::new("rn_tray_toggle"),
            new_note: MenuId::new("rn_tray_new_note"),
            open_grid: MenuId::new("rn_tray_grid"),
            toggle_aot: MenuId::new("rn_tray_aot"),
            open_folder: MenuId::new("rn_tray_open_folder"),
            settings: MenuId::new("rn_tray_settings"),
            quit: MenuId::new("rn_tray_quit"),
        };

        let slot = TRAY_FORWARDER.get_or_init(|| {
            TrayIconEvent::set_event_handler(Some(|event: TrayIconEvent| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    if let Some(lock) = TRAY_FORWARDER.get() {
                        if let Ok(guard) = lock.lock() {
                            if let Some(ref fwd) = *guard {
                                let _ = fwd.tx.send(TrayAction::ToggleWindow);
                                fwd.wake.wake();
                            }
                        }
                    }
                }
            }));

            MenuEvent::set_event_handler(Some(|event: MenuEvent| {
                if let Some(lock) = TRAY_FORWARDER.get() {
                    if let Ok(guard) = lock.lock() {
                        if let Some(ref fwd) = *guard {
                            let action = if event.id == fwd.ids.toggle {
                                Some(TrayAction::ToggleWindow)
                            } else if event.id == fwd.ids.new_note {
                                Some(TrayAction::NewNote)
                            } else if event.id == fwd.ids.open_grid {
                                Some(TrayAction::OpenGrid)
                            } else if event.id == fwd.ids.toggle_aot {
                                Some(TrayAction::ToggleAlwaysOnTop)
                            } else if event.id == fwd.ids.open_folder {
                                Some(TrayAction::OpenNotesFolder)
                            } else if event.id == fwd.ids.settings {
                                Some(TrayAction::OpenSettings)
                            } else if event.id == fwd.ids.quit {
                                fwd.wake.mark_quit_requested();
                                Some(TrayAction::Quit)
                            } else {
                                None
                            };

                            if let Some(act) = action {
                                let _ = fwd.tx.send(act);
                                fwd.wake.wake();
                            }
                        }
                    }
                }
            }));

            Mutex::new(None)
        });

        if let Ok(mut guard) = slot.lock() {
            *guard = Some(TrayForwarder {
                tx,
                wake: wake_state,
                ids: ids.clone(),
            });
        }

        let (tray_icon, aot_check_item) = match Self::build_tray(&ids, always_on_top) {
            Ok((t, item)) => (Some(t), Some(item)),
            Err(_) => (None, None),
        };

        Self {
            _tray_icon: tray_icon,
            aot_check_item,
            rx,
        }
    }

    pub fn sync_always_on_top(&self, always_on_top: bool) {
        if let Some(item) = &self.aot_check_item {
            item.set_checked(always_on_top);
        }
    }

    pub fn poll_actions(&self) -> Vec<TrayAction> {
        let mut actions = Vec::new();
        while let Ok(action) = self.rx.try_recv() {
            actions.push(action);
        }
        actions
    }

    fn build_tray(ids: &TrayMenuIds, always_on_top: bool) -> Result<(TrayIcon, CheckMenuItem), String> {
        let menu = Menu::new();
        let item_toggle = MenuItem::with_id(
            ids.toggle.clone(),
            "Show / Hide Rapid Notes",
            true,
            None,
        );
        let item_new = MenuItem::with_id(ids.new_note.clone(), "New Note", true, None);
        let item_grid = MenuItem::with_id(ids.open_grid.clone(), "At-a-Glance Grid", true, None);
        let item_aot = CheckMenuItem::with_id(
            ids.toggle_aot.clone(),
            "Always on Top",
            true,
            always_on_top,
            None,
        );
        let item_folder =
            MenuItem::with_id(ids.open_folder.clone(), "Open Notes Folder", true, None);
        let item_settings = MenuItem::with_id(ids.settings.clone(), "Settings...", true, None);
        let item_quit = MenuItem::with_id(ids.quit.clone(), "Quit", true, None);

        let _ = menu.append(&item_toggle);
        let _ = menu.append(&item_new);
        let _ = menu.append(&item_grid);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&item_aot);
        let _ = menu.append(&item_folder);
        let _ = menu.append(&item_settings);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&item_quit);

        let icon = generate_tray_icon()?;
        let tray = TrayIconBuilder::new()
            .with_tooltip("Rapid Notes")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| format!("Failed to build system tray icon: {e}"))?;

        Ok((tray, item_aot))
    }
}

pub fn generate_tray_icon() -> Result<Icon, String> {
    let (rgba, w, h) = generate_icon_rgba();
    Icon::from_rgba(rgba, w, h).map_err(|e| e.to_string())
}

/// Generates a crisp 32x32 RGBA rounded translucent note-pad icon in memory.
pub fn generate_icon_rgba() -> (Vec<u8>, u32, u32) {
    let w = 32u32;
    let h = 32u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];

    for y in 0..h {
        for x in 0..w {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;

            let half = 13.5;
            let radius = 6.5;
            let dx = (fx - 16.0).abs() - (half - radius);
            let dy = (fy - 16.0).abs() - (half - radius);
            let dist = dx.max(0.0).hypot(dy.max(0.0)) + dx.min(0.0).max(dy.min(0.0)) - radius;

            if dist <= 0.8 {
                let edge_alpha = (0.8 - dist).clamp(0.0, 1.0);
                let t = (fy / 32.0).clamp(0.0, 1.0);
                let mut r = 44.0 * (1.0 - t) + 24.0 * t;
                let mut g = 122.0 * (1.0 - t) + 74.0 * t;
                let mut b = 236.0 * (1.0 - t) + 186.0 * t;

                // Subtle white highlight lines representing note rows
                let in_line_x = (8.5..=23.5).contains(&fx);
                let in_line_2 = (8.5..=20.0).contains(&fx);
                let in_line_3 = (8.5..=17.5).contains(&fx);
                let is_stroke = (in_line_x && (fy - 10.5).abs() < 1.15)
                    || (in_line_2 && (fy - 16.0).abs() < 1.15)
                    || (in_line_3 && (fy - 21.5).abs() < 1.15);
                if is_stroke {
                    r = 246.0;
                    g = 250.0;
                    b = 255.0;
                }

                let idx = ((y * w + x) * 4) as usize;
                rgba[idx] = r.clamp(0.0, 255.0) as u8;
                rgba[idx + 1] = g.clamp(0.0, 255.0) as u8;
                rgba[idx + 2] = b.clamp(0.0, 255.0) as u8;
                rgba[idx + 3] = (edge_alpha * 255.0) as u8;
            }
        }
    }

    (rgba, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tray_icon_generation() {
        let (rgba, w, h) = generate_icon_rgba();
        assert_eq!(w, 32);
        assert_eq!(h, 32);
        assert_eq!(rgba.len(), 32 * 32 * 4);
        assert!(generate_tray_icon().is_ok());
    }
}
