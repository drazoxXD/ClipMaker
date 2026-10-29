//! Screen capture: a frame source (WGC or DXGI duplication, never injected into a game) feeds a GPU
//! video processor that scales to the output size and converts BGRA→NV12 in one pass. A small ring of
//! staging textures reads frames back without stalling the GPU, and the capture thread paces it all
//! at the target frame rate, repeating the last frame when the screen is still (constant frame rate).

mod convert;
mod dxgi;
mod wgc;

use crate::{
	buffer::{ReplayBuffer, StreamInfo, StreamKind},
	clock,
	encode::video,
	ff::{Frame, HwFrames},
	settings::VideoSettings,
};
use parking_lot::Mutex;
use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL, THREAD_PRIORITY_HIGHEST};
use anyhow::{anyhow, Result};
use crossbeam_channel::{Receiver, Sender, TrySendError};
use std::sync::{
	atomic::{AtomicBool, AtomicU64, Ordering},
	Arc,
};
use windows::{
	core::Interface,
	Win32::{
		Foundation::{HMODULE, HWND},
		Graphics::{
			Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
			Direct3D11::{D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread, ID3D11Texture2D, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION},
			Dxgi::{CreateDXGIFactory1, IDXGIAdapter1, IDXGIDevice, IDXGIFactory1},
			Gdi::HMONITOR,
		},
	},
};

pub use convert::Converter;

/// A tightly packed NV12 frame: Y plane (w×h) then interleaved UV (w×h/2).
pub struct RawFrame {
	pub data: Vec<u8>,
	pub index: i64,
}

pub trait FrameSource {
	fn size(&self) -> (u32, u32);
	/// The newest captured texture, if a new one arrived since the last call.
	fn next(&mut self) -> Result<Option<ID3D11Texture2D>>;
}

pub struct Gpu {
	pub device: ID3D11Device,
	pub ctx: ID3D11DeviceContext,
}

/// Creates the D3D11 device on the adapter that drives `hmon`, so capture never crosses GPUs.
pub fn create_device(hmon: HMONITOR, boost: bool) -> Result<Gpu> {
	unsafe {
		let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
		let mut chosen: Option<IDXGIAdapter1> = None;
		let mut i = 0;
		while let Ok(adapter) = factory.EnumAdapters1(i) {
			let mut j = 0;
			while let Ok(out) = adapter.EnumOutputs(j) {
				if out.GetDesc().map(|d| d.Monitor == hmon).unwrap_or(false) {
					chosen = Some(adapter.clone());
				}
				j += 1;
			}
			if chosen.is_some() {
				break;
			}
			i += 1;
		}
		let adapter = chosen.ok_or_else(|| anyhow!("no GPU drives the selected monitor"))?;

		let mut device = None;
		let mut ctx = None;
		D3D11CreateDevice(&adapter, D3D_DRIVER_TYPE_UNKNOWN, HMODULE::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut ctx))?;
		let device = device.unwrap();
		let ctx = ctx.unwrap();
		if let Ok(mt) = ctx.cast::<ID3D11Multithread>() {
			let _ = mt.SetMultithreadProtected(true);
		}
		if boost {
			if let Ok(dxgi) = device.cast::<IDXGIDevice>() {
				let _ = dxgi.SetGPUThreadPriority(7);
			}
		}
		Ok(Gpu { device, ctx })
	}
}

pub struct CaptureConfig {
	pub hmon: isize,
	pub api: String,
	pub cursor: bool,
	pub fps: u32,
	pub out_w: u32,
	pub out_h: u32,
	pub full_range: bool,
	pub t0_us: i64,
	/// Set for GPU encoders that take D3D11 textures directly (see video::zero_copy).
	pub gpu_encode: Option<GpuEncode>,
	/// Boost GPU and thread priority (the "Recording first" setting).
	pub boost: bool,
}

#[derive(Default)]
pub struct CaptureStats {
	pub frames: AtomicU64,
	pub dropped: AtomicU64,
	/// What is being captured right now, for the Overview page ("Monitor" or "GTA5.exe").
	pub target: Mutex<String>,
}

/// Opens the capture source: a game window when one is given, otherwise the monitor.
fn open_source(cfg: &CaptureConfig, gpu: &Gpu, window: Option<isize>) -> Result<Box<dyn FrameSource>> {
	let hmon = HMONITOR(cfg.hmon as *mut _);
	if let Some(hwnd) = window {
		return Ok(Box::new(wgc::WgcSource::for_window(gpu, HWND(hwnd as *mut _), cfg.cursor, cfg.fps)?));
	}
	if cfg.api == "dxgi" {
		return Ok(Box::new(dxgi::DxgiSource::new(gpu, hmon)?));
	}
	match wgc::WgcSource::for_monitor(gpu, hmon, cfg.cursor, cfg.fps) {
		Ok(s) => Ok(Box::new(s)),
		Err(e) => {
			log::warn!("Graphics Capture failed ({e}), falling back to DXGI");
			Ok(Box::new(dxgi::DxgiSource::new(gpu, hmon)?))
		}
	}
}

/// The capture thread body. Runs until `stop` is set; frame `index` k belongs at `t0 + k/fps`.
pub fn run(cfg: CaptureConfig, tx: Sender<RawFrame>, pool: Receiver<Vec<u8>>, stop: Arc<AtomicBool>, stats: Arc<CaptureStats>, ready: Sender<Result<()>>) {
	unsafe {
		let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
	}

	// Capture must not starve while a game keeps the CPU busy; "Recording first" goes further.
	unsafe {
		let _ = SetThreadPriority(GetCurrentThread(), if cfg.boost { THREAD_PRIORITY_HIGHEST } else { THREAD_PRIORITY_ABOVE_NORMAL });
	}

	let setup = (|| -> Result<(Gpu, Box<dyn FrameSource>, Option<HwEncoder>)> {
		let gpu = create_device(HMONITOR(cfg.hmon as *mut _), cfg.boost)?;
		let src = open_source(&cfg, &gpu, None)?;
		let hw = cfg.gpu_encode.as_ref().map(|g| HwEncoder::open(&gpu, g, cfg.out_w, cfg.out_h, cfg.t0_us)).transpose()?;
		Ok((gpu, src, hw))
	})();
	let (gpu, mut source, mut hw_enc) = match setup {
		Ok(v) => {
			let _ = ready.send(Ok(()));
			v
		}
		Err(e) => {
			let _ = ready.send(Err(e));
			return;
		}
	};

	let mut converter: Option<Converter> = None;
	let interval = 1_000_000.0 / cfg.fps as f64;
	let frame_bytes = (cfg.out_w * cfg.out_h * 3 / 2) as usize;
	let mut tick: i64 = 0;
	let mut have_frame = false;
	// "auto": follow the fullscreen game in front (re-checked about once a second), else the monitor.
	let follow_game = cfg.api == "auto";
	let mut window: Option<isize> = None;
	*stats.target.lock() = "Monitor".into();

	while !stop.load(Ordering::Relaxed) {
		let deadline = cfg.t0_us + (tick as f64 * interval) as i64;
		clock::sleep_until(deadline);

		// Fell more than a frame behind (system stall): skip ahead instead of bursting.
		let now = clock::now_us();
		let due = ((now - cfg.t0_us) as f64 / interval) as i64;
		if due > tick + 1 {
			stats.dropped.fetch_add((due - tick - 1) as u64, Ordering::Relaxed);
			tick = due;
		}

		if follow_game && tick % cfg.fps.max(1) as i64 == 0 {
			let found = find_game_window(HMONITOR(cfg.hmon as *mut _));
			if found.as_ref().map(|g| g.0) != window {
				match open_source(&cfg, &gpu, found.as_ref().map(|g| g.0)) {
					Ok(s) => {
						source = s;
						window = found.as_ref().map(|g| g.0);
						*stats.target.lock() = found.map(|g| g.1).unwrap_or_else(|| "Monitor".into());
					}
					Err(e) => log::warn!("switching capture target: {e}"),
				}
			}
		}

		match source.next() {
			Ok(Some(tex)) => {
				let (sw, sh) = source.size();
				if converter.as_ref().is_none_or(|c| c.src_size() != (sw, sh)) {
					match Converter::new(&gpu, sw, sh, cfg.out_w, cfg.out_h, cfg.fps, cfg.full_range) {
						Ok(c) => converter = Some(c),
						Err(e) => {
							log::error!("video processor: {e}");
							break;
						}
					}
				}
				converter.as_mut().unwrap().upload(&gpu, &tex);
				have_frame = true;
			}
			Ok(None) => {}
			Err(e) => {
				log::warn!("capture source error: {e}; reopening");
				std::thread::sleep(std::time::Duration::from_millis(250));
				// A captured game window that closed lands here: go back to the monitor.
				if let Ok(s) = open_source(&cfg, &gpu, None) {
					source = s;
					window = None;
					*stats.target.lock() = "Monitor".into();
				}
			}
		}

		if have_frame && converter.as_mut().unwrap().blt() {
			let conv = converter.as_mut().unwrap();
			if let Some(enc) = hw_enc.as_mut() {
				// Zero-copy: the NV12 texture goes straight to the GPU encoder.
				let counter = if enc.submit(&gpu, conv.output(), tick) { &stats.frames } else { &stats.dropped };
				counter.fetch_add(1, Ordering::Relaxed);
			} else {
			let mut buf = pool.try_recv().unwrap_or_default();
			buf.resize(frame_bytes, 0);
			if let Some(index) = conv.read(&gpu, tick, &mut buf) {
				match tx.try_send(RawFrame { data: buf, index }) {
					Ok(()) => {
						stats.frames.fetch_add(1, Ordering::Relaxed);
					}
					Err(TrySendError::Full(_)) => {
						stats.dropped.fetch_add(1, Ordering::Relaxed);
					}
					Err(TrySendError::Disconnected(_)) => break,
				}
			}
			}
		}
		tick += 1;
	}
	if let Some(enc) = hw_enc.as_mut() {
		enc.finish();
	}
}

/// What the capture thread needs to run a GPU encoder itself (the zero-copy path).
pub struct GpuEncode {
	pub video: VideoSettings,
	pub buffer: Arc<Mutex<ReplayBuffer>>,
	pub stream: u32,
}

/// A GPU encoder on the capture device, fed D3D11 textures with no CPU round trip. The capture
/// thread only queues a GPU copy into a pooled encoder frame; a separate thread feeds AMF/NVENC, so
/// a submit that waits behind a GPU-bound game never costs the capture loop a frame.
struct HwEncoder {
	frames: HwFrames,
	tx: Option<Sender<Frame>>,
	thread: Option<std::thread::JoinHandle<()>>,
}

impl HwEncoder {
	fn open(gpu: &Gpu, cfg: &GpuEncode, w: u32, h: u32, t0_us: i64) -> Result<Self> {
		// FFmpeg takes ownership of one reference to our device.
		let frames = unsafe { HwFrames::d3d11(gpu.device.clone().into_raw(), w, h)? };
		let mut encoder = video::open_hw(&cfg.video, w, h, &frames)?;
		cfg.buffer.lock().add_stream(cfg.stream, StreamInfo { kind: StreamKind::Video, title: "Video".into(), skip_if_silent: false, params: Arc::new(encoder.params()) });

		// Up to ~a quarter second of frames may wait for the encoder before new ones are dropped.
		let (tx, rx) = crossbeam_channel::bounded::<Frame>((cfg.video.fps / 4).max(4) as usize);
		let (buffer, stream) = (cfg.buffer.clone(), cfg.stream);
		let thread = std::thread::Builder::new().name("video-encode-gpu".into()).spawn(move || {
			unsafe {
				let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL);
			}
			let tb = encoder.time_base();
			for frame in rx.iter() {
				match encoder.encode(Some(&frame)) {
					Ok(pkts) => video::store_packets(&buffer, stream, t0_us, tb, pkts),
					Err(e) => log::warn!("gpu encode: {e}"),
				}
			}
			if let Ok(pkts) = encoder.encode(None) {
				video::store_packets(&buffer, stream, t0_us, tb, pkts);
			}
		})?;
		Ok(Self { frames, tx: Some(tx), thread: Some(thread) })
	}

	/// Queues frame `index`: a GPU-side copy into a pooled encoder texture, then a hand-off to the
	/// encoder thread. Returns false when the frame had to be dropped.
	fn submit(&mut self, gpu: &Gpu, tex: &ID3D11Texture2D, index: i64) -> bool {
		let Ok((mut frame, dst, slice)) = self.frames.get() else { return false };
		unsafe {
			let Some(dst) = ID3D11Texture2D::from_raw_borrowed(&dst) else { return false };
			gpu.ctx.CopySubresourceRegion(dst, slice, 0, 0, 0, tex, 0, None);
			// Push the copy to the GPU now rather than when the encoder next touches the context.
			gpu.ctx.Flush();
		}
		frame.set_pts(index);
		self.tx.as_ref().is_some_and(|tx| tx.try_send(frame).is_ok())
	}

	/// Lets the encoder drain what is queued, then waits for it.
	fn finish(&mut self) {
		self.tx.take();
		if let Some(t) = self.thread.take() {
			let _ = t.join();
		}
	}
}
#[link(name = "gdi32")]
unsafe extern "system" {
	fn D3DKMTSetProcessSchedulingPriorityClass(process: windows::Win32::Foundation::HANDLE, priority: i32) -> i32;
}

/// GPU scheduling priority for the whole process: what OBS does so a GPU-bound game can't starve
/// capture. REALTIME needs admin rights, so fall back to HIGH; "Game first" puts it back to NORMAL.
pub fn set_gpu_scheduling(boost: bool) {
	const NORMAL: i32 = 2;
	const HIGH: i32 = 4;
	const REALTIME: i32 = 5;
	unsafe {
		let me = windows::Win32::System::Threading::GetCurrentProcess();
		if !boost {
			D3DKMTSetProcessSchedulingPriorityClass(me, NORMAL);
			return;
		}
		for class in [REALTIME, HIGH] {
			if D3DKMTSetProcessSchedulingPriorityClass(me, class) == 0 {
				log::info!("GPU scheduling priority class {class}");
				return;
			}
		}
		log::warn!("could not raise GPU scheduling priority");
	}
}
/// The fullscreen (exclusive or borderless) window in front on `hmon`, with its exe name: what a
/// game looks like. Our own windows and the desktop/taskbar don't count.
fn find_game_window(hmon: HMONITOR) -> Option<(isize, String)> {
	use windows::Win32::{
		Foundation::RECT,
		Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONULL},
		UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible},
	};
	unsafe {
		let hwnd = GetForegroundWindow();
		if hwnd.0.is_null() || !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
			return None;
		}
		let mut pid = 0u32;
		GetWindowThreadProcessId(hwnd, Some(&mut pid));
		if pid == std::process::id() {
			return None;
		}
		let mut class = [0u16; 64];
		let n = GetClassNameW(hwnd, &mut class) as usize;
		let class = String::from_utf16_lossy(&class[..n]);
		if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
			return None;
		}
		if MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL) != hmon {
			return None;
		}
		let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
		let mut rect = RECT::default();
		if !GetMonitorInfoW(hmon, &mut info).as_bool() || GetWindowRect(hwnd, &mut rect).is_err() {
			return None;
		}
		let m = info.rcMonitor;
		let covers = rect.left <= m.left && rect.top <= m.top && rect.right >= m.right && rect.bottom >= m.bottom;
		if !covers {
			return None;
		}
		let exe = crate::audio::sessions::exe_name(pid).unwrap_or_else(|| format!("pid {pid}"));
		Some((hwnd.0 as isize, exe))
	}
}