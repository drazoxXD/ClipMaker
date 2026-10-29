//! DXGI Desktop Duplication fallback. Note: it does not composite the mouse cursor.

use super::{FrameSource, Gpu};
use anyhow::{anyhow, Result};
use windows::{
	core::Interface,
	Win32::Graphics::{
		Direct3D11::ID3D11Texture2D,
		Dxgi::{IDXGIDevice, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO},
		Gdi::HMONITOR,
	},
};

pub struct DxgiSource {
	dup: IDXGIOutputDuplication,
	size: (u32, u32),
	holding: bool,
}

impl DxgiSource {
	pub fn new(gpu: &Gpu, hmon: HMONITOR) -> Result<Self> {
		unsafe {
			let dxgi: IDXGIDevice = gpu.device.cast()?;
			let adapter = dxgi.GetAdapter()?;
			let mut i = 0;
			while let Ok(out) = adapter.EnumOutputs(i) {
				let desc = out.GetDesc()?;
				if desc.Monitor == hmon {
					let out1: IDXGIOutput1 = out.cast()?;
					let dup = out1.DuplicateOutput(&gpu.device)?;
					let r = desc.DesktopCoordinates;
					return Ok(Self { dup, size: ((r.right - r.left) as u32, (r.bottom - r.top) as u32), holding: false });
				}
				i += 1;
			}
			Err(anyhow!("monitor not found on the capture adapter"))
		}
	}
}

impl FrameSource for DxgiSource {
	fn size(&self) -> (u32, u32) {
		self.size
	}

	fn next(&mut self) -> Result<Option<ID3D11Texture2D>> {
		unsafe {
			if self.holding {
				let _ = self.dup.ReleaseFrame();
				self.holding = false;
			}
			let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
			let mut res: Option<IDXGIResource> = None;
			match self.dup.AcquireNextFrame(0, &mut info, &mut res) {
				Ok(()) => {}
				Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(None),
				Err(e) if e.code() == DXGI_ERROR_ACCESS_LOST => return Err(anyhow!("duplication access lost")),
				Err(e) => return Err(e.into()),
			}
			self.holding = true;
			if info.LastPresentTime == 0 {
				return Ok(None);
			}
			Ok(Some(res.unwrap().cast()?))
		}
	}
}
