//! Persisted settings. Mirrors src/types.ts; serde camelCase keeps the JSON identical on both sides.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf};

/// Folder name for the default clips and cache folders.
pub const APP_DIR: &str = "ClipMaker";
/// The app's name and config identifier before it was renamed to ClipMaker.
const OLD_APP_DIR: &str = "Drazox Recorder";
pub const OLD_IDENTIFIER: &str = "com.drazox.recorder";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GeneralSettings {
	pub language: String,
	pub autostart: bool,
	pub start_minimized: bool,
	pub start_buffer_on_launch: bool,
	pub toast_on_save: bool,
	pub save_sound: bool,
	pub save_sound_volume: u32,
}

impl Default for GeneralSettings {
	fn default() -> Self {
		Self { language: "en".into(), autostart: false, start_minimized: false, start_buffer_on_launch: true, toast_on_save: true, save_sound: false, save_sound_volume: 60 }
	}
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ResolutionSettings {
	pub preset: String,
	pub width: u32,
	pub height: u32,
	pub keep_aspect: bool,
}

impl Default for ResolutionSettings {
	fn default() -> Self {
		Self { preset: "1080".into(), width: 2560, height: 1080, keep_aspect: true }
	}
}

impl ResolutionSettings {
	/// The clip size for a monitor. Presets fix the height and derive the width from the monitor's own
	/// aspect ratio, so a 3440x1440 screen at "1080" becomes 2580x1080: never letterboxed.
	pub fn output_size(&self, mon_w: u32, mon_h: u32) -> (u32, u32) {
		let even = |n: f64| ((n / 2.0).round() as u32 * 2).max(2);
		match self.preset.as_str() {
			"native" => (even(mon_w as f64), even(mon_h as f64)),
			"custom" => (even(self.width as f64), even(self.height as f64)),
			p => {
				let h = p.parse::<u32>().unwrap_or(1080).min(mon_h);
				(even(h as f64 * mon_w as f64 / mon_h as f64), even(h as f64))
			}
		}
	}
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct VideoSettings {
	pub monitor: u32,
	pub capture_api: String,
	pub capture_cursor: bool,
	pub resolution: ResolutionSettings,
	pub fps: u32,
	pub device: String,
	pub codec: String,
	pub encoder: String,
	pub rate_control: String,
	pub bitrate_kbps: u32,
	pub max_bitrate_kbps: u32,
	pub quality: u32,
	pub preset: String,
	pub profile: String,
	pub tune: String,
	pub keyframe_sec: f64,
	pub b_frames: u32,
	pub threads: u32,
	pub color_range: String,
	/// "recording" (default): the recorder's GPU work and threads are boosted so a GPU-bound game
	/// can't starve capture. "game": everything at normal priority, best game FPS.
	pub priority: String,
}

impl Default for VideoSettings {
	fn default() -> Self {
		Self {
			monitor: 0,
			capture_api: "auto".into(),
			capture_cursor: true,
			resolution: ResolutionSettings::default(),
			fps: 60,
			device: "gpu".into(),
			codec: "h264".into(),
			encoder: String::new(),
			rate_control: "cbr".into(),
			bitrate_kbps: 30_000,
			max_bitrate_kbps: 45_000,
			quality: 23,
			preset: "balanced".into(),
			profile: "high".into(),
			tune: "transcoding".into(),
			keyframe_sec: 1.0,
			b_frames: 0,
			threads: 0,
			color_range: "limited".into(),
			priority: "recording".into(),
		}
	}
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MicSettings {
	pub enabled: bool,
	pub device_id: Option<String>,
	pub own_track: bool,
	pub volume: u32,
	pub gate_db: i32,
	/// "off" or "rnnoise"; more suppressors can be added as further values.
	pub suppression: String,
	/// 0–100: how much of the denoised signal replaces the original.
	pub suppression_strength: u32,
}

impl Default for MicSettings {
	fn default() -> Self {
		Self { enabled: false, device_id: None, own_track: true, volume: 100, gate_db: -90, suppression: "rnnoise".into(), suppression_strength: 100 }
	}
}

impl MicSettings {
	/// The denoiser strength (0–1) when suppression is on.
	pub fn denoise(&self) -> Option<f32> {
		(self.suppression == "rnnoise" && self.suppression_strength > 0).then(|| self.suppression_strength.min(100) as f32 / 100.0)
	}
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AudioSettings {
	pub system_track: bool,
	pub separate_apps: bool,
	pub default_app_rule: String,
	pub app_rules: BTreeMap<String, String>,
	/// "apps" (default): the mix track is built from one channel per app, each with its own volume.
	/// "loopback": one Windows loopback of everything (no per-app volume).
	pub mix_mode: String,
	/// Per-app volume in percent (0–200) by lowercase exe name; 100 when absent.
	pub app_volumes: BTreeMap<String, u32>,
	pub mic: MicSettings,
	pub codec: String,
	pub bitrate_kbps: u32,
	pub sample_rate: u32,
}

impl Default for AudioSettings {
	fn default() -> Self {
		Self {
			system_track: true,
			separate_apps: true,
			default_app_rule: "separate".into(),
			app_rules: BTreeMap::new(),
			mix_mode: "apps".into(),
			app_volumes: BTreeMap::new(),
			mic: MicSettings::default(),
			codec: "aac".into(),
			bitrate_kbps: 160,
			sample_rate: 48_000,
		}
	}
}

impl AudioSettings {
	pub fn rule_for(&self, exe: &str) -> &str {
		self.app_rules.get(&exe.to_lowercase()).map(String::as_str).unwrap_or(&self.default_app_rule)
	}

	/// Linear gain for an app (1.0 = unchanged).
	pub fn app_volume(&self, exe: &str) -> f32 {
		self.app_volumes.get(&exe.to_lowercase()).copied().unwrap_or(100).min(200) as f32 / 100.0
	}

	/// Opus only runs at 48 kHz, whatever the setting says.
	pub fn effective_rate(&self) -> u32 {
		if self.codec == "opus" { 48_000 } else { self.sample_rate }
	}
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ReplaySettings {
	pub length_sec: u32,
	pub buffer_mode: String,
	pub cache_dir: String,
}

impl Default for ReplaySettings {
	fn default() -> Self {
		let cache = dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join(APP_DIR).join("buffer");
		Self { length_sec: 60, buffer_mode: "memory".into(), cache_dir: cache.to_string_lossy().into() }
	}
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct OutputSettings {
	pub clips_dir: String,
	pub container: String,
	pub filename_template: String,
}

impl Default for OutputSettings {
	fn default() -> Self {
		let dir = dirs::video_dir().or_else(dirs::home_dir).unwrap_or_default().join(APP_DIR);
		Self { clips_dir: dir.to_string_lossy().into(), container: "mp4".into(), filename_template: "{app}_{date}_{time}".into() }
	}
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct HotkeySettings {
	pub save_clip: String,
	pub toggle_buffer: String,
	pub open_window: String,
}

impl Default for HotkeySettings {
	fn default() -> Self {
		Self { save_clip: "F8".into(), toggle_buffer: "Ctrl+Shift+F8".into(), open_window: "Ctrl+Shift+F9".into() }
	}
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
	pub general: GeneralSettings,
	pub video: VideoSettings,
	pub audio: AudioSettings,
	pub replay: ReplaySettings,
	pub output: OutputSettings,
	pub hotkeys: HotkeySettings,
}

impl Settings {
	pub fn load(path: &PathBuf) -> (Self, bool) {
		match fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()) {
			Some(s) => (s, true),
			None => (Self::default(), false),
		}
	}

	/// Points folders still at the pre-rename defaults (`Videos\Drazox Recorder`, ...) at the new ones,
	/// moving each old folder over when it can. Returns whether anything changed.
	pub fn migrate_rename(&mut self) -> bool {
		let mut changed = false;
		for (dir, new) in [(&mut self.output.clips_dir, OutputSettings::default().clips_dir), (&mut self.replay.cache_dir, ReplaySettings::default().cache_dir)] {
			let old = new.replacen(APP_DIR, OLD_APP_DIR, 1);
			if *dir == old {
				// A folder that can't move (a clip open in a player) stays where it is, under its old name.
				if PathBuf::from(&old).exists() && (PathBuf::from(&new).exists() || fs::rename(&old, &new).is_err()) {
					continue;
				}
				*dir = new;
				changed = true;
			}
		}
		changed
	}

	pub fn save(&self, path: &PathBuf) -> anyhow::Result<()> {
		if let Some(dir) = path.parent() {
			fs::create_dir_all(dir)?;
		}
		fs::write(path, serde_json::to_string_pretty(self)?)?;
		Ok(())
	}

	/// Whether a change touches the capture pipeline, which then has to restart.
	pub fn pipeline_differs(&self, other: &Settings) -> bool {
		self.video != other.video || self.replay != other.replay || self.audio.system_track != other.audio.system_track || self.audio.mix_mode != other.audio.mix_mode || self.audio.mic != other.audio.mic || self.audio.codec != other.audio.codec || self.audio.bitrate_kbps != other.audio.bitrate_kbps || self.audio.sample_rate != other.audio.sample_rate
	}
}
