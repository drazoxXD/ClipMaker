//! BGRA source → scaled NV12 on the GPU with ID3D11VideoProcessor, then a 3-deep staging ring for
//! readback. The output keeps the source aspect (the size comes from ResolutionSettings), so the
//! processor only scales: there is nothing to letterbox.

use super::Gpu;
use anyhow::Result;
use std::mem::ManuallyDrop;
use windows::{
	core::Interface,
	Win32::Graphics::{
		Direct3D11::*,
		Dxgi::Common::{DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709, DXGI_COLOR_SPACE_YCBCR_FULL_G22_LEFT_P709, DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_RATIONAL, DXGI_SAMPLE_DESC},
	},
};

/// Readback ring depth. Deeper means more slack before Map has to wait for a GPU-bound game.
const RING: usize = 4;

pub struct Converter {
	src_w: u32,
	src_h: u32,
	out_w: u32,
	out_h: u32,
	input: ID3D11Texture2D,
	output: ID3D11Texture2D,
	vctx: ID3D11VideoContext1,
	vp: ID3D11VideoProcessor,
	in_view: ID3D11VideoProcessorInputView,
	out_view: ID3D11VideoProcessorOutputView,
	staging: Vec<ID3D11Texture2D>,
	slot_index: [Option<i64>; RING],
	written: u64,
}

fn texture(dev: &ID3D11Device, w: u32, h: u32, format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT, usage: D3D11_USAGE, bind: u32, cpu: u32) -> Result<ID3D11Texture2D> {
	let desc = D3D11_TEXTURE2D_DESC {
		Width: w,
		Height: h,
		MipLevels: 1,
		ArraySize: 1,
		Format: format,
		SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
		Usage: usage,
		BindFlags: bind,
		CPUAccessFlags: cpu,
		MiscFlags: 0,
	};
	let mut tex = None;
	unsafe { dev.CreateTexture2D(&desc, None, Some(&mut tex))? };
	Ok(tex.unwrap())
}

impl Converter {
	pub fn new(gpu: &Gpu, src_w: u32, src_h: u32, out_w: u32, out_h: u32, fps: u32, full_range: bool) -> Result<Self> {
		unsafe {
			let vdev: ID3D11VideoDevice = gpu.device.cast()?;
			let vctx: ID3D11VideoContext1 = gpu.ctx.cast()?;
			let rate = DXGI_RATIONAL { Numerator: fps, Denominator: 1 };
			let content = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
				InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
				InputFrameRate: rate,
				InputWidth: src_w,
				InputHeight: src_h,
				OutputFrameRate: rate,
				OutputWidth: out_w,
				OutputHeight: out_h,
				Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
			};
			let en = vdev.CreateVideoProcessorEnumerator(&content)?;
			let vp = vdev.CreateVideoProcessor(&en, 0)?;

			let input = texture(&gpu.device, src_w, src_h, DXGI_FORMAT_B8G8R8A8_UNORM, D3D11_USAGE_DEFAULT, (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32, 0)?;
			let output = texture(&gpu.device, out_w, out_h, DXGI_FORMAT_NV12, D3D11_USAGE_DEFAULT, D3D11_BIND_RENDER_TARGET.0 as u32, 0)?;

			let in_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
				FourCC: 0,
				ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
				Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: 0 } },
			};
			let mut in_view = None;
			vdev.CreateVideoProcessorInputView(&input, &en, &in_desc, Some(&mut in_view))?;

			let out_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
				ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
				Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 } },
			};
			let mut out_view = None;
			vdev.CreateVideoProcessorOutputView(&output, &en, &out_desc, Some(&mut out_view))?;

			vctx.VideoProcessorSetStreamColorSpace1(&vp, 0, DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709);
			vctx.VideoProcessorSetOutputColorSpace1(&vp, if full_range { DXGI_COLOR_SPACE_YCBCR_FULL_G22_LEFT_P709 } else { DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709 });
			vctx.VideoProcessorSetStreamAutoProcessingMode(&vp, 0, false);
			vctx.VideoProcessorSetStreamFrameFormat(&vp, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);

			let staging = (0..RING).map(|_| texture(&gpu.device, out_w, out_h, DXGI_FORMAT_NV12, D3D11_USAGE_STAGING, 0, D3D11_CPU_ACCESS_READ.0 as u32)).collect::<Result<Vec<_>>>()?;

			Ok(Self { src_w, src_h, out_w, out_h, input, output, vctx, vp, in_view: in_view.unwrap(), out_view: out_view.unwrap(), staging, slot_index: [None; RING], written: 0 })
		}
	}

	pub fn src_size(&self) -> (u32, u32) {
		(self.src_w, self.src_h)
	}

	/// Copies the captured frame into our own input texture so the capture pool can recycle its surface.
	pub fn upload(&mut self, gpu: &Gpu, tex: &ID3D11Texture2D) {
		unsafe {
			let mut d = D3D11_TEXTURE2D_DESC::default();
			tex.GetDesc(&mut d);
			if d.Width == self.src_w && d.Height == self.src_h {
				gpu.ctx.CopyResource(&self.input, tex);
			} else {
				let b = D3D11_BOX { left: 0, top: 0, front: 0, right: d.Width.min(self.src_w), bottom: d.Height.min(self.src_h), back: 1 };
				gpu.ctx.CopySubresourceRegion(&self.input, 0, 0, 0, 0, tex, 0, Some(&b));
			}
		}
	}

	/// Scales and converts the current input into the NV12 output texture.
	pub fn blt(&mut self) -> bool {
		unsafe {
			let stream = D3D11_VIDEO_PROCESSOR_STREAM { Enable: true.into(), pInputSurface: ManuallyDrop::new(Some(self.in_view.clone())), ..Default::default() };
			let streams = [stream];
			let r = self.vctx.VideoProcessorBlt(&self.vp, &self.out_view, 0, &streams);
			let [mut stream] = streams;
			ManuallyDrop::drop(&mut stream.pInputSurface);
			if let Err(e) = r {
				log::warn!("VideoProcessorBlt: {e}");
				return false;
			}
			true
		}
	}

	/// The NV12 frame `blt` produced; the zero-copy path hands it straight to the GPU encoder.
	pub fn output(&self) -> &ID3D11Texture2D {
		&self.output
	}

	/// CPU-encoder path: queues the output for readback as frame `index` and returns the oldest
	/// frame in the ring (copied RING-1 ticks ago) once it is primed.
	pub fn read(&mut self, gpu: &Gpu, index: i64, out: &mut [u8]) -> Option<i64> {
		unsafe {
			let slot = (self.written % RING as u64) as usize;
			gpu.ctx.CopyResource(&self.staging[slot], &self.output);
			self.slot_index[slot] = Some(index);
			self.written += 1;

			// Read the oldest slot: copied several ticks ago, so even a busy GPU is done with it.
			let read = (self.written % RING as u64) as usize;
			let idx = self.slot_index[read]?;
			let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
			if gpu.ctx.Map(&self.staging[read], 0, D3D11_MAP_READ, 0, Some(&mut mapped)).is_err() {
				return None;
			}
			let (w, h) = (self.out_w as usize, self.out_h as usize);
			let pitch = mapped.RowPitch as usize;
			let src = mapped.pData as *const u8;
			for row in 0..h {
				std::ptr::copy_nonoverlapping(src.add(row * pitch), out.as_mut_ptr().add(row * w), w);
			}
			let uv_src = src.add(pitch * h);
			let uv_dst = out.as_mut_ptr().add(w * h);
			for row in 0..h / 2 {
				std::ptr::copy_nonoverlapping(uv_src.add(row * pitch), uv_dst.add(row * w), w);
			}
			gpu.ctx.Unmap(&self.staging[read], 0);
			self.slot_index[read] = None;
			Some(idx)
		}
	}
}
