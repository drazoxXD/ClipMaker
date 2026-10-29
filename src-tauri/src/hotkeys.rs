//! Global hotkeys from Raw Input, the way Medal does it. RegisterHotKey (what the Tauri shortcut
//! plugin uses) goes silent in games that read the keyboard with RIDEV_NOHOTKEYS, Rainbow Six Siege
//! among them. A Raw Input sink sees every key on any foreground window, passively: no hook, nothing
//! injected, no added input latency for the game.

use parking_lot::RwLock;
use std::{
	collections::HashSet,
	sync::{Arc, OnceLock},
};
use windows::{
	core::w,
	Win32::{
		Foundation::{HWND, LPARAM, LRESULT, WPARAM},
		System::LibraryLoader::GetModuleHandleW,
		UI::{
			Input::{GetRawInputData, RegisterRawInputDevices, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RIDEV_INPUTSINK, RID_INPUT, RIM_TYPEKEYBOARD},
			WindowsAndMessaging::{CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, RegisterClassW, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_INPUT, WNDCLASSW},
		},
	},
};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct Mods {
	ctrl: bool,
	alt: bool,
	shift: bool,
	win: bool,
}

#[derive(Clone, Debug)]
struct Binding {
	mods: Mods,
	vk: u16,
	/// NumpadEnter and the like: the key's E0 (extended) flag must match too.
	extended: Option<bool>,
	action: Action,
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
	SaveClip,
	ToggleBuffer,
	OpenWindow,
}

struct Shared {
	bindings: RwLock<Vec<Binding>>,
	on_action: Box<dyn Fn(Action) + Send + Sync>,
	held: parking_lot::Mutex<HashSet<(u16, bool)>>,
}

static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();

const VK_SHIFT: u16 = 0x10;
const VK_CONTROL: u16 = 0x11;
const VK_MENU: u16 = 0x12;
const VK_LWIN: u16 = 0x5B;
const VK_RWIN: u16 = 0x5C;
const VK_RETURN: u16 = 0x0D;

/// Key names as the UI writes them (KeyboardEvent.code with "Key"/"Digit" stripped) to virtual keys.
fn key_vk(name: &str) -> Option<(u16, Option<bool>)> {
	let n = name.to_ascii_lowercase();
	let simple = |vk: u16| Some((vk, None));
	if n.len() == 1 {
		let c = n.as_bytes()[0];
		if c.is_ascii_lowercase() || c.is_ascii_digit() {
			return simple(c.to_ascii_uppercase() as u16);
		}
	}
	if let Some(f) = n.strip_prefix('f').and_then(|d| d.parse::<u16>().ok()).filter(|f| (1..=24).contains(f)) {
		return simple(0x70 + f - 1);
	}
	if let Some(d) = n.strip_prefix("numpad").and_then(|d| d.parse::<u16>().ok()).filter(|d| *d <= 9) {
		return simple(0x60 + d);
	}
	match n.as_str() {
		"numpadmultiply" => simple(0x6A),
		"numpadadd" => simple(0x6B),
		"numpadsubtract" => simple(0x6D),
		"numpaddecimal" => simple(0x6E),
		"numpaddivide" => simple(0x6F),
		"numpadenter" => Some((VK_RETURN, Some(true))),
		"enter" => Some((VK_RETURN, Some(false))),
		"space" => simple(0x20),
		"escape" => simple(0x1B),
		"tab" => simple(0x09),
		"backspace" => simple(0x08),
		"insert" => simple(0x2D),
		"delete" => simple(0x2E),
		"home" => simple(0x24),
		"end" => simple(0x23),
		"pageup" => simple(0x21),
		"pagedown" => simple(0x22),
		"arrowup" | "up" => simple(0x26),
		"arrowdown" | "down" => simple(0x28),
		"arrowleft" | "left" => simple(0x25),
		"arrowright" | "right" => simple(0x27),
		"printscreen" => simple(0x2C),
		"scrolllock" => simple(0x91),
		"pause" => simple(0x13),
		"backquote" => simple(0xC0),
		"minus" => simple(0xBD),
		"equal" => simple(0xBB),
		"bracketleft" => simple(0xDB),
		"bracketright" => simple(0xDD),
		"backslash" | "intlbackslash" => simple(0xDC),
		"semicolon" => simple(0xBA),
		"quote" => simple(0xDE),
		"comma" => simple(0xBC),
		"period" => simple(0xBE),
		"slash" => simple(0xBF),
		_ => None,
	}
}

fn parse(accel: &str, action: Action) -> Option<Binding> {
	let mut mods = Mods::default();
	let mut key = None;
	for part in accel.split('+').map(str::trim).filter(|p| !p.is_empty()) {
		match part.to_ascii_lowercase().as_str() {
			"ctrl" | "control" | "commandorcontrol" | "cmdorctrl" => mods.ctrl = true,
			"alt" | "option" => mods.alt = true,
			"shift" => mods.shift = true,
			"super" | "win" | "meta" | "cmd" | "command" => mods.win = true,
			_ => key = key_vk(part),
		}
	}
	let (vk, extended) = key?;
	Some(Binding { mods, vk, extended, action })
}

/// Swaps in the hotkeys from settings; unparseable or empty ones are skipped.
pub fn set_bindings(save_clip: &str, toggle_buffer: &str, open_window: &str) {
	let Some(shared) = SHARED.get() else { return };
	let list = [(save_clip, Action::SaveClip), (toggle_buffer, Action::ToggleBuffer), (open_window, Action::OpenWindow)]
		.into_iter()
		.filter_map(|(accel, action)| {
			let b = parse(accel, action);
			if b.is_none() && !accel.is_empty() {
				log::warn!("hotkey {accel:?} not understood");
			}
			b
		})
		.collect();
	*shared.bindings.write() = list;
}

fn current_mods(held: &HashSet<(u16, bool)>) -> Mods {
	let has = |vk: u16| held.iter().any(|(v, _)| *v == vk);
	Mods { ctrl: has(VK_CONTROL), alt: has(VK_MENU), shift: has(VK_SHIFT), win: has(VK_LWIN) || has(VK_RWIN) }
}

fn is_modifier(vk: u16) -> bool {
	matches!(vk, VK_SHIFT | VK_CONTROL | VK_MENU | VK_LWIN | VK_RWIN | 0xA0..=0xA5)
}

/// Normalises left/right modifier codes to the generic ones.
fn generic(vk: u16) -> u16 {
	match vk {
		0xA0 | 0xA1 => VK_SHIFT,
		0xA2 | 0xA3 => VK_CONTROL,
		0xA4 | 0xA5 => VK_MENU,
		v => v,
	}
}

/// Our own windows get keys normally; skipping them also stops the Hotkeys page from firing the
/// action while you are assigning that very key.
fn foreground_is_ours() -> bool {
	unsafe {
		let mut pid = 0u32;
		GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
		pid == std::process::id()
	}
}

unsafe fn on_raw_input(lparam: LPARAM) {
	let Some(shared) = SHARED.get() else { return };
	let mut raw = RAWINPUT::default();
	let mut size = std::mem::size_of::<RAWINPUT>() as u32;
	let got = GetRawInputData(HRAWINPUT(lparam.0 as _), RID_INPUT, Some(&mut raw as *mut _ as *mut _), &mut size, std::mem::size_of::<RAWINPUTHEADER>() as u32);
	if got == u32::MAX || raw.header.dwType != RIM_TYPEKEYBOARD.0 {
		return;
	}
	let kb = raw.data.keyboard;
	let vk = generic(kb.VKey);
	if vk == 0 || vk == 0xFF {
		return;
	}
	let extended = kb.Flags & 2 != 0; // RI_KEY_E0
	let up = kb.Flags & 1 != 0; // RI_KEY_BREAK

	let mut held = shared.held.lock();
	if up {
		held.retain(|(v, _)| *v != vk);
		return;
	}
	// Auto-repeat: the key is already down, don't fire again.
	if !held.insert((vk, extended)) || is_modifier(vk) {
		return;
	}
	let mods = current_mods(&held);
	drop(held);

	if foreground_is_ours() {
		return;
	}
	let hit = shared.bindings.read().iter().find(|b| b.vk == vk && b.mods == mods && b.extended.is_none_or(|e| e == extended)).map(|b| b.action);
	if let Some(action) = hit {
		(shared.on_action)(action);
	}
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
	if msg == WM_INPUT {
		on_raw_input(lparam);
	}
	DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Starts the listener thread once; `on_action` runs on that thread, so keep it quick.
pub fn start(on_action: impl Fn(Action) + Send + Sync + 'static) {
	if SHARED.set(Arc::new(Shared { bindings: RwLock::new(Vec::new()), on_action: Box::new(on_action), held: Default::default() })).is_err() {
		return;
	}
	let _ = std::thread::Builder::new().name("hotkeys".into()).spawn(|| unsafe {
		let instance = GetModuleHandleW(None).unwrap_or_default();
		let class = w!("DrazoxHotkeys");
		RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: instance.into(), lpszClassName: class, ..Default::default() });
		let Ok(hwnd) = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(instance.into()), None) else {
			log::error!("hotkey window could not be created");
			return;
		};
		// INPUTSINK: deliver keyboard input even while another app (the game) is in the foreground.
		let dev = RAWINPUTDEVICE { usUsagePage: 0x01, usUsage: 0x06, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd };
		if RegisterRawInputDevices(&[dev], std::mem::size_of::<RAWINPUTDEVICE>() as u32).is_err() {
			log::error!("raw input registration failed");
			return;
		}
		let mut msg = MSG::default();
		while GetMessageW(&mut msg, None, 0, 0).as_bool() {
			DispatchMessageW(&msg);
		}
	});
}
