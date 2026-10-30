mod audio;
mod buffer;
mod capture;
mod clips;
mod clock;
mod encode;
mod ff;
mod hotkeys;
mod i18n;
mod monitors;
mod recorder;
mod settings;

use encode::probe::{self, EncoderInfo};
use parking_lot::RwLock;
use recorder::Recorder;
use settings::Settings;
use std::{
	path::PathBuf,
	sync::{Arc, OnceLock},
	time::Duration,
};
use tauri::{
	menu::{Menu, MenuItem, PredefinedMenuItem},
	tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
	AppHandle, Emitter, Manager, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt as _;

/// A click-through, always-on-top transparent window in the primary monitor's top-right corner for
/// the save notifications. It is excluded from screen capture, so it never shows up in clips.
fn create_overlay(app: &tauri::App) -> tauri::Result<()> {
	use tauri::{PhysicalPosition, WebviewUrl, WebviewWindowBuilder};
	const W: f64 = 380.0;
	const H: f64 = 420.0;
	let win = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay.html".into()))
		.title("ClipMaker Overlay")
		.decorations(false)
		.transparent(true)
		.always_on_top(true)
		.skip_taskbar(true)
		.focused(false)
		.focusable(false)
		.resizable(false)
		.shadow(false)
		.inner_size(W, H)
		.visible(false)
		.build()?;
	if let Some(m) = app.primary_monitor()? {
		let sf = m.scale_factor();
		let (pos, size) = (m.position(), m.size());
		let margin = (12.0 * sf) as i32;
		win.set_position(PhysicalPosition::new(pos.x + size.width as i32 - (W * sf) as i32 - margin, pos.y + margin))?;
	}
	win.set_ignore_cursor_events(true)?;
	if let Ok(hwnd) = win.hwnd() {
		use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE}};
		unsafe {
			let _ = SetWindowDisplayAffinity(HWND(hwnd.0 as _), WDA_EXCLUDEFROMCAPTURE);
		}
	}
	// Hidden until a notification needs it; the overlay page hides itself again afterwards.
	Ok(())
}

/// Shows the (non-activating, click-through) overlay just before it displays a notification.
fn show_overlay(app: &AppHandle) {
	if let Some(w) = app.get_webview_window("overlay") {
		let _ = w.show();
	}
}

const TRAY_ID: &str = "main";

pub struct AppState {
	settings: Arc<RwLock<Settings>>,
	settings_path: PathBuf,
	thumbs_dir: PathBuf,
	recorder: Arc<Recorder>,
	encoders: Arc<OnceLock<Vec<EncoderInfo>>>,
	monitor: audio::monitor::Monitor,
}

impl AppState {
	fn encoders(&self) -> Vec<EncoderInfo> {
		// The probe runs at startup on its own thread; wait for it if a command arrives first.
		loop {
			if let Some(e) = self.encoders.get() {
				return e.clone();
			}
			std::thread::sleep(Duration::from_millis(50));
		}
	}
}

fn emit_status(app: &AppHandle) {
	let st = app.state::<AppState>().recorder.refresh_status();
	let _ = app.emit("recorder://status", st);
}

fn start_buffer(app: &AppHandle) {
	let state = app.state::<AppState>();
	let encoders = state.encoders();
	if let Err(e) = state.recorder.start(&state.settings, &encoders) {
		log::error!("start failed: {e:#}");
		state.recorder.set_error(&e);
		show_overlay(app);
		let _ = app.emit("recorder://error", format!("{e:#}"));
	}
	emit_status(app);
	rebuild_tray(app);
}

fn stop_buffer(app: &AppHandle) {
	app.state::<AppState>().recorder.stop();
	emit_status(app);
	rebuild_tray(app);
}

fn toggle_buffer(app: &AppHandle) {
	let app = app.clone();
	std::thread::spawn(move || {
		if app.state::<AppState>().recorder.is_running() {
			stop_buffer(&app)
		} else {
			start_buffer(&app)
		}
	});
}

fn save_clip(app: &AppHandle) {
	let app = app.clone();
	std::thread::spawn(move || {
		let state = app.state::<AppState>();
		let settings = state.settings.read().clone();
		// The overlay shows "saving" now and flips it to "saved" (or an error) when the file is written.
		if state.recorder.is_running() {
			if settings.general.toast_on_save {
				show_overlay(&app);
			}
			let _ = app.emit("recorder://clip-saving", ());
		}
		emit_status(&app);
		match state.recorder.save(&settings) {
			Ok(clip) => {
				let info = clips::ClipInfo {
					path: clip.path.to_string_lossy().into(),
					name: clip.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
					size_bytes: clip.size_bytes,
					duration_sec: clip.duration_sec,
					created_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
					thumbnail: None,
				};
				let _ = app.emit("recorder://clip-saved", &info);
				// The thumbnail is for the Clips page; build it after the overlay already said "saved".
				let _ = clips::thumbnail_data_url(&state.thumbs_dir, &clip.path);
			}
			Err(e) => {
				log::error!("save failed: {e:#}");
				show_overlay(&app);
				let _ = app.emit("recorder://error", format!("{e:#}"));
			}
		}
		emit_status(&app);
	});
}

fn show_window(app: &AppHandle) {
	if let Some(w) = app.get_webview_window("main") {
		let _ = w.show();
		let _ = w.unminimize();
		let _ = w.set_focus();
	}
}

fn register_hotkeys(app: &AppHandle) {
	let hk = app.state::<AppState>().settings.read().hotkeys.clone();
	hotkeys::set_bindings(&hk.save_clip, &hk.toggle_buffer, &hk.open_window);
}

fn rebuild_tray(app: &AppHandle) {
	let state = app.state::<AppState>();
	let lang = state.settings.read().general.language.clone();
	let running = state.recorder.is_running();
	let t = |k: &str| i18n::t(&lang, k, &[]);
	let build = || -> tauri::Result<Menu<tauri::Wry>> {
		let open = MenuItem::with_id(app, "open", t("tray.open"), true, None::<&str>)?;
		let toggle = MenuItem::with_id(app, "toggle", if running { t("tray.stopBuffer") } else { t("tray.startBuffer") }, true, None::<&str>)?;
		let save = MenuItem::with_id(app, "save", t("tray.saveClip"), running, None::<&str>)?;
		let quit = MenuItem::with_id(app, "quit", t("tray.quit"), true, None::<&str>)?;
		Menu::with_items(app, &[&open, &PredefinedMenuItem::separator(app)?, &toggle, &save, &PredefinedMenuItem::separator(app)?, &quit])
	};
	if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build()) {
		let _ = tray.set_menu(Some(menu));
		let _ = tray.set_tooltip(Some(if running { "ClipMaker · REC" } else { "ClipMaker" }));
	}
}

// ---------------------------------------------------------------- commands

type CmdResult<T> = Result<T, String>;

#[tauri::command]
fn get_settings(state: tauri::State<AppState>) -> Settings {
	state.settings.read().clone()
}

#[tauri::command]
fn set_settings(app: AppHandle, state: tauri::State<AppState>, settings: Settings) -> CmdResult<Settings> {
	let old = state.settings.read().clone();
	if old == settings {
		return Ok(settings);
	}
	*state.settings.write() = settings.clone();
	settings.save(&state.settings_path).map_err(|e| e.to_string())?;

	if old.hotkeys != settings.hotkeys {
		register_hotkeys(&app);
	}
	if old.general.autostart != settings.general.autostart {
		let al = app.autolaunch();
		let _ = if settings.general.autostart { al.enable() } else { al.disable() };
	}
	if old.general.language != settings.general.language {
		rebuild_tray(&app);
	}
	if state.recorder.is_running() && old.pipeline_differs(&settings) {
		let app = app.clone();
		std::thread::spawn(move || {
			stop_buffer(&app);
			start_buffer(&app);
		});
	}
	Ok(settings)
}

#[tauri::command]
fn list_monitors() -> Vec<monitors::MonitorInfo> {
	monitors::list()
}

#[tauri::command]
async fn list_encoders(state: tauri::State<'_, AppState>) -> CmdResult<Vec<EncoderInfo>> {
	Ok(state.encoders())
}

#[tauri::command]
async fn list_audio_apps(state: tauri::State<'_, AppState>) -> CmdResult<Vec<audio::sessions::AudioApp>> {
	let tracked = state.recorder.tracked_pids();
	tauri::async_runtime::spawn_blocking(move || {
		audio::com_init();
		audio::sessions::list(&tracked)
	})
	.await
	.map_err(|e| e.to_string())
}

/// The Audio page holds the live level stream open while it is mounted.
#[tauri::command]
fn monitor_start(app: AppHandle, state: tauri::State<AppState>) {
	let recorder = state.recorder.clone();
	state.monitor.start(app, state.settings.clone(), move || recorder.tracked_pids());
}

#[tauri::command]
fn monitor_stop(state: tauri::State<AppState>) {
	state.monitor.stop();
}

/// Plays the processed mic back on the default output while the Audio page is open.
#[tauri::command]
fn monitor_listen(state: tauri::State<AppState>, on: bool) {
	state.monitor.set_listen(on);
}

#[tauri::command]
async fn list_mic_devices() -> CmdResult<Vec<audio::wasapi::AudioDevice>> {
	tauri::async_runtime::spawn_blocking(|| {
		audio::com_init();
		audio::wasapi::list_mics().map_err(|e| e.to_string())
	})
	.await
	.map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_status(state: tauri::State<AppState>) -> recorder::Status {
	state.recorder.refresh_status()
}

#[tauri::command]
fn start_buffer_cmd(app: AppHandle) {
	std::thread::spawn(move || start_buffer(&app));
}

#[tauri::command]
fn stop_buffer_cmd(app: AppHandle) {
	std::thread::spawn(move || stop_buffer(&app));
}

#[tauri::command]
fn save_clip_cmd(app: AppHandle) {
	save_clip(&app);
}

#[tauri::command]
async fn list_clips(state: tauri::State<'_, AppState>) -> CmdResult<Vec<clips::ClipInfo>> {
	let dir = PathBuf::from(&state.settings.read().output.clips_dir);
	let thumbs = state.thumbs_dir.clone();
	tauri::async_runtime::spawn_blocking(move || clips::list(&dir, &thumbs)).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_clip(state: tauri::State<AppState>, path: String) -> CmdResult<()> {
	let dir = PathBuf::from(&state.settings.read().output.clips_dir);
	clips::delete(&dir, &PathBuf::from(path), &state.thumbs_dir).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_path(path: String) {
	// Folders (clips, cache) may not exist yet the first time someone opens them.
	let p = PathBuf::from(&path);
	if !p.exists() && p.extension().is_none() {
		let _ = std::fs::create_dir_all(&p);
	}
	clips::open(&path);
}

#[tauri::command]
fn reveal_path(path: String) {
	clips::reveal(&path);
}

// ---------------------------------------------------------------- app

pub fn run() {
	ff::set_log_level(ffmpeg_sys_next::AV_LOG_ERROR as i32);

	tauri::Builder::default()
		.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_window(app)))
		.plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
		.plugin(tauri_plugin_dialog::init())
		.setup(|app| {
			let handle = app.handle().clone();
			let config_dir = app.path().app_config_dir()?;
			let settings_path = config_dir.join("settings.json");
			let thumbs_dir = app.path().app_cache_dir()?.join("thumbs");
			// Settings from before the rename to ClipMaker carry over once.
			let old_settings = config_dir.with_file_name(settings::OLD_IDENTIFIER).join("settings.json");
			let carried = !settings_path.exists() && old_settings.exists() && std::fs::create_dir_all(&config_dir).and_then(|_| std::fs::copy(&old_settings, &settings_path)).is_ok();
			let (mut settings, mut existed) = Settings::load(&settings_path);
			if carried && settings.general.autostart {
				let _ = app.autolaunch().enable(); // the old entry starts the old exe
			}
			if settings.migrate_rename() {
				existed = false; // saved below, once the encoders are known
			}
			let launched_minimized = std::env::args().any(|a| a == "--minimized");

			let encoders: Arc<OnceLock<Vec<EncoderInfo>>> = Arc::new(OnceLock::new());
			app.manage(AppState { settings: Arc::new(RwLock::new(settings.clone())), settings_path: settings_path.clone(), thumbs_dir, recorder: Arc::new(Recorder::new()), encoders: encoders.clone(), monitor: audio::monitor::Monitor::new() });

			// Tray.
			TrayIconBuilder::with_id(TRAY_ID)
				.icon(app.default_window_icon().cloned().ok_or("no icon")?)
				.show_menu_on_left_click(false)
				.on_menu_event(|app, ev| match ev.id().as_ref() {
					"open" => show_window(app),
					"toggle" => toggle_buffer(app),
					"save" => save_clip(app),
					"quit" => {
						app.state::<AppState>().recorder.stop();
						app.exit(0);
					}
					_ => {}
				})
				.on_tray_icon_event(|tray, ev| {
					if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
						show_window(tray.app_handle());
					}
				})
				.build(app)?;
			rebuild_tray(&handle);
			{
				let h = handle.clone();
				hotkeys::start(move |action| match action {
					hotkeys::Action::SaveClip => save_clip(&h),
					hotkeys::Action::ToggleBuffer => toggle_buffer(&h),
					hotkeys::Action::OpenWindow => show_window(&h),
				});
			}
			register_hotkeys(&handle);
			if let Err(e) = create_overlay(app) {
				log::warn!("overlay window: {e}");
			}

			// Closing the window hides it to the tray; Quit lives in the tray menu.
			if let Some(w) = app.get_webview_window("main") {
				let w2 = w.clone();
				w.on_window_event(move |ev| {
					if let WindowEvent::CloseRequested { api, .. } = ev {
						api.prevent_close();
						let _ = w2.hide();
					}
				});
				if !(settings.general.start_minimized || launched_minimized) {
					let _ = w.show();
				}
			}

			// Probe encoders off the UI thread, then fill in a default encoder and maybe start buffering.
			std::thread::spawn(move || {
				let list = probe::probe();
				log::info!("encoders: {:?}", list.iter().map(|e| &e.id).collect::<Vec<_>>());
				let state = handle.state::<AppState>();
				{
					let mut s = state.settings.write();
					if !list.iter().any(|e| e.id == s.video.encoder) {
						if let Some(d) = probe::default_encoder(&list) {
							s.video.encoder = d.id.clone();
							s.video.codec = d.codec.clone();
							s.video.device = d.device.clone();
						}
						let _ = s.save(&settings_path);
					} else if !existed {
						let _ = s.save(&settings_path);
					}
				}
				let _ = encoders.set(list);
				if state.settings.read().general.start_buffer_on_launch {
					start_buffer(&handle);
				}
				// Status ticker.
				loop {
					std::thread::sleep(Duration::from_millis(500));
					emit_status(&handle);
				}
			});
			Ok(())
		})
		.invoke_handler(tauri::generate_handler![get_settings, set_settings, list_monitors, list_encoders, list_audio_apps, monitor_start, monitor_stop, monitor_listen, list_mic_devices, get_status, start_buffer_cmd, stop_buffer_cmd, save_clip_cmd, list_clips, delete_clip, open_path, reveal_path])
		.run(tauri::generate_context!())
		.expect("error while running ClipMaker");
}
