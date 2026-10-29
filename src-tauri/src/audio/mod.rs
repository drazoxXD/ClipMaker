//! Audio source threads. Each one owns a WASAPI client and its own encoder track, places samples on
//! the master clock using the QPC position WASAPI reports, and fills silence while a source is
//! quiet, so every track stays continuous and in sync with the video.

pub mod monitor;
pub mod sessions;
pub mod wasapi;

use crate::{
	clock,
	encode::audio::{AudioTrack, CHANNELS},
};
use parking_lot::Mutex;
use std::{
	collections::VecDeque,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
	time::{Duration, Instant},
};
use windows::Win32::{
	Media::Audio::{IAudioCaptureClient, IAudioClient, AUDCLNT_BUFFERFLAGS_SILENT},
	System::{
		Com::{CoInitializeEx, COINIT_MULTITHREADED},
		Threading::{CreateEventW, WaitForSingleObject},
	},
};

pub enum SourceKind {
	/// Everything except our own process tree.
	System,
	/// One app's process tree.
	App(u32),
	Mic { device: Option<String> },
}

/// Samples (interleaved stereo) waiting to be mixed into the mix track.
pub type MixQueue = Arc<Mutex<VecDeque<f32>>>;

/// The per-app channels feeding the mix track: (pid, queue). The session watcher adds and removes them.
pub type MixInputs = Arc<Mutex<Vec<(u32, MixQueue)>>>;

pub struct SourceConfig {
	pub kind: SourceKind,
	pub rate: u32,
	pub volume: f32,
	pub gate_db: i32,
	/// Mic only: RNNoise strength (0–1) when noise suppression is on.
	pub denoise: Option<f32>,
	/// Apps: volume read live per packet (the Audio page's per-app sliders); overrides `volume`.
	pub live_volume: Option<Arc<dyn Fn() -> f32 + Send + Sync>>,
	/// Mic and apps: hand a copy of the samples to the mix track.
	pub mix_out: Option<MixQueue>,
	/// System only: mic samples to add in.
	pub mix_in: Option<MixQueue>,
}

/// Start-up gap tolerated before silence is inserted to re-align with the clock.
const ALIGN_SLACK_US: i64 = 20_000;
/// When no packets arrive for this long, the track is padded up to "now".
const IDLE_PAD_US: i64 = 100_000;

pub fn com_init() {
	unsafe {
		let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
	}
}

fn open(kind: &SourceKind, rate: u32) -> anyhow::Result<IAudioClient> {
	match kind {
		SourceKind::System => wasapi::process_loopback(std::process::id(), false, rate),
		SourceKind::App(pid) => wasapi::process_loopback(*pid, true, rate),
		SourceKind::Mic { device } => wasapi::mic_client(device.as_deref(), rate),
	}
}

/// The gate's "off" position on the slider.
pub const GATE_OFF_DB: i32 = -90;

pub fn rms_db(samples: &[f32]) -> f32 {
	let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt();
	20.0 * rms.max(1e-9).log10()
}

/// RNNoise noise suppression (nnnoiseless). It works on 10 ms mono frames, so the stereo mic is
/// folded to mono, denoised, and written back to both channels one frame (10 ms) later. `strength`
/// blends between the original (0) and the fully denoised signal (1).
pub struct Denoiser {
	state: Box<nnnoiseless::DenoiseState<'static>>,
	strength: f32,
	pending: Vec<f32>,
	ready: VecDeque<f32>,
	voice: f32,
}

impl Denoiser {
	pub fn new(strength: f32) -> Self {
		Self { state: nnnoiseless::DenoiseState::new(), strength, pending: Vec::new(), ready: VecDeque::new(), voice: 0.0 }
	}

	pub fn strength(&self) -> f32 {
		self.strength
	}

	/// RNNoise's voice probability (0–1) for the newest frame.
	pub fn voice(&self) -> f32 {
		self.voice
	}

	/// Denoises interleaved stereo in place; the output has the same length as the input.
	pub fn process(&mut self, samples: &mut [f32]) {
		const N: usize = nnnoiseless::DenoiseState::FRAME_SIZE;
		// RNNoise expects 16-bit sample magnitudes.
		self.pending.extend(samples.chunks(CHANNELS).map(|f| f.iter().sum::<f32>() / f.len() as f32 * 32767.0));
		let mut out = [0f32; N];
		while self.pending.len() >= N {
			self.voice = self.state.process_frame(&mut out, &self.pending[..N]);
			for i in 0..N {
				let dry = self.pending[i];
				self.ready.push_back((dry + (out[i] - dry) * self.strength) / 32767.0);
			}
			self.pending.drain(..N);
		}
		for frame in samples.chunks_mut(CHANNELS) {
			let v = self.ready.pop_front().unwrap_or(0.0).clamp(-1.0, 1.0);
			frame.iter_mut().for_each(|s| *s = v);
		}
	}
}

pub fn apply_volume(samples: &mut [f32], volume: f32) {
	if (volume - 1.0).abs() > f32::EPSILON {
		samples.iter_mut().for_each(|s| *s = (*s * volume).clamp(-1.0, 1.0));
	}
}

/// How long the gate stays open after the level last reached the threshold, so word endings and
/// short pauses between words aren't chopped off.
pub const GATE_HOLD: Duration = Duration::from_millis(250);

/// Noise gate with hold. The recorder and the Audio page's mic meter both use it, so the meter's
/// "they can hear you" is exactly what ends up in the clip.
#[derive(Default)]
pub struct Gate {
	hold_until: Option<Instant>,
}

impl Gate {
	/// Feeds one chunk's level (after volume); returns whether the gate is open.
	pub fn update(&mut self, level_db: f32, gate_db: i32) -> bool {
		if gate_db <= GATE_OFF_DB {
			return true;
		}
		let now = Instant::now();
		if level_db >= gate_db as f32 {
			self.hold_until = Some(now + GATE_HOLD);
		}
		self.hold_until.is_some_and(|t| now < t)
	}

	/// Volume, then the gate: silences the chunk while the gate is closed.
	pub fn process(&mut self, samples: &mut [f32], volume: f32, gate_db: i32) {
		apply_volume(samples, volume);
		if !self.update(rms_db(samples), gate_db) {
			samples.iter_mut().for_each(|s| *s = 0.0);
		}
	}
}

/// Adds queued mic samples into `samples`.
fn mix_from(queue: &Option<MixQueue>, samples: &mut [f32]) {
	let Some(q) = queue else { return };
	let mut q = q.lock();
	for s in samples.iter_mut() {
		match q.pop_front() {
			Some(m) => *s = (*s + m).clamp(-1.0, 1.0),
			None => break,
		}
	}
}

/// Pads a track while its own source is silent. Loopback delivers nothing then, so this is also
/// where the mic gets mixed in when no other sound is playing.
fn push_quiet(t: &mut AudioTrack, frames: usize, mix_in: &Option<MixQueue>) {
	if mix_in.is_none() {
		return t.push_silence(frames);
	}
	let mut buf = vec![0.0f32; frames * CHANNELS];
	mix_from(mix_in, &mut buf);
	t.push(&buf);
}

/// Runs one source until `stop`. `track` is None for a mic that only feeds the system mix.
pub fn run_source(cfg: SourceConfig, mut track: Option<AudioTrack>, stop: Arc<AtomicBool>) {
	com_init();
	let client = match open(&cfg.kind, cfg.rate) {
		Ok(c) => c,
		Err(e) => {
			log::warn!("audio source failed to open: {e}");
			return;
		}
	};

	unsafe {
		let Ok(event) = CreateEventW(None, false, false, None) else { return };
		if client.SetEventHandle(event).is_err() {
			return;
		}
		let Ok(capture) = client.GetService::<IAudioCaptureClient>() else { return };
		if client.Start().is_err() {
			return;
		}

		let mut last_packet_us = clock::now_us();
		let mut gate = Gate::default();
		let mut denoiser = cfg.denoise.map(Denoiser::new);
		while !stop.load(Ordering::Relaxed) {
			WaitForSingleObject(event, 50);

			let mut got_any = false;
			while capture.GetNextPacketSize().unwrap_or(0) > 0 {
				let mut data = std::ptr::null_mut();
				let mut frames = 0u32;
				let mut flags = 0u32;
				let mut qpc = 0u64;
				if capture.GetBuffer(&mut data, &mut frames, &mut flags, None, Some(&mut qpc)).is_err() {
					break;
				}
				let n = frames as usize * CHANNELS;
				let mut samples = if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() { vec![0.0f32; n] } else { std::slice::from_raw_parts(data as *const f32, n).to_vec() };
				let _ = capture.ReleaseBuffer(frames);
				got_any = true;
				last_packet_us = clock::now_us();

				// Mic chain: noise suppression → volume → gate.
				if let Some(d) = denoiser.as_mut() {
					d.process(&mut samples);
				}
				let volume = cfg.live_volume.as_ref().map_or(cfg.volume, |f| f());
				gate.process(&mut samples, volume, cfg.gate_db);

				// Mic: hand a copy to the system track's mix, and keep encoding our own track if we have one.
				if let Some(q) = &cfg.mix_out {
					let mut q = q.lock();
					q.extend(samples.iter());
					let cap = cfg.rate as usize * CHANNELS; // keep at most 1 s
					if q.len() > cap {
						let extra = q.len() - cap;
						q.drain(..extra);
					}
				}

				mix_from(&cfg.mix_in, &mut samples);

				let Some(t) = track.as_mut() else { continue };
				let packet_us = if qpc > 0 { clock::hns_to_us(qpc) } else { last_packet_us };
				let gap = packet_us - t.position_us();
				if gap > ALIGN_SLACK_US {
					push_quiet(t, (gap as i128 * cfg.rate as i128 / 1_000_000) as usize, &cfg.mix_in);
				}
				t.push(&samples);
			}

			// Quiet source (loopback delivers nothing while silent): pad so the track keeps up.
			if !got_any {
				if let Some(t) = track.as_mut() {
					let now = clock::now_us();
					if now - last_packet_us > IDLE_PAD_US {
						let target = now - IDLE_PAD_US / 2;
						let behind = target - t.position_us();
						if behind > 0 {
							push_quiet(t, (behind as i128 * cfg.rate as i128 / 1_000_000) as usize, &cfg.mix_in);
						}
					}
				}
			}
		}
		let _ = client.Stop();
		let _ = windows::Win32::Foundation::CloseHandle(event);
	}
	if let Some(t) = track.as_mut() {
		t.flush();
	}
}

/// How far behind real time the mix is built, so every channel's newest packets have arrived.
const MIX_LAG_US: i64 = 80_000;

fn mix_queue_into(q: &MixQueue, buf: &mut [f32], max_backlog: usize) {
	let mut q = q.lock();
	// A channel that ran ahead (bursty delivery) is trimmed so it can't drift out of sync.
	if q.len() > max_backlog {
		let extra = q.len() - max_backlog;
		q.drain(..extra);
	}
	for s in buf.iter_mut() {
		match q.pop_front() {
			Some(v) => *s += v,
			None => break,
		}
	}
}

/// Builds the mix track: every app channel (already at its own volume) plus the mic, summed into
/// one stereo track that stays on the master clock. Quiet channels simply contribute silence.
pub fn run_mixer(mut track: AudioTrack, inputs: MixInputs, mic: Option<MixQueue>, rate: u32, stop: Arc<AtomicBool>) {
	let max_backlog = (rate as i64 * MIX_LAG_US * 3 / 1_000_000) as usize * CHANNELS;
	while !stop.load(Ordering::Relaxed) {
		std::thread::sleep(Duration::from_millis(10));
		let behind = clock::now_us() - MIX_LAG_US - track.position_us();
		let frames = (behind.max(0) as i128 * rate as i128 / 1_000_000) as usize;
		if frames == 0 {
			continue;
		}
		let mut buf = vec![0.0f32; frames * CHANNELS];
		for (_, q) in inputs.lock().iter() {
			mix_queue_into(q, &mut buf, max_backlog);
		}
		if let Some(m) = &mic {
			mix_queue_into(m, &mut buf, max_backlog);
		}
		buf.iter_mut().for_each(|s| *s = s.clamp(-1.0, 1.0));
		track.push(&buf);
	}
	track.flush();
}