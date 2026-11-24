use crate::config::{Keybinds, ScrollModifier};
use egui_glass_ui::{
    is_win_key_pressed, read_modifier_side_state, wake_cloaked_window, ModifierRequirement,
    SideAwareHotkeyHook, SideAwareHotkeySpec,
};
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use std::sync::{
    atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering},
    mpsc::{self, Receiver, Sender},
    Arc, Mutex, OnceLock,
};
use std::time::{Duration, Instant};

/// Shared wakeup state so global-hotkey and tray callbacks can safely wake a hidden or sleeping window.
pub struct SharedWakeState {
    egui_ctx: Mutex<Option<egui::Context>>,
    hwnd: AtomicIsize,
    hidden: AtomicBool,
    always_on_top: AtomicBool,
    quit_requested: AtomicBool,
}

impl SharedWakeState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            egui_ctx: Mutex::new(None),
            hwnd: AtomicIsize::new(0),
            hidden: AtomicBool::new(false),
            always_on_top: AtomicBool::new(false),
            quit_requested: AtomicBool::new(false),
        })
    }

    pub fn set_ctx(&self, ctx: egui::Context) {
        if let Ok(mut guard) = self.egui_ctx.lock() {
            *guard = Some(ctx);
        }
    }

    pub fn set_hwnd(&self, hwnd: Option<isize>) {
        self.hwnd.store(hwnd.unwrap_or(0), Ordering::SeqCst);
    }

    pub fn set_hidden(&self, hidden: bool) {
        self.hidden.store(hidden, Ordering::SeqCst);
    }

    pub fn set_always_on_top(&self, aot: bool) {
        self.always_on_top.store(aot, Ordering::SeqCst);
    }

    pub fn mark_quit_requested(&self) {
        self.quit_requested.store(true, Ordering::SeqCst);
    }

    pub fn is_quit_requested(&self) -> bool {
        self.quit_requested.load(Ordering::SeqCst)
    }

    /// Wakes the UI thread; when the root HWND is hidden, wakes it cloaked so `update()` can
    /// handle actions or position on the configured monitor before revealing without visual flash.
    pub fn wake(&self) {
        let hwnd_raw = self.hwnd.load(Ordering::SeqCst);
        let hwnd = (hwnd_raw != 0).then_some(hwnd_raw);
        let is_hidden = self.hidden.load(Ordering::SeqCst);

        if is_hidden && hwnd.is_some() {
            wake_cloaked_window(hwnd);
        }

        if let Ok(guard) = self.egui_ctx.lock() {
            if let Some(ref ctx) = *guard {
                ctx.request_repaint();
            }
        }
    }
}

static GLOBAL_HOTKEY_FORWARDER: OnceLock<Mutex<Option<(mpsc::Sender<()>, Arc<AtomicU32>, Arc<SharedWakeState>)>>> =
    OnceLock::new();

pub struct GlobalHotkeyController {
    manager: Option<GlobalHotKeyManager>,
    active_hotkey: Option<HotKey>,
    active_id: Arc<AtomicU32>,
    side_hook: Option<SideAwareHotkeyHook>,
    active_parsed: Option<ParsedShortcut>,
    paused: bool,
    tx: Sender<()>,
    rx: Receiver<()>,
    wake_state: Arc<SharedWakeState>,
    pub active_shortcut_str: String,
    pub status_error: Option<String>,
}

impl GlobalHotkeyController {
    pub fn new(initial_shortcut: &str, wake_state: Arc<SharedWakeState>) -> Self {
        let manager = GlobalHotKeyManager::new().ok();
        let (tx, rx) = mpsc::channel();
        let active_id = Arc::new(AtomicU32::new(0));

        let slot = GLOBAL_HOTKEY_FORWARDER.get_or_init(|| {
            GlobalHotKeyEvent::set_event_handler(Some(|event: GlobalHotKeyEvent| {
                if event.state != HotKeyState::Pressed {
                    return;
                }
                if let Some(lock) = GLOBAL_HOTKEY_FORWARDER.get() {
                    if let Ok(guard) = lock.lock() {
                        if let Some((ref tx, ref expected_id, ref wake)) = *guard {
                            let expected = expected_id.load(Ordering::SeqCst);
                            if expected != 0 && event.id == expected {
                                let _ = tx.send(());
                                wake.wake();
                            }
                        }
                    }
                }
            }));
            Mutex::new(None)
        });
        if let Ok(mut guard) = slot.lock() {
            *guard = Some((tx.clone(), Arc::clone(&active_id), Arc::clone(&wake_state)));
        }

        let mut controller = Self {
            manager,
            active_hotkey: None,
            active_id,
            side_hook: None,
            active_parsed: None,
            paused: false,
            tx,
            rx,
            wake_state,
            active_shortcut_str: initial_shortcut.to_string(),
            status_error: None,
        };
        let _ = controller.set_shortcut(initial_shortcut);
        controller
    }

    pub fn set_shortcut(&mut self, shortcut_str: &str) -> Result<(), String> {
        let parsed = parse_shortcut(shortcut_str)?;
        let normalized = parsed.format_label();

        if parsed.is_side_specific() {
            let spec = parsed.to_side_aware_spec()?;

            if let (Some(mgr), Some(old)) = (&self.manager, self.active_hotkey.take()) {
                let _ = mgr.unregister(old);
            }
            self.active_id.store(0, Ordering::SeqCst);

            if let Some(ref hook) = self.side_hook {
                hook.update_spec(spec);
                hook.set_paused(self.paused);
            } else {
                let tx = self.tx.clone();
                let wake = Arc::clone(&self.wake_state);
                let hook = SideAwareHotkeyHook::new(spec, move || {
                    let _ = tx.send(());
                    wake.wake();
                })?;
                if self.paused {
                    hook.set_paused(true);
                }
                self.side_hook = Some(hook);
            }

            self.active_parsed = Some(parsed);
            self.active_shortcut_str = normalized;
            self.status_error = None;
            Ok(())
        } else {
            let new_hk = parsed.to_global_hotkey()?;
            let Some(mgr) = &self.manager else {
                let err = "GlobalHotKeyManager unavailable on this session".to_string();
                self.status_error = Some(err.clone());
                return Err(err);
            };

            self.side_hook = None;
            if let Some(old) = self.active_hotkey.take() {
                let _ = mgr.unregister(old);
            }
            self.active_id.store(0, Ordering::SeqCst);

            if !self.paused {
                mgr.register(new_hk)
                    .map_err(|e| format!("Could not register '{normalized}': {e}"))?;
                self.active_id.store(new_hk.id(), Ordering::SeqCst);
                self.active_hotkey = Some(new_hk);
            }
            self.active_parsed = Some(parsed);
            self.active_shortcut_str = normalized;
            self.status_error = None;
            Ok(())
        }
    }

    /// Temporarily pauses/resumes the active global shortcut (used while recording a new summon hotkey).
    pub fn set_paused(&mut self, paused: bool) {
        if self.paused == paused {
            return;
        }
        self.paused = paused;
        if let Some(ref hook) = self.side_hook {
            hook.set_paused(paused);
            return;
        }
        if let (Some(mgr), Some(parsed)) = (&self.manager, self.active_parsed) {
            if paused {
                if let Some(old) = self.active_hotkey.take() {
                    let _ = mgr.unregister(old);
                }
                self.active_id.store(0, Ordering::SeqCst);
            } else if self.active_hotkey.is_none() {
                if let Ok(hk) = parsed.to_global_hotkey() {
                    if mgr.register(hk).is_ok() {
                        self.active_id.store(hk.id(), Ordering::SeqCst);
                        self.active_hotkey = Some(hk);
                    }
                }
            }
        }
    }

    pub fn poll_triggered(&self) -> bool {
        let mut triggered = false;
        while self.rx.try_recv().is_ok() {
            triggered = true;
        }
        triggered
    }
}

impl Drop for GlobalHotkeyController {
    fn drop(&mut self) {
        self.side_hook = None;
        if let (Some(mgr), Some(hk)) = (&self.manager, self.active_hotkey.take()) {
            let _ = mgr.unregister(hk);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedShortcut {
    pub win: ModifierRequirement,
    pub ctrl: ModifierRequirement,
    pub alt: ModifierRequirement,
    pub shift: ModifierRequirement,
    pub key: egui::Key,
}

fn push_modifier_label(
    parts: &mut Vec<&'static str>,
    req: ModifierRequirement,
    left: &'static str,
    right: &'static str,
    any: &'static str,
) {
    match req {
        ModifierRequirement::None => {}
        ModifierRequirement::Any => parts.push(any),
        ModifierRequirement::Left => parts.push(left),
        ModifierRequirement::Right => parts.push(right),
        ModifierRequirement::Both => {
            parts.push(left);
            parts.push(right);
        }
    }
}

impl ParsedShortcut {
    pub fn is_side_specific(self) -> bool {
        self.win.is_side_specific()
            || self.ctrl.is_side_specific()
            || self.alt.is_side_specific()
            || self.shift.is_side_specific()
    }

    pub fn format_label(self) -> String {
        let mut parts = Vec::new();
        push_modifier_label(&mut parts, self.win, "LWin", "RWin", "Win");
        push_modifier_label(&mut parts, self.ctrl, "LCtrl", "RCtrl", "Ctrl");
        push_modifier_label(&mut parts, self.alt, "LAlt", "RAlt", "Alt");
        push_modifier_label(&mut parts, self.shift, "LShift", "RShift", "Shift");
        parts.push(egui_key_to_label(self.key));
        parts.join("+")
    }

    pub fn matches_key_event(
        self,
        key: egui::Key,
        modifiers: egui::Modifiers,
        win_pressed: bool,
    ) -> bool {
        if self.key != key {
            return false;
        }
        let ctrl_down = modifiers.ctrl || modifiers.command;
        self.ctrl.to_any_bool() == ctrl_down
            && self.alt.to_any_bool() == modifiers.alt
            && self.shift.to_any_bool() == modifiers.shift
            && self.win.to_any_bool() == win_pressed
    }

    pub fn to_side_aware_spec(self) -> Result<SideAwareHotkeySpec, String> {
        let vk_code = egui_key_to_vk_code(self.key)?;
        Ok(SideAwareHotkeySpec {
            vk_code,
            win: self.win,
            ctrl: self.ctrl,
            alt: self.alt,
            shift: self.shift,
        })
    }

    pub fn to_global_hotkey(self) -> Result<HotKey, String> {
        let mut mods = Modifiers::empty();
        if self.win.to_any_bool() {
            mods |= Modifiers::SUPER;
        }
        if self.ctrl.to_any_bool() {
            mods |= Modifiers::CONTROL;
        }
        if self.alt.to_any_bool() {
            mods |= Modifiers::ALT;
        }
        if self.shift.to_any_bool() {
            mods |= Modifiers::SHIFT;
        }
        let code = egui_key_to_global_code(self.key)?;
        let mods_opt = (!mods.is_empty()).then_some(mods);
        Ok(HotKey::new(mods_opt, code))
    }
}

pub fn parse_shortcut(input: &str) -> Result<ParsedShortcut, String> {
    let parts: Vec<&str> = input
        .split('+')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if parts.is_empty() {
        return Err("Keybind cannot be empty".to_string());
    }

    let mut parsed = ParsedShortcut {
        win: ModifierRequirement::None,
        ctrl: ModifierRequirement::None,
        alt: ModifierRequirement::None,
        shift: ModifierRequirement::None,
        key: egui::Key::A,
    };
    let mut main_key: Option<egui::Key> = None;

    for part in parts {
        match part.to_ascii_uppercase().as_str() {
            "WIN" | "SUPER" | "META" | "CMD" => parsed.win.add_any(),
            "LWIN" | "LEFTWIN" | "LSUPER" | "LEFTSUPER" => parsed.win.add_left(),
            "RWIN" | "RIGHTWIN" | "RSUPER" | "RIGHTSUPER" => parsed.win.add_right(),
            "CTRL" | "CONTROL" => parsed.ctrl.add_any(),
            "LCTRL" | "LEFTCTRL" | "LCONTROL" | "LEFTCONTROL" => parsed.ctrl.add_left(),
            "RCTRL" | "RIGHTCTRL" | "RCONTROL" | "RIGHTCONTROL" => parsed.ctrl.add_right(),
            "ALT" | "OPTION" => parsed.alt.add_any(),
            "LALT" | "LEFTALT" | "LOPTION" | "LEFTOPTION" => parsed.alt.add_left(),
            "RALT" | "RIGHTALT" | "ROPTION" | "RIGHTOPTION" | "ALTGR" => parsed.alt.add_right(),
            "SHIFT" => parsed.shift.add_any(),
            "LSHIFT" | "LEFTSHIFT" => parsed.shift.add_left(),
            "RSHIFT" | "RIGHTSHIFT" => parsed.shift.add_right(),
            other => {
                if main_key.is_some() {
                    return Err(format!("Multiple non-modifier keys in '{input}'"));
                }
                main_key = Some(parse_key_token(other)?);
            }
        }
    }

    parsed.key = main_key
        .ok_or_else(|| "Shortcut must include a main key (e.g. A, Left, F9, Escape)".to_string())?;
    Ok(parsed)
}

pub fn normalize_shortcut(input: &str) -> Result<String, String> {
    parse_shortcut(input).map(|s| s.format_label())
}

fn parse_key_token(token: &str) -> Result<egui::Key, String> {
    use egui::Key;
    let key = match token {
        "A" => Key::A,
        "B" => Key::B,
        "C" => Key::C,
        "D" => Key::D,
        "E" => Key::E,
        "F" => Key::F,
        "G" => Key::G,
        "H" => Key::H,
        "I" => Key::I,
        "J" => Key::J,
        "K" => Key::K,
        "L" => Key::L,
        "M" => Key::M,
        "N" => Key::N,
        "O" => Key::O,
        "P" => Key::P,
        "Q" => Key::Q,
        "R" => Key::R,
        "S" => Key::S,
        "T" => Key::T,
        "U" => Key::U,
        "V" => Key::V,
        "W" => Key::W,
        "X" => Key::X,
        "Y" => Key::Y,
        "Z" => Key::Z,
        "0" | "DIGIT0" | "NUM0" => Key::Num0,
        "1" | "DIGIT1" | "NUM1" => Key::Num1,
        "2" | "DIGIT2" | "NUM2" => Key::Num2,
        "3" | "DIGIT3" | "NUM3" => Key::Num3,
        "4" | "DIGIT4" | "NUM4" => Key::Num4,
        "5" | "DIGIT5" | "NUM5" => Key::Num5,
        "6" | "DIGIT6" | "NUM6" => Key::Num6,
        "7" | "DIGIT7" | "NUM7" => Key::Num7,
        "8" | "DIGIT8" | "NUM8" => Key::Num8,
        "9" | "DIGIT9" | "NUM9" => Key::Num9,
        "F1" => Key::F1,
        "F2" => Key::F2,
        "F3" => Key::F3,
        "F4" => Key::F4,
        "F5" => Key::F5,
        "F6" => Key::F6,
        "F7" => Key::F7,
        "F8" => Key::F8,
        "F9" => Key::F9,
        "F10" => Key::F10,
        "F11" => Key::F11,
        "F12" => Key::F12,
        "SPACE" | "SPACEBAR" => Key::Space,
        "TAB" => Key::Tab,
        "ESCAPE" | "ESC" => Key::Escape,
        "ENTER" | "RETURN" => Key::Enter,
        "BACKSPACE" => Key::Backspace,
        "HOME" => Key::Home,
        "END" => Key::End,
        "PAGEUP" | "PGUP" => Key::PageUp,
        "PAGEDOWN" | "PGDN" => Key::PageDown,
        "INSERT" | "INS" => Key::Insert,
        "DELETE" | "DEL" => Key::Delete,
        "UP" | "ARROWUP" => Key::ArrowUp,
        "DOWN" | "ARROWDOWN" => Key::ArrowDown,
        "LEFT" | "ARROWLEFT" => Key::ArrowLeft,
        "RIGHT" | "ARROWRIGHT" => Key::ArrowRight,
        "COMMA" | "," => Key::Comma,
        "PERIOD" | "." => Key::Period,
        "SLASH" | "/" => Key::Slash,
        "BACKSLASH" | "\\" => Key::Backslash,
        "BACKQUOTE" | "BACKTICK" | "GRAVE" | "`" | "~" => Key::Backtick,
        "MINUS" | "-" => Key::Minus,
        "EQUAL" | "EQUALS" | "=" => Key::Equals,
        _ => return Err(format!("Unsupported key '{token}'")),
    };
    Ok(key)
}

fn egui_key_to_label(key: egui::Key) -> &'static str {
    use egui::Key;
    match key {
        Key::A => "A",
        Key::B => "B",
        Key::C => "C",
        Key::D => "D",
        Key::E => "E",
        Key::F => "F",
        Key::G => "G",
        Key::H => "H",
        Key::I => "I",
        Key::J => "J",
        Key::K => "K",
        Key::L => "L",
        Key::M => "M",
        Key::N => "N",
        Key::O => "O",
        Key::P => "P",
        Key::Q => "Q",
        Key::R => "R",
        Key::S => "S",
        Key::T => "T",
        Key::U => "U",
        Key::V => "V",
        Key::W => "W",
        Key::X => "X",
        Key::Y => "Y",
        Key::Z => "Z",
        Key::Num0 => "0",
        Key::Num1 => "1",
        Key::Num2 => "2",
        Key::Num3 => "3",
        Key::Num4 => "4",
        Key::Num5 => "5",
        Key::Num6 => "6",
        Key::Num7 => "7",
        Key::Num8 => "8",
        Key::Num9 => "9",
        Key::F1 => "F1",
        Key::F2 => "F2",
        Key::F3 => "F3",
        Key::F4 => "F4",
        Key::F5 => "F5",
        Key::F6 => "F6",
        Key::F7 => "F7",
        Key::F8 => "F8",
        Key::F9 => "F9",
        Key::F10 => "F10",
        Key::F11 => "F11",
        Key::F12 => "F12",
        Key::Space => "Space",
        Key::Tab => "Tab",
        Key::Escape => "Escape",
        Key::Enter => "Enter",
        Key::Backspace => "Backspace",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PageUp",
        Key::PageDown => "PageDown",
        Key::Insert => "Insert",
        Key::Delete => "Delete",
        Key::ArrowUp => "Up",
        Key::ArrowDown => "Down",
        Key::ArrowLeft => "Left",
        Key::ArrowRight => "Right",
        Key::Comma => "Comma",
        Key::Period => "Period",
        Key::Slash => "Slash",
        Key::Backslash => "Backslash",
        Key::Backtick => "Backquote",
        Key::Minus => "Minus",
        Key::Equals => "Equal",
        _ => "Unknown",
    }
}

fn egui_key_to_global_code(key: egui::Key) -> Result<Code, String> {
    use egui::Key;
    let code = match key {
        Key::A => Code::KeyA,
        Key::B => Code::KeyB,
        Key::C => Code::KeyC,
        Key::D => Code::KeyD,
        Key::E => Code::KeyE,
        Key::F => Code::KeyF,
        Key::G => Code::KeyG,
        Key::H => Code::KeyH,
        Key::I => Code::KeyI,
        Key::J => Code::KeyJ,
        Key::K => Code::KeyK,
        Key::L => Code::KeyL,
        Key::M => Code::KeyM,
        Key::N => Code::KeyN,
        Key::O => Code::KeyO,
        Key::P => Code::KeyP,
        Key::Q => Code::KeyQ,
        Key::R => Code::KeyR,
        Key::S => Code::KeyS,
        Key::T => Code::KeyT,
        Key::U => Code::KeyU,
        Key::V => Code::KeyV,
        Key::W => Code::KeyW,
        Key::X => Code::KeyX,
        Key::Y => Code::KeyY,
        Key::Z => Code::KeyZ,
        Key::Num0 => Code::Digit0,
        Key::Num1 => Code::Digit1,
        Key::Num2 => Code::Digit2,
        Key::Num3 => Code::Digit3,
        Key::Num4 => Code::Digit4,
        Key::Num5 => Code::Digit5,
        Key::Num6 => Code::Digit6,
        Key::Num7 => Code::Digit7,
        Key::Num8 => Code::Digit8,
        Key::Num9 => Code::Digit9,
        Key::F1 => Code::F1,
        Key::F2 => Code::F2,
        Key::F3 => Code::F3,
        Key::F4 => Code::F4,
        Key::F5 => Code::F5,
        Key::F6 => Code::F6,
        Key::F7 => Code::F7,
        Key::F8 => Code::F8,
        Key::F9 => Code::F9,
        Key::F10 => Code::F10,
        Key::F11 => Code::F11,
        Key::F12 => Code::F12,
        Key::Space => Code::Space,
        Key::Tab => Code::Tab,
        Key::Escape => Code::Escape,
        Key::Enter => Code::Enter,
        Key::Backspace => Code::Backspace,
        Key::Home => Code::Home,
        Key::End => Code::End,
        Key::PageUp => Code::PageUp,
        Key::PageDown => Code::PageDown,
        Key::Insert => Code::Insert,
        Key::Delete => Code::Delete,
        Key::ArrowUp => Code::ArrowUp,
        Key::ArrowDown => Code::ArrowDown,
        Key::ArrowLeft => Code::ArrowLeft,
        Key::ArrowRight => Code::ArrowRight,
        Key::Comma => Code::Comma,
        Key::Period => Code::Period,
        Key::Slash => Code::Slash,
        Key::Backslash => Code::Backslash,
        Key::Backtick => Code::Backquote,
        Key::Minus => Code::Minus,
        Key::Equals => Code::Equal,
        _ => return Err("Key cannot be used as a global shortcut".to_string()),
    };
    Ok(code)
}

fn egui_key_to_vk_code(key: egui::Key) -> Result<u32, String> {
    use egui::Key;
    let vk = match key {
        Key::A => 0x41,
        Key::B => 0x42,
        Key::C => 0x43,
        Key::D => 0x44,
        Key::E => 0x45,
        Key::F => 0x46,
        Key::G => 0x47,
        Key::H => 0x48,
        Key::I => 0x49,
        Key::J => 0x4A,
        Key::K => 0x4B,
        Key::L => 0x4C,
        Key::M => 0x4D,
        Key::N => 0x4E,
        Key::O => 0x4F,
        Key::P => 0x50,
        Key::Q => 0x51,
        Key::R => 0x52,
        Key::S => 0x53,
        Key::T => 0x54,
        Key::U => 0x55,
        Key::V => 0x56,
        Key::W => 0x57,
        Key::X => 0x58,
        Key::Y => 0x59,
        Key::Z => 0x5A,
        Key::Num0 => 0x30,
        Key::Num1 => 0x31,
        Key::Num2 => 0x32,
        Key::Num3 => 0x33,
        Key::Num4 => 0x34,
        Key::Num5 => 0x35,
        Key::Num6 => 0x36,
        Key::Num7 => 0x37,
        Key::Num8 => 0x38,
        Key::Num9 => 0x39,
        Key::F1 => 0x70,
        Key::F2 => 0x71,
        Key::F3 => 0x72,
        Key::F4 => 0x73,
        Key::F5 => 0x74,
        Key::F6 => 0x75,
        Key::F7 => 0x76,
        Key::F8 => 0x77,
        Key::F9 => 0x78,
        Key::F10 => 0x79,
        Key::F11 => 0x7A,
        Key::F12 => 0x7B,
        Key::Space => 0x20,
        Key::Tab => 0x09,
        Key::Escape => 0x1B,
        Key::Enter => 0x0D,
        Key::Backspace => 0x08,
        Key::PageUp => 0x21,
        Key::PageDown => 0x22,
        Key::End => 0x23,
        Key::Home => 0x24,
        Key::ArrowLeft => 0x25,
        Key::ArrowUp => 0x26,
        Key::ArrowRight => 0x27,
        Key::ArrowDown => 0x28,
        Key::Insert => 0x2D,
        Key::Delete => 0x2E,
        Key::Equals => 0xBB,
        Key::Comma => 0xBC,
        Key::Minus => 0xBD,
        Key::Period => 0xBE,
        Key::Slash => 0xBF,
        Key::Backtick => 0xC0,
        Key::Backslash => 0xDC,
        _ => return Err("Key cannot be used as a global shortcut".to_string()),
    };
    Ok(vk)
}

/// Captures a shortcut from egui input when recording a keybind in Settings.
/// When `distinguish_sides` is true, Left/Right modifier sides (`LShift`, `RShift`, `LCtrl`, ...)
/// are recorded via Win32 `GetAsyncKeyState`. Returns `Some("ESCAPE_CANCEL")` if Escape is pressed unmodified.
pub fn capture_shortcut(ctx: &egui::Context, distinguish_sides: bool) -> Option<String> {
    let mut sides = read_modifier_side_state();
    ctx.input(|i| {
        for event in &i.events {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            {
                if *key == egui::Key::Escape
                    && !modifiers.ctrl
                    && !modifiers.command
                    && !modifiers.alt
                    && !modifiers.shift
                    && !sides.any_win()
                {
                    return Some("ESCAPE_CANCEL".to_string());
                }
                let label = egui_key_to_label(*key);
                if label != "Unknown" {
                    let parsed = if distinguish_sides {
                        if sides.ralt && sides.lctrl && !modifiers.ctrl && !modifiers.command {
                            sides.lctrl = false;
                        }
                        let win = ModifierRequirement::from_sides(sides.lwin, sides.rwin);
                        let ctrl = if sides.any_ctrl() {
                            ModifierRequirement::from_sides(sides.lctrl, sides.rctrl)
                        } else if modifiers.ctrl || modifiers.command {
                            ModifierRequirement::Any
                        } else {
                            ModifierRequirement::None
                        };
                        let alt = if sides.any_alt() {
                            ModifierRequirement::from_sides(sides.lalt, sides.ralt)
                        } else if modifiers.alt {
                            ModifierRequirement::Any
                        } else {
                            ModifierRequirement::None
                        };
                        let shift = if sides.any_shift() {
                            ModifierRequirement::from_sides(sides.lshift, sides.rshift)
                        } else if modifiers.shift {
                            ModifierRequirement::Any
                        } else {
                            ModifierRequirement::None
                        };
                        ParsedShortcut {
                            win,
                            ctrl,
                            alt,
                            shift,
                            key: *key,
                        }
                    } else {
                        let any_if = |b: bool| {
                            if b {
                                ModifierRequirement::Any
                            } else {
                                ModifierRequirement::None
                            }
                        };
                        ParsedShortcut {
                            win: any_if(sides.any_win()),
                            ctrl: any_if(modifiers.ctrl || modifiers.command),
                            alt: any_if(modifiers.alt),
                            shift: any_if(modifiers.shift),
                            key: *key,
                        }
                    };
                    return Some(parsed.format_label());
                }
            }
        }
        None
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InAppAction {
    ToggleGridView,
    PrevNote,
    NextNote,
    NewNote,
    DeleteNote,
    OpenSettings,
    HideWindow,
    ToggleAlwaysOnTop,
    CopyNote,
    ToggleChecklist,
    SearchNotes,
    FormatBold,
    FormatItalic,
    FormatCode,
}

#[derive(Debug, Clone, Copy)]
struct ActionBinding<'a> {
    action: InAppAction,
    shortcut: &'a str,
}

/// Checks and consumes any matching in-app shortcut from `ctx` input so the key event
/// does not also type stray text into the editor.
pub fn poll_in_app_action(ctx: &egui::Context, keybinds: &Keybinds) -> Option<InAppAction> {
    let bindings = [
        ActionBinding {
            action: InAppAction::ToggleGridView,
            shortcut: &keybinds.toggle_grid_view,
        },
        ActionBinding {
            action: InAppAction::PrevNote,
            shortcut: &keybinds.prev_note,
        },
        ActionBinding {
            action: InAppAction::NextNote,
            shortcut: &keybinds.next_note,
        },
        ActionBinding {
            action: InAppAction::NewNote,
            shortcut: &keybinds.new_note,
        },
        ActionBinding {
            action: InAppAction::DeleteNote,
            shortcut: &keybinds.delete_note,
        },
        ActionBinding {
            action: InAppAction::OpenSettings,
            shortcut: &keybinds.open_settings,
        },
        ActionBinding {
            action: InAppAction::ToggleAlwaysOnTop,
            shortcut: &keybinds.toggle_always_on_top,
        },
        ActionBinding {
            action: InAppAction::CopyNote,
            shortcut: &keybinds.copy_note,
        },
        ActionBinding {
            action: InAppAction::ToggleChecklist,
            shortcut: &keybinds.toggle_checklist,
        },
        ActionBinding {
            action: InAppAction::SearchNotes,
            shortcut: &keybinds.search_notes,
        },
        ActionBinding {
            action: InAppAction::FormatBold,
            shortcut: &keybinds.format_bold,
        },
        ActionBinding {
            action: InAppAction::FormatItalic,
            shortcut: &keybinds.format_italic,
        },
        ActionBinding {
            action: InAppAction::FormatCode,
            shortcut: &keybinds.format_code,
        },
        // Check HideWindow last so more specific shortcuts using Escape win if configured
        ActionBinding {
            action: InAppAction::HideWindow,
            shortcut: &keybinds.hide_window,
        },
    ];

    let parsed_list: Vec<(InAppAction, ParsedShortcut)> = bindings
        .iter()
        .filter_map(|b| parse_shortcut(b.shortcut).ok().map(|p| (b.action, p)))
        .collect();

    let win_down = is_win_key_pressed();
    ctx.input_mut(|i| {
        let mut matched: Option<(usize, InAppAction)> = None;
        for (idx, event) in i.events.iter().enumerate() {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            {
                for (action, parsed) in &parsed_list {
                    if parsed.matches_key_event(*key, *modifiers, win_down) {
                        matched = Some((idx, *action));
                        break;
                    }
                }
            }
            if matched.is_some() {
                break;
            }
        }

        if let Some((ev_idx, action)) = matched {
            i.events.remove(ev_idx);
            Some(action)
        } else {
            None
        }
    })
}

/// Accumulates raw `Event::MouseWheel` events while the configured scroll-switch modifier
/// is held, consuming them before `egui` feeds its multi-frame `smooth_scroll_delta` queue.
#[derive(Debug, Default)]
pub struct ScrollNoteSwitcher {
    accumulated_detents: f32,
    last_event_at: Option<Instant>,
    last_switch_at: Option<Instant>,
}

impl ScrollNoteSwitcher {
    const POINTS_PER_DETENT: f32 = 40.0;
    const DETENT_THRESHOLD: f32 = 0.999;
    const ACCUM_IDLE_RESET: Duration = Duration::from_millis(250);
    const SWITCH_DEBOUNCE: Duration = Duration::from_millis(50);

    pub fn reset(&mut self) {
        self.accumulated_detents = 0.0;
        self.last_event_at = None;
    }

    /// Inspects `raw_input.events` before `egui` processes them, removing any `Event::MouseWheel`
    /// events matching `scroll_mod`. Returns `-1` (previous note), `+1` (next note), or `0`.
    pub fn consume_raw_input(
        &mut self,
        raw_input: &mut egui::RawInput,
        scroll_mod: ScrollModifier,
        now: Instant,
    ) -> i32 {
        let mut frame_delta = 0.0_f32;
        let mut consumed_any = false;

        raw_input.events.retain(|ev| {
            let egui::Event::MouseWheel {
                unit,
                delta,
                modifiers,
            } = ev
            else {
                return true;
            };

            if !scroll_mod.matches(*modifiers) {
                return true;
            }

            consumed_any = true;
            let dominant = if delta.x.abs() > delta.y.abs() {
                delta.x
            } else {
                delta.y
            };
            let detents = match unit {
                egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => dominant,
                egui::MouseWheelUnit::Point => dominant / Self::POINTS_PER_DETENT,
            };
            frame_delta += detents;
            false
        });

        if !consumed_any {
            if !scroll_mod.matches(raw_input.modifiers)
                || self
                    .last_event_at
                    .is_some_and(|t| now.saturating_duration_since(t) >= Self::ACCUM_IDLE_RESET)
            {
                self.reset();
            }
            return 0;
        }

        if self
            .last_event_at
            .is_some_and(|t| now.saturating_duration_since(t) >= Self::ACCUM_IDLE_RESET)
        {
            self.accumulated_detents = 0.0;
        }
        self.last_event_at = Some(now);

        if frame_delta == 0.0 {
            return 0;
        }

        if self
            .last_switch_at
            .is_some_and(|t| now.saturating_duration_since(t) < Self::SWITCH_DEBOUNCE)
        {
            self.accumulated_detents = 0.0;
            return 0;
        }

        if self.accumulated_detents != 0.0
            && self.accumulated_detents.signum() != frame_delta.signum()
        {
            self.accumulated_detents = 0.0;
        }

        self.accumulated_detents += frame_delta;

        if self.accumulated_detents.abs() >= Self::DETENT_THRESHOLD {
            // Wheel down (negative y) / wheel right (negative x) -> next note (+1);
            // wheel up (positive y) / wheel left (positive x) -> previous note (-1).
            let dir = if self.accumulated_detents < 0.0 {
                1
            } else {
                -1
            };
            self.accumulated_detents = 0.0;
            self.last_switch_at = Some(now);
            dir
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shortcut_parsing_and_normalization() {
        let win_shift_a = parse_shortcut("win+shift+a").unwrap();
        assert_eq!(win_shift_a.win, ModifierRequirement::Any);
        assert_eq!(win_shift_a.shift, ModifierRequirement::Any);
        assert_eq!(win_shift_a.ctrl, ModifierRequirement::None);
        assert_eq!(win_shift_a.alt, ModifierRequirement::None);
        assert!(!win_shift_a.is_side_specific());
        assert_eq!(win_shift_a.key, egui::Key::A);
        assert_eq!(win_shift_a.format_label(), "Win+Shift+A");
        assert!(win_shift_a.to_global_hotkey().is_ok());

        let lwin_rshift_a = parse_shortcut("lwin + rshift + a").unwrap();
        assert_eq!(lwin_rshift_a.win, ModifierRequirement::Left);
        assert_eq!(lwin_rshift_a.shift, ModifierRequirement::Right);
        assert!(lwin_rshift_a.is_side_specific());
        assert_eq!(lwin_rshift_a.format_label(), "LWin+RShift+A");
        let spec = lwin_rshift_a.to_side_aware_spec().unwrap();
        assert_eq!(spec.vk_code, 0x41);
        assert_eq!(spec.win, ModifierRequirement::Left);
        assert_eq!(spec.shift, ModifierRequirement::Right);

        let rctrl_lalt_space = parse_shortcut("rightctrl + leftalt + space").unwrap();
        assert_eq!(rctrl_lalt_space.format_label(), "RCtrl+LAlt+Space");
        assert!(rctrl_lalt_space.is_side_specific());
        assert_eq!(rctrl_lalt_space.to_side_aware_spec().unwrap().vk_code, 0x20);

        let both_shifts = parse_shortcut("lshift+rshift+a").unwrap();
        assert_eq!(both_shifts.shift, ModifierRequirement::Both);
        assert_eq!(both_shifts.format_label(), "LShift+RShift+A");

        let alt_a = parse_shortcut("alt + a").unwrap();
        assert_eq!(alt_a.format_label(), "Alt+A");
        assert!(alt_a.matches_key_event(egui::Key::A, egui::Modifiers::ALT, false));
        assert!(!alt_a.matches_key_event(egui::Key::A, egui::Modifiers::NONE, false));

        let comma = parse_shortcut("ctrl+,").unwrap();
        assert_eq!(comma.format_label(), "Ctrl+Comma");
        assert!(comma.to_global_hotkey().is_ok());

        assert!(parse_shortcut("Ctrl+Shift").is_err());
        assert!(parse_shortcut("").is_err());
    }

    fn wheel_raw_input(
        time_secs: f64,
        modifiers: egui::Modifiers,
        events: Vec<egui::Event>,
    ) -> egui::RawInput {
        egui::RawInput {
            time: Some(time_secs),
            modifiers,
            events,
            ..Default::default()
        }
    }

    #[test]
    fn test_scroll_note_switcher_single_notch_does_not_double_switch_with_egui_smoothing() {
        let mut switcher = ScrollNoteSwitcher::default();
        let ctx = egui::Context::default();
        let t0 = Instant::now();

        let mut down_input = wheel_raw_input(
            0.0,
            egui::Modifiers::ALT,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -1.0),
                modifiers: egui::Modifiers::ALT,
            }],
        );
        assert_eq!(
            switcher.consume_raw_input(&mut down_input, ScrollModifier::Alt, t0),
            1
        );
        assert!(down_input.events.is_empty());

        let _ = ctx.run(down_input, |ctx| {
            ctx.input(|i| {
                assert_eq!(i.raw_scroll_delta, egui::Vec2::ZERO);
                assert_eq!(i.smooth_scroll_delta, egui::Vec2::ZERO);
            });
        });

        for ms in [16_u64, 64, 128, 160] {
            let mut follow_up = wheel_raw_input(ms as f64 / 1000.0, egui::Modifiers::ALT, vec![]);
            assert_eq!(
                switcher.consume_raw_input(
                    &mut follow_up,
                    ScrollModifier::Alt,
                    t0 + Duration::from_millis(ms),
                ),
                0
            );
            let _ = ctx.run(follow_up, |ctx| {
                ctx.input(|i| {
                    assert_eq!(i.raw_scroll_delta, egui::Vec2::ZERO);
                    assert_eq!(i.smooth_scroll_delta, egui::Vec2::ZERO);
                });
            });
        }

        let mut up_input = wheel_raw_input(
            0.25,
            egui::Modifiers::ALT,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, 1.0),
                modifiers: egui::Modifiers::ALT,
            }],
        );
        assert_eq!(
            switcher.consume_raw_input(
                &mut up_input,
                ScrollModifier::Alt,
                t0 + Duration::from_millis(250),
            ),
            -1
        );
    }

    #[test]
    fn test_scroll_note_switcher_high_res_and_point_deltas_accumulate() {
        let mut switcher = ScrollNoteSwitcher::default();
        let t0 = Instant::now();

        for step in 0..7 {
            let mut raw = wheel_raw_input(
                step as f64 * 0.01,
                egui::Modifiers::ALT,
                vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, -0.125),
                    modifiers: egui::Modifiers::ALT,
                }],
            );
            assert_eq!(
                switcher.consume_raw_input(
                    &mut raw,
                    ScrollModifier::Alt,
                    t0 + Duration::from_millis(step * 10),
                ),
                0
            );
            assert!(raw.events.is_empty());
        }

        let mut eighth = wheel_raw_input(
            0.07,
            egui::Modifiers::ALT,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -0.125),
                modifiers: egui::Modifiers::ALT,
            }],
        );
        assert_eq!(
            switcher.consume_raw_input(
                &mut eighth,
                ScrollModifier::Alt,
                t0 + Duration::from_millis(70),
            ),
            1
        );

        let mut trailing = wheel_raw_input(
            0.08,
            egui::Modifiers::ALT,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -0.125),
                modifiers: egui::Modifiers::ALT,
            }],
        );
        assert_eq!(
            switcher.consume_raw_input(
                &mut trailing,
                ScrollModifier::Alt,
                t0 + Duration::from_millis(80),
            ),
            0
        );

        let mut point_switcher = ScrollNoteSwitcher::default();
        let mut results = Vec::new();
        for step in 0..4 {
            let mut raw = wheel_raw_input(
                step as f64 * 0.01,
                egui::Modifiers::ALT,
                vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 10.0),
                    modifiers: egui::Modifiers::ALT,
                }],
            );
            results.push(point_switcher.consume_raw_input(
                &mut raw,
                ScrollModifier::Alt,
                t0 + Duration::from_millis(step * 10),
            ));
        }
        assert_eq!(results, vec![0, 0, 0, -1]);
    }

    #[test]
    fn test_scroll_note_switcher_large_delta_and_debounce_and_consecutive_notches() {
        let mut switcher = ScrollNoteSwitcher::default();
        let t0 = Instant::now();

        let mut large = wheel_raw_input(
            0.0,
            egui::Modifiers::ALT,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -3.0),
                modifiers: egui::Modifiers::ALT,
            }],
        );
        assert_eq!(
            switcher.consume_raw_input(&mut large, ScrollModifier::Alt, t0),
            1
        );

        let mut empty = wheel_raw_input(0.06, egui::Modifiers::ALT, vec![]);
        assert_eq!(
            switcher.consume_raw_input(
                &mut empty,
                ScrollModifier::Alt,
                t0 + Duration::from_millis(60),
            ),
            0
        );

        let mut bounce = wheel_raw_input(
            0.02,
            egui::Modifiers::ALT,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -1.0),
                modifiers: egui::Modifiers::ALT,
            }],
        );
        assert_eq!(
            switcher.consume_raw_input(
                &mut bounce,
                ScrollModifier::Alt,
                t0 + Duration::from_millis(20),
            ),
            0
        );
        assert!(bounce.events.is_empty());

        let mut second = wheel_raw_input(
            0.08,
            egui::Modifiers::ALT,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -1.0),
                modifiers: egui::Modifiers::ALT,
            }],
        );
        assert_eq!(
            switcher.consume_raw_input(
                &mut second,
                ScrollModifier::Alt,
                t0 + Duration::from_millis(80),
            ),
            1
        );
    }

    #[test]
    fn test_scroll_note_switcher_ignores_unmodified_and_resets_on_release_or_reverse() {
        let mut switcher = ScrollNoteSwitcher::default();
        let t0 = Instant::now();

        let mut unmodified = wheel_raw_input(
            0.0,
            egui::Modifiers::NONE,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -1.0),
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            switcher.consume_raw_input(&mut unmodified, ScrollModifier::Alt, t0),
            0
        );
        assert_eq!(unmodified.events.len(), 1);

        let mut partial_down = wheel_raw_input(
            0.01,
            egui::Modifiers::ALT,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -0.5),
                modifiers: egui::Modifiers::ALT,
            }],
        );
        assert_eq!(
            switcher.consume_raw_input(
                &mut partial_down,
                ScrollModifier::Alt,
                t0 + Duration::from_millis(10),
            ),
            0
        );

        let mut released = wheel_raw_input(0.02, egui::Modifiers::NONE, vec![]);
        assert_eq!(
            switcher.consume_raw_input(
                &mut released,
                ScrollModifier::Alt,
                t0 + Duration::from_millis(20),
            ),
            0
        );

        let mut after_release = wheel_raw_input(
            0.03,
            egui::Modifiers::ALT,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -0.5),
                modifiers: egui::Modifiers::ALT,
            }],
        );
        assert_eq!(
            switcher.consume_raw_input(
                &mut after_release,
                ScrollModifier::Alt,
                t0 + Duration::from_millis(30),
            ),
            0
        );

        // Direction reversal resets prior partial -0.5 delta
        let mut rev_1 = wheel_raw_input(
            0.04,
            egui::Modifiers::ALT,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, 0.5),
                modifiers: egui::Modifiers::ALT,
            }],
        );
        assert_eq!(
            switcher.consume_raw_input(
                &mut rev_1,
                ScrollModifier::Alt,
                t0 + Duration::from_millis(40),
            ),
            0
        );
        let mut rev_2 = wheel_raw_input(
            0.05,
            egui::Modifiers::ALT,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, 0.5),
                modifiers: egui::Modifiers::ALT,
            }],
        );
        assert_eq!(
            switcher.consume_raw_input(
                &mut rev_2,
                ScrollModifier::Alt,
                t0 + Duration::from_millis(50),
            ),
            -1
        );

        // Ctrl scroll-switch modifier consumes Ctrl + MouseWheel so egui sees zero zoom/scroll delta
        let ctx = egui::Context::default();
        let mut ctrl_switcher = ScrollNoteSwitcher::default();
        let mut ctrl_input = wheel_raw_input(
            0.0,
            egui::Modifiers::CTRL,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -1.0),
                modifiers: egui::Modifiers::CTRL,
            }],
        );
        assert_eq!(
            ctrl_switcher.consume_raw_input(&mut ctrl_input, ScrollModifier::Ctrl, t0),
            1
        );
        assert!(ctrl_input.events.is_empty());
        let _ = ctx.run(ctrl_input, |ctx| {
            ctx.input(|i| {
                assert_eq!(i.zoom_delta(), 1.0);
                assert_eq!(i.raw_scroll_delta, egui::Vec2::ZERO);
                assert_eq!(i.smooth_scroll_delta, egui::Vec2::ZERO);
            });
        });
    }
}
