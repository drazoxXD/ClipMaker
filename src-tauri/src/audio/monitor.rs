//! Live levels for the Audio page: which apps are making sound, and the mic before and after the
//! recorder's processing (noise suppression → volume → gate). Runs only while the page is open
//! (start/stop are reference counted), streams an `audio://levels` event about 15 times a second,
//! and can play the processed mic back (Listen) exactly as it will sound in a clip.

use super::{apply_volume, rms_db, sessions, wasapi, Denoiser, Gate};
use crate::settings::Settings;
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::{
	collections::{HashMap, HashSet, VecDeque},
	sync::{
		atomic::{AtomicBool, AtomicUsize, Ordering},
		Arc,
	},
	thread,
	time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use windows::Win32::Media::Audio::{IAudioCaptureClient, IAudioClient, IAudioRenderClient, AUDCLNT_BUFFERFLAGS_SILENT};

/// The mic is processed (and played back) every 10 ms; levels are sent every EMIT_EVERY ticks.
const TICK: Duration = Duration::from_millis(10);
const EMIT_EVERY: u64 = 7;
/// 10 ms of 48 kHz stereo.
const GATE_CHUNK: usize = 480 * 2;
/// How long an app stays listed after it goes quiet, so the list doesn't flicker between sounds.
const LINGER: Duration = Duration::from_secs(4);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveApp {
	#[serde(flatten)]
	pub app: sessions::AudioApp,
	/// False while the app is only lingering after going quiet.
	pub active: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicLevel {
	/// Raw mic level before any processing, in dBFS.
	pub input_db: f32,
	/// Level after noise suppression and volume, in dBFS; what the gate compares against.
	pub rms_db: f32,
	pub peak_db: f32,
	pub gate_open: bool,
	/// RNNoise's voice probability (0–1), when suppression is on.
	pub voice: Option<f32>,
	pub listening: bool,
	pub error: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Levels {
	pub apps: Vec<LiveApp>,
	pub mic: Option<MicLevel>,
}

struct Mic {
	device: Option<String>,
	client: IAudioClient,
	capture: IAudioCaptureClient,
}

impl Mic {
	fn open(device: Option<String>, rate: u32) -> anyhow::Result<Self> {
		let client = wasapi::mic_client_polled(device.as_deref(), rate)?;
		let capture = unsafe { client.GetService::<IAudioCaptureClient>()? };
		unsafe { client.Start()? };
		Ok(Self { device, client, capture })
	}

	/// Everything captured since the last call, interleaved stereo.
	fn drain(&self) -> Vec<f32> {
		let mut out = Vec::new();
		unsafe {
			while self.capture.GetNextPacketSize().unwrap_or(0) > 0 {
				let mut data = std::ptr::null_mut();
				let mut frames = 0u32;
				let mut flags = 0u32;
				if self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None).is_err() {
					break;
				}
				let n = frames as usize * 2;
				if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
					out.resize(out.len() + n, 0.0);
				} else {
					out.extend_from_slice(std::slice::from_raw_parts(data as *const f32, n));
				}
				let _ = self.capture.ReleaseBuffer(frames);
			}
		}
		out
	}
}

impl Drop for Mic {
	fn drop(&mut self) {
		unsafe {
			let _ = self.client.Stop();
		}
	}
}

/// Plays the processed mic on the default output for Listen. It runs inside the recorder's own
/// process, which the System track excludes, so the listen-back never ends up in a clip.
struct Speaker {
	client: IAudioClient,
	render: IAudioRenderClient,
	size: u32,
	pending: VecDeque<f32>,
}

impl Speaker {
	fn open() -> anyhow::Result<Self> {
		let client = wasapi::render_client(48_000)?;
		unsafe {
			let size = client.GetBufferSize()?;
			let render = client.GetService::<IAudioRenderClient>()?;
			client.Start()?;
			Ok(Self { client, render, size, pending: VecDeque::new() })
		}
	}

	fn play(&mut self, samples: &[f32]) {
		self.pending.extend(samples);
		// Never fall more than ~80 ms behind: drop the oldest audio instead of drifting.
		let max = 48_000 * 2 * 80 / 1000;
		if self.pending.len() > max {
			let extra = self.pending.len() - max;
			self.pending.drain(..extra);
		}
		unsafe {
			let Ok(padding) = self.client.GetCurrentPadding() else { return };
			let frames = (self.size - padding).min((self.pending.len() / 2) as u32);
			if frames == 0 {
				return;
			}
			let Ok(buf) = self.render.GetBuffer(frames) else { return };
			let out = std::slice::from_raw_parts_mut(buf as *mut f32, frames as usize * 2);
			for s in out.iter_mut() {
				*s = self.pending.pop_front().unwrap_or(0.0);
			}
			let _ = self.render.ReleaseBuffer(frames, 0);
		}
	}
}

impl Drop for Speaker {
	fn drop(&mut self) {
		unsafe {
			let _ = self.client.Stop();
		}
	}
}

pub struct Monitor {
	users: Arc<AtomicUsize>,
	running: Arc<Mutex<bool>>,
	listen: Arc<AtomicBool>,
}

impl Monitor {
	pub fn new() -> Self {
		Self { users: Arc::new(AtomicUsize::new(0)), running: Arc::new(Mutex::new(false)), listen: Arc::new(AtomicBool::new(false)) }
	}

	pub fn start(&self, app: AppHandle, settings: Arc<RwLock<Settings>>, tracked: impl Fn() -> HashSet<u32> + Send + 'static) {
		self.users.fetch_add(1, Ordering::SeqCst);
		let mut running = self.running.lock();
		if *running {
			return;
		}
		*running = true;
		let (users, flag, listen) = (self.users.clone(), self.running.clone(), self.listen.clone());
		let _ = thread::Builder::new().name("audio-monitor".into()).spawn(move || run(app, settings, users, flag, listen, tracked));
	}

	pub fn stop(&self) {
		let _ = self.users.try_update(Ordering::SeqCst, Ordering::SeqCst, |n| Some(n.saturating_sub(1)));
	}

	pub fn set_listen(&self, on: bool) {
		self.listen.store(on, Ordering::SeqCst);
	}
}

/// Mic levels gathered over the ticks between two emits.
#[derive(Default)]
struct Acc {
	raw_sq: f64,
	out_sq: f64,
	n: usize,
	peak: f32,
	voice: f32,
}

impl Acc {
	fn db(sq: f64, n: usize) -> f32 {
		if n == 0 { -90.0 } else { (20.0 * ((sq / n as f64).sqrt() as f32).max(1e-9).log10()).max(-90.0) }
	}
}

fn run(app: AppHandle, settings: Arc<RwLock<Settings>>, users: Arc<AtomicUsize>, running: Arc<Mutex<bool>>, listen: Arc<AtomicBool>, tracked: impl Fn() -> HashSet<u32>) {
	super::com_init();
	let mut seen: HashMap<u32, (sessions::AudioApp, Instant)> = HashMap::new();
	let mut mic: Option<Mic> = None;
	let mut mic_error: Option<String> = None;
	let mut gate = Gate::default();
	let mut gate_open = false;
	let mut denoiser: Option<Denoiser> = None;
	let mut speaker: Option<Speaker> = None;
	let mut acc = Acc::default();
	let mut tick = 0u64;
	let started = Instant::now();

	loop {
		// Exit only under the lock, so a start() racing with the last stop() can't be lost.
		if users.load(Ordering::SeqCst) == 0 {
			let mut r = running.lock();
			if users.load(Ordering::SeqCst) == 0 {
				*r = false;
				listen.store(false, Ordering::SeqCst);
				break;
			}
		}
		let audio = settings.read().audio.clone();
		let listening = listen.load(Ordering::SeqCst) && audio.mic.enabled;

		// Mic: (re)open when enabled or the device changed, then run this tick's audio through the
		// recorder's exact chain: noise suppression → volume → gate (in 10 ms chunks, like WASAPI).
		if audio.mic.enabled {
			if mic.as_ref().is_none_or(|m| m.device != audio.mic.device_id) {
				mic = None;
				// Retrying a broken device every tick would spam; try again about once a second.
				if mic_error.is_none() || tick % 100 == 0 {
					match Mic::open(audio.mic.device_id.clone(), 48_000) {
						Ok(m) => {
							mic = Some(m);
							mic_error = None;
						}
						Err(e) => mic_error = Some(format!("{e}")),
					}
				}
			}
			let mut samples = mic.as_ref().map(Mic::drain).unwrap_or_default();
			acc.raw_sq += samples.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>();

			match audio.mic.denoise() {
				Some(strength) => {
					if denoiser.as_ref().is_none_or(|d| d.strength() != strength) {
						denoiser = Some(Denoiser::new(strength));
					}
					let d = denoiser.as_mut().unwrap();
					d.process(&mut samples);
					acc.voice = acc.voice.max(d.voice());
				}
				None => denoiser = None,
			}
			apply_volume(&mut samples, audio.mic.volume as f32 / 100.0);
			acc.out_sq += samples.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>();
			acc.n += samples.len();
			acc.peak = samples.iter().fold(acc.peak, |m, s| m.max(s.abs()));

			if samples.is_empty() {
				gate_open = gate.update(-90.0, audio.mic.gate_db);
			}
			for chunk in samples.chunks_mut(GATE_CHUNK) {
				gate_open = gate.update(rms_db(chunk), audio.mic.gate_db);
				if !gate_open {
					chunk.iter_mut().for_each(|s| *s = 0.0);
				}
			}

			if listening {
				if speaker.is_none() {
					match Speaker::open() {
						Ok(s) => speaker = Some(s),
						Err(e) => mic_error = Some(format!("listen: {e}")),
					}
				}
				if let Some(s) = speaker.as_mut() {
					s.play(&samples);
				}
			}
		} else {
			mic = None;
			denoiser = None;
		}
		if !listening {
			speaker = None;
		}

		if tick % EMIT_EVERY == 0 {
			// Apps: refresh peaks, keep quiet ones around for LINGER.
			let tracked_now = tracked();
			let now = Instant::now();
			for a in sessions::list_all(&tracked_now) {
				let loud = a.peak > sessions::SOUND_THRESHOLD || a.tracked;
				match seen.get_mut(&a.pid) {
					Some(entry) => {
						if loud {
							entry.1 = now;
						}
						entry.0 = a;
					}
					None if loud => {
						seen.insert(a.pid, (a, now));
					}
					None => {}
				}
			}
			seen.retain(|_, (a, last)| a.tracked || now.duration_since(*last) < LINGER);
			let mut apps: Vec<LiveApp> = seen.values().map(|(a, last)| LiveApp { app: a.clone(), active: now.duration_since(*last) < Duration::from_millis(400) }).collect();
			apps.sort_by(|x, y| x.app.exe.cmp(&y.app.exe).then(x.app.pid.cmp(&y.app.pid)));

			let mic_level = audio.mic.enabled.then(|| MicLevel {
				input_db: Acc::db(acc.raw_sq, acc.n),
				rms_db: Acc::db(acc.out_sq, acc.n),
				peak_db: (20.0 * acc.peak.max(1e-9).log10()).max(-90.0),
				gate_open,
				voice: denoiser.as_ref().map(|_| acc.voice),
				listening: speaker.is_some(),
				error: mic_error.clone(),
			});
			acc = Acc::default();
			let _ = app.emit("audio://levels", Levels { apps, mic: mic_level });
		}

		tick += 1;
		// Schedule against the start time so the loop doesn't drift.
		let next = started + TICK * tick as u32;
		if let Some(left) = next.checked_duration_since(Instant::now()) {
			thread::sleep(left);
		}
	}

	drop(speaker);
	drop(mic);
}