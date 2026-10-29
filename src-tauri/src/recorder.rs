//! The pipeline: one capture thread, one video encoder thread, one thread per audio source, a
//! session watcher that adds per-app tracks while apps make sound, and a status ticker. Saving
//! remuxes a buffer snapshot on its own thread, so recording never pauses.

use crate::{
	audio::{self, sessions, MixInputs, MixQueue, SourceConfig, SourceKind},
	buffer::{ReplayBuffer, StreamInfo, StreamKind},
	capture::{self, CaptureConfig, CaptureStats, RawFrame},
	clips, clock,
	encode::{audio::AudioTrack, probe::EncoderInfo, video},
	ff::Muxer,
	monitors,
	settings::Settings,
};
use anyhow::{anyhow, Context, Result};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::{
	collections::{HashMap, HashSet},
	path::PathBuf,
	sync::{
		atomic::{AtomicBool, AtomicU32, Ordering},
		Arc,
	},
	thread::{self, JoinHandle},
	time::Duration,
};

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Status {
	pub state: String,
	pub buffered_sec: f64,
	pub buffer_bytes: u64,
	pub fps_actual: f64,
	pub dropped_frames: u64,
	pub encoder: String,
	pub width: u32,
	pub height: u32,
	pub audio_tracks: Vec<String>,
	/// "Monitor" or the game exe being captured (Game window mode).
	pub capture_target: String,
	pub error: Option<String>,
}

pub struct SavedClip {
	pub path: PathBuf,
	pub size_bytes: u64,
	pub duration_sec: f64,
}

struct AppTrack {
	stop: Arc<AtomicBool>,
	handle: JoinHandle<()>,
	exe: String,
	/// Whether this capture also encodes a track of its own ("Own track" rule).
	own_track: bool,
}

struct Pipeline {
	stop: Arc<AtomicBool>,
	threads: Vec<JoinHandle<()>>,
	buffer: Arc<Mutex<ReplayBuffer>>,
	stats: Arc<CaptureStats>,
	app_tracks: Arc<Mutex<HashMap<u32, AppTrack>>>,
	encoder: String,
	width: u32,
	height: u32,
	fps: u32,
	started_us: i64,
}

pub struct Recorder {
	pipeline: Mutex<Option<Pipeline>>,
	pub status: RwLock<Status>,
	saving: AtomicBool,
}

impl Recorder {
	pub fn new() -> Self {
		Self { pipeline: Mutex::new(None), status: RwLock::new(Status { state: "idle".into(), ..Default::default() }), saving: AtomicBool::new(false) }
	}

	pub fn is_running(&self) -> bool {
		self.pipeline.lock().is_some()
	}

	pub fn tracked_pids(&self) -> HashSet<u32> {
		self.pipeline.lock().as_ref().map(|p| p.app_tracks.lock().keys().copied().collect()).unwrap_or_default()
	}

	pub fn set_error(&self, e: &anyhow::Error) {
		let mut s = self.status.write();
		s.state = "error".into();
		s.error = Some(format!("{e:#}"));
	}

	pub fn start(&self, settings: &Arc<RwLock<Settings>>, encoders: &[EncoderInfo]) -> Result<()> {
		let mut slot = self.pipeline.lock();
		if slot.is_some() {
			return Ok(());
		}
		self.status.write().state = "starting".into();
		let s = settings.read().clone();

		let mon = monitors::resolve(s.video.monitor).ok_or_else(|| anyhow!("no monitor found"))?;
		let (w, h) = s.video.resolution.output_size(mon.width, mon.height);

		let mut vs = s.video.clone();
		if !encoders.iter().any(|e| e.id == vs.encoder) {
			vs.encoder = crate::encode::probe::default_encoder(encoders).map(|e| e.id.clone()).ok_or_else(|| anyhow!("no working video encoder"))?;
		}
		// GPU encoders that accept D3D11 textures run zero-copy inside the capture thread (opened there,
		// on the capture device); everything else gets NV12 read back to the CPU and its own thread.
		let zero_copy = video::zero_copy(&vs);
		let cpu_encoder = if zero_copy { None } else { Some(video::open(&vs, w, h).with_context(|| format!("opening {}", vs.encoder))?) };
		let encoder_label = if zero_copy { format!("{} · zero-copy", vs.encoder) } else { vs.encoder.clone() };

		let disk = (s.replay.buffer_mode == "disk").then(|| PathBuf::from(&s.replay.cache_dir).join(format!("session-{}", std::process::id())));
		let buffer = Arc::new(Mutex::new(ReplayBuffer::new(s.replay.length_sec, disk)?));
		if let Some((encoder, _)) = &cpu_encoder {
			buffer.lock().add_stream(0, StreamInfo { kind: StreamKind::Video, title: "Video".into(), skip_if_silent: false, params: Arc::new(encoder.params()) });
		}

		let stop = Arc::new(AtomicBool::new(false));
		let stats = Arc::new(CaptureStats::default());
		let t0 = clock::now_us() + 100_000;
		let mut threads = Vec::new();

		// Capture → encoder. A bounded queue of ~half a second absorbs encoder hiccups.
		let (frame_tx, frame_rx) = crossbeam_channel::bounded::<RawFrame>((vs.fps / 2).max(4) as usize);
		let (recycle_tx, recycle_rx) = crossbeam_channel::bounded::<Vec<u8>>(8);
		let (ready_tx, ready_rx) = crossbeam_channel::bounded(1);
		let cap_cfg = CaptureConfig { hmon: mon.handle, api: vs.capture_api.clone(), cursor: vs.capture_cursor, fps: vs.fps, out_w: w, out_h: h, full_range: vs.color_range == "full", t0_us: t0, gpu_encode: zero_copy.then(|| capture::GpuEncode { video: vs.clone(), buffer: buffer.clone(), stream: 0 }), boost: vs.priority == "recording" };
		unsafe {
			use windows::Win32::System::Threading::{GetCurrentProcess, SetPriorityClass, ABOVE_NORMAL_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS};
			let _ = SetPriorityClass(GetCurrentProcess(), if vs.priority == "recording" { ABOVE_NORMAL_PRIORITY_CLASS } else { NORMAL_PRIORITY_CLASS });
		}
		capture::set_gpu_scheduling(vs.priority == "recording");
		{
			let (stop, stats) = (stop.clone(), stats.clone());
			threads.push(thread::Builder::new().name("capture".into()).spawn(move || capture::run(cap_cfg, frame_tx, recycle_rx, stop, stats, ready_tx))?);
		}
		match ready_rx.recv_timeout(Duration::from_secs(5)) {
			Ok(Ok(())) => {}
			Ok(Err(e)) => {
				stop.store(true, Ordering::Relaxed);
				return Err(e.context("starting screen capture"));
			}
			Err(_) => {
				stop.store(true, Ordering::Relaxed);
				return Err(anyhow!("screen capture did not start"));
			}
		}

		if let Some((encoder, pix_fmt)) = cpu_encoder {
			let vt = video::VideoThread { encoder, pix_fmt, width: w, height: h, t0_us: t0, stream: 0, buffer: buffer.clone() };
			threads.push(thread::Builder::new().name("video-encode".into()).spawn(move || vt.run(frame_rx, recycle_tx))?);
		}

		// Audio tracks.
		let a = &s.audio;
		let rate = a.effective_rate();
		let next_id = Arc::new(AtomicU32::new(1));
		// The mic is always mixed into the System track (players only play the first track); "own track" adds a separate copy for editing.
		let mic_mix: Option<MixQueue> = (a.mic.enabled && a.system_track).then(|| Arc::new(Mutex::new(Default::default())));

		// The mix track. "apps": built by a mixer from one channel per app (each at its own volume) plus
		// the mic. "loopback": one Windows loopback of everything except us, with the mic added in.
		// Spawned before add_track, which holds `threads` for the rest of the setup.
		let mix_inputs: Option<MixInputs> = (a.system_track && a.mix_mode == "apps").then(MixInputs::default);
		if let Some(inputs) = &mix_inputs {
			let id = next_id.fetch_add(1, Ordering::Relaxed);
			let track = AudioTrack::new(&a.codec, rate, a.bitrate_kbps, t0, id, buffer.clone())?;
			buffer.lock().add_stream(id, StreamInfo { kind: StreamKind::Audio, title: "Mix".into(), skip_if_silent: false, params: Arc::new(track.params()) });
			let (inputs, mic, stop) = (inputs.clone(), mic_mix.clone(), stop.clone());
			threads.push(thread::Builder::new().name("audio-mixer".into()).spawn(move || audio::run_mixer(track, inputs, mic, rate, stop))?);
		}

		let mut add_track = |title: &str, kind: SourceKind, volume: f32, gate: i32, mix_in: Option<MixQueue>, mix_out: Option<MixQueue>, own: bool, start: i64| -> Result<()> {
			let track = if own {
				let id = next_id.fetch_add(1, Ordering::Relaxed);
				let t = AudioTrack::new(&a.codec, rate, a.bitrate_kbps, start, id, buffer.clone())?;
				buffer.lock().add_stream(id, StreamInfo { kind: StreamKind::Audio, title: title.into(), skip_if_silent: false, params: Arc::new(t.params()) });
				Some(t)
			} else {
				None
			};
			let denoise = if matches!(kind, SourceKind::Mic { .. }) { a.mic.denoise() } else { None };
			let cfg = SourceConfig { kind, rate, volume, gate_db: gate, denoise, live_volume: None, mix_in, mix_out };
			let stop = stop.clone();
			threads.push(thread::Builder::new().name(format!("audio-{title}")).spawn(move || audio::run_source(cfg, track, stop))?);
			Ok(())
		};

		if mix_inputs.is_none() && a.system_track {
			add_track("System", SourceKind::System, 1.0, -90, mic_mix.clone(), None, true, t0)?;
		}
		if a.mic.enabled {
			let kind = SourceKind::Mic { device: a.mic.device_id.clone() };
			add_track("Microphone", kind, a.mic.volume as f32 / 100.0, a.mic.gate_db, None, mic_mix.clone(), a.mic.own_track || !a.system_track, t0)?;
		}

		// Per-app tracks come and go with the session watcher.
		let app_tracks: Arc<Mutex<HashMap<u32, AppTrack>>> = Arc::default();
		{
			let (stop, settings, buffer, app_tracks, next_id) = (stop.clone(), settings.clone(), buffer.clone(), app_tracks.clone(), next_id.clone());
			threads.push(thread::Builder::new().name("session-watcher".into()).spawn(move || watch_sessions(stop, settings, buffer, app_tracks, next_id, mix_inputs))?);
		}

		{
			let mut st = self.status.write();
			*st = Status { state: "buffering".into(), encoder: encoder_label.clone(), width: w, height: h, ..Default::default() };
		}
		*slot = Some(Pipeline { stop, threads, buffer, stats, app_tracks, encoder: encoder_label, width: w, height: h, fps: vs.fps, started_us: t0 });
		Ok(())
	}

	pub fn stop(&self) {
		let Some(p) = self.pipeline.lock().take() else { return };
		p.stop.store(true, Ordering::Relaxed);
		for (_, t) in p.app_tracks.lock().drain() {
			t.stop.store(true, Ordering::Relaxed);
			let _ = t.handle.join();
		}
		for t in p.threads {
			let _ = t.join();
		}
		*self.status.write() = Status { state: "idle".into(), ..Default::default() };
	}

	/// Refreshes the live numbers; called by the status ticker.
	pub fn refresh_status(&self) -> Status {
		let guard = self.pipeline.lock();
		let mut st = self.status.write();
		if let Some(p) = guard.as_ref() {
			let b = p.buffer.lock();
			st.buffered_sec = b.buffered_sec();
			st.buffer_bytes = b.bytes();
			st.audio_tracks = b.audio_titles();
			if let Some(e) = &b.last_error {
				st.error = Some(e.clone());
			}
			let elapsed = ((clock::now_us() - p.started_us) as f64 / 1e6).max(0.001);
			st.fps_actual = (p.stats.frames.load(Ordering::Relaxed) as f64 / elapsed).min(p.fps as f64);
			st.dropped_frames = p.stats.dropped.load(Ordering::Relaxed);
			st.encoder = p.encoder.clone();
			st.capture_target = p.stats.target.lock().clone();
			st.width = p.width;
			st.height = p.height;
			if self.saving.load(Ordering::Relaxed) {
				st.state = "saving".into();
			} else if st.state == "saving" {
				st.state = "buffering".into();
			}
		}
		st.clone()
	}

	/// Writes the current buffer to a clip. Blocks the caller (run it on its own thread).
	pub fn save(&self, settings: &Settings) -> Result<SavedClip> {
		if self.saving.swap(true, Ordering::SeqCst) {
			return Err(anyhow!("a clip is already being saved"));
		}
		let result = (|| {
			let (buffer, w, h, fps) = {
				let g = self.pipeline.lock();
				let p = g.as_ref().ok_or_else(|| anyhow!("the replay buffer is not running"))?;
				(p.buffer.clone(), p.width, p.height, p.fps)
			};
			let snap = buffer.lock().snapshot()?.ok_or_else(|| anyhow!("nothing buffered yet"))?;

			let dir = PathBuf::from(&settings.output.clips_dir);
			std::fs::create_dir_all(&dir)?;
			let path = clips::unique_path(&dir, &clips::render_name(&settings.output.filename_template, &sessions::foreground_app(), w, h, fps), &settings.output.container);

			let mut mux = Muxer::create(&path)?;
			let present: HashSet<u32> = snap.packets.iter().map(|p| p.stream).collect();
			let audible: HashSet<u32> = snap.packets.iter().filter(|p| p.loud).map(|p| p.stream).collect();
			// App tracks are captured even while the app is quiet (so the mix never clips its first
			// sound); a track whose app stayed silent for the whole clip is left out.
			let used: HashSet<u32> = present.into_iter().filter(|id| snap.streams.get(id).is_some_and(|s| !s.skip_if_silent || audible.contains(id))).collect();
			let mut map = HashMap::new();
			let mut first_audio = true;
			for (id, info) in snap.streams.iter().filter(|(id, _)| used.contains(id)) {
				let default = info.kind == StreamKind::Video || std::mem::take(&mut first_audio);
				map.insert(*id, mux.add_stream(&info.params, &info.title, default)?);
			}
			mux.write_header()?;
			for p in &snap.packets {
				if let Some(&si) = map.get(&p.stream) {
					mux.write(si, &p.data, p.pts_us - snap.start_us, p.dts_us - snap.start_us, p.dur_us, p.key)?;
				}
			}
			mux.finish()?;

			let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
			Ok(SavedClip { path, size_bytes, duration_sec: (snap.end_us - snap.start_us) as f64 / 1e6 })
		})();
		self.saving.store(false, Ordering::SeqCst);
		result
	}
}

/// Keeps one capture per app. In "apps" mix mode every app with an audio session gets a channel into
/// the mixer (captured while still quiet, so nothing is clipped when it starts playing); apps set to
/// "Own track" also get a track of their own. In "loopback" mode only own-track apps are captured, once
/// they make sound. Volumes are read live, so the sliders apply without a restart.
fn watch_sessions(stop: Arc<AtomicBool>, settings: Arc<RwLock<Settings>>, buffer: Arc<Mutex<ReplayBuffer>>, tracks: Arc<Mutex<HashMap<u32, AppTrack>>>, next_id: Arc<AtomicU32>, mix: Option<MixInputs>) {
	audio::com_init();
	while !stop.load(Ordering::Relaxed) {
		let s = settings.read().audio.clone();
		let wants_track = |exe: &str| s.separate_apps && s.rule_for(exe) == "separate";

		// Stop captures whose process died or whose track rule changed (they restart with the new rule).
		tracks.lock().retain(|pid, t| {
			let keep = sessions::is_alive(*pid) && (mix.is_some() || wants_track(&t.exe)) && t.own_track == wants_track(&t.exe);
			if !keep {
				t.stop.store(true, Ordering::Relaxed);
				if let Some(m) = &mix {
					m.lock().retain(|(p, _)| p != pid);
				}
			}
			keep
		});

		let tracked: HashSet<u32> = tracks.lock().keys().copied().collect();
		let apps = if mix.is_some() { sessions::list_all(&tracked) } else { sessions::list(&tracked) };
		// One capture per process tree; an exe with several (VLC instances) shares one row, volume and rule.
		for (app, root, nth) in apps.iter().flat_map(|a| a.roots.iter().enumerate().map(move |(i, r)| (a, *r, i))).filter(|(_, r, _)| !tracked.contains(r)) {
			let own = wants_track(&app.exe);
			if mix.is_none() && !(own && app.peak > sessions::SOUND_THRESHOLD) {
				continue;
			}
			let rate = s.effective_rate();
			let track = if own {
				let id = next_id.fetch_add(1, Ordering::Relaxed);
				match AudioTrack::new(&s.codec, rate, s.bitrate_kbps, clock::now_us(), id, buffer.clone()) {
					Ok(t) => {
						let title = if nth == 0 { app.name.clone() } else { format!("{} {}", app.name, nth + 1) };
						buffer.lock().add_stream(id, StreamInfo { kind: StreamKind::Audio, title, skip_if_silent: true, params: Arc::new(t.params()) });
						Some(t)
					}
					Err(e) => {
						log::warn!("app track for {}: {e}", app.exe);
						continue;
					}
				}
			} else {
				None
			};
			let mix_out = mix.as_ref().map(|m| {
				let q = MixQueue::default();
				m.lock().push((root, q.clone()));
				q
			});
			let live_volume: Arc<dyn Fn() -> f32 + Send + Sync> = {
				let (settings, exe) = (settings.clone(), app.exe.clone());
				Arc::new(move || settings.read().audio.app_volume(&exe))
			};
			// Each capture has its own flag; the pipeline's stop sets them all (below and in Recorder::stop).
			let tstop = Arc::new(AtomicBool::new(false));
			let cfg = SourceConfig { kind: SourceKind::App(root), rate, volume: 1.0, gate_db: -90, denoise: None, live_volume: Some(live_volume), mix_in: None, mix_out };
			let ts = tstop.clone();
			match thread::Builder::new().name(format!("audio-{}", app.exe)).spawn(move || audio::run_source(cfg, track, ts)) {
				Ok(handle) => {
					tracks.lock().insert(root, AppTrack { stop: tstop, handle, exe: app.exe.clone(), own_track: own });
				}
				Err(e) => log::warn!("spawn app capture: {e}"),
			}
		}

		for _ in 0..5 {
			if stop.load(Ordering::Relaxed) {
				break;
			}
			thread::sleep(Duration::from_millis(100));
		}
	}
	for (_, t) in tracks.lock().drain() {
		t.stop.store(true, Ordering::Relaxed);
		let _ = t.handle.join();
	}
}
