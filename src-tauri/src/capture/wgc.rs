//! Windows Graphics Capture of a whole monitor: the same path OBS uses, works with fullscreen games
//! and needs no hooks. The yellow border is turned off where Windows allows it.

use super::{FrameSource, Gpu};
use anyhow::Result;
use windows::{
	core::{factory, Interface},
	Graphics::{
		Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession},
		DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
		SizeInt32,
	},
	Win32::{
		Foundation::HWND,
		Graphics::{Direct3D11::ID3D11Texture2D, Dxgi::IDXGIDevice, Gdi::HMONITOR},
		System::WinRT::{
			Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
			Graphics::Capture::IGraphicsCaptureItemInterop,
		},
	},
};

pub struct WgcSource {
	device: IDirect3DDevice,
	pool: Direct3D11CaptureFramePool,
	session: GraphicsCaptureSession,
	size: SizeInt32,
}

impl WgcSource {
	/// The whole monitor.
	pub fn for_monitor(gpu: &Gpu, hmon: HMONITOR, cursor: bool, fps: u32) -> Result<Self> {
		let item = unsafe { factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?.CreateForMonitor(hmon)? };
		Self::start(gpu, item, cursor, fps)
	}

	/// One window: a game's own frames, which keep coming even when a fullscreen game bypasses
	/// desktop composition and monitor capture starves (the way Game DVR records).
	pub fn for_window(gpu: &Gpu, hwnd: HWND, cursor: bool, fps: u32) -> Result<Self> {
		let item = unsafe { factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?.CreateForWindow(hwnd)? };
		Self::start(gpu, item, cursor, fps)
	}

	fn start(gpu: &Gpu, item: GraphicsCaptureItem, cursor: bool, fps: u32) -> Result<Self> {
		if !GraphicsCaptureSession::IsSupported()? {
			anyhow::bail!("Graphics Capture is not supported on this system");
		}
		unsafe {
			let dxgi: IDXGIDevice = gpu.device.cast()?;
			let device: IDirect3DDevice = CreateDirect3D11DeviceFromDXGIDevice(&dxgi)?.cast()?;
			let size = item.Size()?;
			// Three buffers so a frame isn't lost while we still hold the previous one.
			let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&device, DirectXPixelFormat::B8G8R8A8UIntNormalized, 3, size)?;
			let session = pool.CreateCaptureSession(&item)?;
			let _ = session.SetIsBorderRequired(false);
			let _ = session.SetIsCursorCaptureEnabled(cursor);
			// Without a limit Windows copies frames at the monitor's full refresh rate (e.g. 144 Hz) though we
			// only use `fps` of them. Allow twice our rate: exactly our rate makes the two clocks beat
			// against each other and every few ticks finds no new frame.
			let _ = session.SetMinUpdateInterval(windows::Foundation::TimeSpan { Duration: 10_000_000 / (fps.max(1) * 2) as i64 });
			session.StartCapture()?;
			Ok(Self { device, pool, session, size })
		}
	}
}

impl FrameSource for WgcSource {
	fn size(&self) -> (u32, u32) {
		(self.size.Width as u32, self.size.Height as u32)
	}

	fn next(&mut self) -> Result<Option<ID3D11Texture2D>> {
		// Drain to the newest frame; older ones are already stale.
		let mut latest = None;
		while let Ok(f) = self.pool.TryGetNextFrame() {
			latest = Some(f);
		}
		let Some(frame) = latest else { return Ok(None) };

		let content = frame.ContentSize()?;
		if content.Width != self.size.Width || content.Height != self.size.Height {
			// Resolution changed (e.g. a game switched modes): rebuild the pool at the new size.
			self.size = content;
			self.pool.Recreate(&self.device, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, content)?;
			return Ok(None);
		}

		let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
		let tex: ID3D11Texture2D = unsafe { access.GetInterface()? };
		Ok(Some(tex))
	}
}

impl Drop for WgcSource {
	fn drop(&mut self) {
		let _ = self.session.Close();
		let _ = self.pool.Close();
	}
}
