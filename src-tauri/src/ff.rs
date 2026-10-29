//! A thin safe layer over the FFmpeg C API (ffmpeg-sys-next): encoders, frames, a muxer and
//! first-frame thumbnails. Everything here owns its FFmpeg pointer and frees it on drop.

#![allow(clippy::missing_safety_doc)]

use anyhow::{anyhow, bail, Result};
use ffmpeg_sys_next as ff;
use std::{
	ffi::{c_void, CStr, CString},
	path::Path,
	ptr,
	sync::Arc,
};

pub const AVERROR_EAGAIN: i32 = -11;
pub const AVERROR_EOF: i32 = -((b'E' as i32) | ((b'O' as i32) << 8) | ((b'F' as i32) << 16) | ((b' ' as i32) << 24));
pub const US: ff::AVRational = ff::AVRational { num: 1, den: 1_000_000 };

pub fn err_str(code: i32) -> String {
	let mut buf = [0i8; 256];
	unsafe { ff::av_strerror(code, buf.as_mut_ptr(), buf.len()) };
	unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned()
}

fn check(code: i32, what: &str) -> Result<i32> {
	if code < 0 {
		bail!("{what}: {}", err_str(code));
	}
	Ok(code)
}

fn cstr(s: &str) -> CString {
	CString::new(s).unwrap_or_default()
}

pub fn set_log_level(level: i32) {
	unsafe { ff::av_log_set_level(level) };
}

// ---------------------------------------------------------------- dictionary

pub struct Dict(*mut ff::AVDictionary);

impl Dict {
	pub fn new() -> Self {
		Self(ptr::null_mut())
	}
	pub fn set(&mut self, k: &str, v: impl ToString) {
		let (k, v) = (cstr(k), cstr(&v.to_string()));
		unsafe { ff::av_dict_set(&mut self.0, k.as_ptr(), v.as_ptr(), 0) };
	}
}

impl Drop for Dict {
	fn drop(&mut self) {
		unsafe { ff::av_dict_free(&mut self.0) };
	}
}

// ---------------------------------------------------------------- codec parameters

/// Stream parameters (incl. SPS/PPS extradata) taken from an opened encoder; what the muxer needs.
pub struct CodecParams(*mut ff::AVCodecParameters);
unsafe impl Send for CodecParams {}
unsafe impl Sync for CodecParams {}

impl CodecParams {
	pub fn as_ptr(&self) -> *const ff::AVCodecParameters {
		self.0
	}
}

impl Drop for CodecParams {
	fn drop(&mut self) {
		unsafe { ff::avcodec_parameters_free(&mut self.0) };
	}
}

// ---------------------------------------------------------------- frames

pub struct Frame(pub *mut ff::AVFrame);
unsafe impl Send for Frame {}

impl Frame {
	pub fn video(fmt: ff::AVPixelFormat, w: u32, h: u32) -> Result<Self> {
		unsafe {
			let f = ff::av_frame_alloc();
			(*f).format = fmt as i32;
			(*f).width = w as i32;
			(*f).height = h as i32;
			check(ff::av_frame_get_buffer(f, 0), "av_frame_get_buffer")?;
			Ok(Self(f))
		}
	}

	pub fn audio(fmt: ff::AVSampleFormat, samples: usize, channels: i32, rate: u32) -> Result<Self> {
		unsafe {
			let f = ff::av_frame_alloc();
			(*f).format = fmt as i32;
			(*f).nb_samples = samples as i32;
			(*f).sample_rate = rate as i32;
			ff::av_channel_layout_default(&mut (*f).ch_layout, channels);
			check(ff::av_frame_get_buffer(f, 0), "av_frame_get_buffer")?;
			Ok(Self(f))
		}
	}

	pub fn plane(&mut self, i: usize) -> *mut u8 {
		unsafe { (*self.0).data[i] }
	}
	pub fn linesize(&self, i: usize) -> usize {
		unsafe { (*self.0).linesize[i] as usize }
	}
	pub fn set_pts(&mut self, pts: i64) {
		unsafe { (*self.0).pts = pts };
	}
}

impl Drop for Frame {
	fn drop(&mut self) {
		unsafe { ff::av_frame_free(&mut self.0) };
	}
}

/// Copies a tightly packed NV12 image (Y plane then interleaved UV) into an NV12 frame.
pub fn fill_nv12(frame: &mut Frame, src: &[u8], w: usize, h: usize) {
	unsafe {
		let (ly, luv) = (frame.linesize(0), frame.linesize(1));
		let (py, puv) = (frame.plane(0), frame.plane(1));
		for row in 0..h {
			ptr::copy_nonoverlapping(src.as_ptr().add(row * w), py.add(row * ly), w);
		}
		let uv = &src[w * h..];
		for row in 0..h / 2 {
			ptr::copy_nonoverlapping(uv.as_ptr().add(row * w), puv.add(row * luv), w);
		}
	}
}

// ---------------------------------------------------------------- scaler

/// NV12 to whatever the encoder wants (yuv420p for x265/SVT, 10-bit for main10).
pub struct Scaler(*mut ff::SwsContext);
unsafe impl Send for Scaler {}

impl Scaler {
	pub fn new(w: u32, h: u32, from: ff::AVPixelFormat, to: ff::AVPixelFormat) -> Result<Self> {
		let s = unsafe { ff::sws_getContext(w as i32, h as i32, from, w as i32, h as i32, to, ff::SwsFlags::SWS_POINT as i32, ptr::null_mut(), ptr::null_mut(), ptr::null()) };
		if s.is_null() {
			bail!("sws_getContext failed");
		}
		Ok(Self(s))
	}

	pub fn run(&mut self, src: &Frame, dst: &mut Frame) {
		unsafe {
			ff::sws_scale(self.0, (*src.0).data.as_ptr() as *const *const u8, (*src.0).linesize.as_ptr(), 0, (*src.0).height, (*dst.0).data.as_ptr(), (*dst.0).linesize.as_ptr());
		}
	}
}

impl Drop for Scaler {
	fn drop(&mut self) {
		unsafe { ff::sws_freeContext(self.0) };
	}
}

// ---------------------------------------------------------------- D3D11 hardware frames

/// D3D11 frames on *our* capture device, so a GPU encoder (AMF/NVENC) reads the scaled NV12 texture
/// directly. No CPU readback means no waiting behind a GPU-bound game: what Medal and ReLive do.
pub struct HwFrames {
	device: *mut ff::AVBufferRef,
	frames: *mut ff::AVBufferRef,
}
unsafe impl Send for HwFrames {}

impl HwFrames {
	/// `device` is an owned (already AddRef'd) ID3D11Device pointer; FFmpeg releases it on free.
	pub unsafe fn d3d11(device: *mut c_void, w: u32, h: u32) -> Result<Self> {
		let mut dev = ff::av_hwdevice_ctx_alloc(ff::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA);
		if dev.is_null() {
			bail!("av_hwdevice_ctx_alloc(D3D11VA) failed");
		}
		let hw = (*dev).data as *mut ff::AVHWDeviceContext;
		(*((*hw).hwctx as *mut ff::AVD3D11VADeviceContext)).device = device as *mut ff::ID3D11Device;
		if let Err(e) = check(ff::av_hwdevice_ctx_init(dev), "av_hwdevice_ctx_init") {
			ff::av_buffer_unref(&mut dev);
			return Err(e);
		}
		let mut frames = ff::av_hwframe_ctx_alloc(dev);
		let f = (*frames).data as *mut ff::AVHWFramesContext;
		(*f).format = ff::AVPixelFormat::AV_PIX_FMT_D3D11;
		(*f).sw_format = ff::AVPixelFormat::AV_PIX_FMT_NV12;
		(*f).width = w as i32;
		(*f).height = h as i32;
		(*f).initial_pool_size = 0;
		// Render-target textures: individually allocated, and what AMF/NVENC accept as input.
		(*((*f).hwctx as *mut ff::AVD3D11VAFramesContext)).BindFlags = 0x20; // D3D11_BIND_RENDER_TARGET
		if let Err(e) = check(ff::av_hwframe_ctx_init(frames), "av_hwframe_ctx_init") {
			ff::av_buffer_unref(&mut frames);
			ff::av_buffer_unref(&mut dev);
			return Err(e);
		}
		Ok(Self { device: dev, frames })
	}

	/// A pooled encoder input frame plus the texture and array slice to copy the image into.
	pub fn get(&self) -> Result<(Frame, *mut c_void, u32)> {
		unsafe {
			let f = Frame(ff::av_frame_alloc());
			check(ff::av_hwframe_get_buffer(self.frames, f.0, 0), "av_hwframe_get_buffer")?;
			let tex = (*f.0).data[0] as *mut c_void;
			let index = (*f.0).data[1] as usize as u32;
			Ok((f, tex, index))
		}
	}
}

impl Drop for HwFrames {
	fn drop(&mut self) {
		unsafe {
			ff::av_buffer_unref(&mut self.frames);
			ff::av_buffer_unref(&mut self.device);
		}
	}
}

// ---------------------------------------------------------------- encoder

#[derive(Clone)]
pub struct EncodedPacket {
	pub data: Arc<[u8]>,
	pub pts: i64,
	pub dts: i64,
	pub duration: i64,
	pub key: bool,
}

pub struct Encoder {
	ctx: *mut ff::AVCodecContext,
	pkt: *mut ff::AVPacket,
}
unsafe impl Send for Encoder {}

pub struct VideoEncoderConfig<'a> {
	pub codec_name: &'a str,
	pub width: u32,
	pub height: u32,
	pub fps: u32,
	pub pix_fmt: ff::AVPixelFormat,
	pub bit_rate: i64,
	pub max_rate: i64,
	pub buf_size: i64,
	pub gop: i32,
	pub b_frames: i32,
	pub threads: i32,
	pub full_range: bool,
	pub global_quality: Option<i32>,
	pub options: Dict,
	/// Encode straight from D3D11 textures (pix_fmt must then be AV_PIX_FMT_D3D11).
	pub hw_frames: Option<&'a HwFrames>,
}

pub struct AudioEncoderConfig<'a> {
	pub codec_name: &'a str,
	pub sample_rate: u32,
	pub channels: i32,
	pub bit_rate: i64,
}

pub fn find_encoder(name: &str) -> Option<*const ff::AVCodec> {
	let n = cstr(name);
	let c = unsafe { ff::avcodec_find_encoder_by_name(n.as_ptr()) };
	(!c.is_null()).then_some(c)
}

/// The formats an encoder accepts; empty means "anything".
pub fn supported_pix_fmts(codec: *const ff::AVCodec) -> Vec<ff::AVPixelFormat> {
	unsafe {
		let mut out: *const c_void = ptr::null();
		let mut n = 0i32;
		if ff::avcodec_get_supported_config(ptr::null(), codec, ff::AVCodecConfig::AV_CODEC_CONFIG_PIX_FORMAT, 0, &mut out, &mut n) < 0 || out.is_null() {
			return vec![];
		}
		std::slice::from_raw_parts(out as *const ff::AVPixelFormat, n as usize).to_vec()
	}
}

pub fn supported_sample_fmts(codec: *const ff::AVCodec) -> Vec<ff::AVSampleFormat> {
	unsafe {
		let mut out: *const c_void = ptr::null();
		let mut n = 0i32;
		if ff::avcodec_get_supported_config(ptr::null(), codec, ff::AVCodecConfig::AV_CODEC_CONFIG_SAMPLE_FORMAT, 0, &mut out, &mut n) < 0 || out.is_null() {
			return vec![];
		}
		std::slice::from_raw_parts(out as *const ff::AVSampleFormat, n as usize).to_vec()
	}
}

impl Encoder {
	pub fn open_video(mut cfg: VideoEncoderConfig) -> Result<Self> {
		let codec = find_encoder(cfg.codec_name).ok_or_else(|| anyhow!("encoder {} not in this FFmpeg build", cfg.codec_name))?;
		unsafe {
			let ctx = ff::avcodec_alloc_context3(codec);
			let c = &mut *ctx;
			c.width = cfg.width as i32;
			c.height = cfg.height as i32;
			c.pix_fmt = cfg.pix_fmt;
			c.time_base = ff::AVRational { num: 1, den: cfg.fps as i32 };
			c.framerate = ff::AVRational { num: cfg.fps as i32, den: 1 };
			c.gop_size = cfg.gop;
			c.max_b_frames = cfg.b_frames;
			c.thread_count = cfg.threads;
			c.flags |= ff::AV_CODEC_FLAG_GLOBAL_HEADER as i32;
			c.color_range = if cfg.full_range { ff::AVColorRange::AVCOL_RANGE_JPEG } else { ff::AVColorRange::AVCOL_RANGE_MPEG };
			c.colorspace = ff::AVColorSpace::AVCOL_SPC_BT709;
			c.color_primaries = ff::AVColorPrimaries::AVCOL_PRI_BT709;
			c.color_trc = ff::AVColorTransferCharacteristic::AVCOL_TRC_BT709;
			if cfg.bit_rate > 0 {
				c.bit_rate = cfg.bit_rate;
			}
			if cfg.max_rate > 0 {
				c.rc_max_rate = cfg.max_rate;
			}
			if cfg.buf_size > 0 {
				c.rc_buffer_size = cfg.buf_size as i32;
			}
			if let Some(hw) = cfg.hw_frames {
				c.hw_frames_ctx = ff::av_buffer_ref(hw.frames);
				c.hw_device_ctx = ff::av_buffer_ref(hw.device);
			}
			if let Some(q) = cfg.global_quality {
				c.flags |= ff::AV_CODEC_FLAG_QSCALE as i32;
				c.global_quality = q * ff::FF_QP2LAMBDA as i32;
			}
			Self::finish_open(ctx, codec, &mut cfg.options.0)
		}
	}

	pub fn open_audio(cfg: AudioEncoderConfig) -> Result<(Self, ff::AVSampleFormat)> {
		let codec = find_encoder(cfg.codec_name).ok_or_else(|| anyhow!("encoder {} not in this FFmpeg build", cfg.codec_name))?;
		let fmts = supported_sample_fmts(codec);
		let pick = [ff::AVSampleFormat::AV_SAMPLE_FMT_FLTP, ff::AVSampleFormat::AV_SAMPLE_FMT_FLT, ff::AVSampleFormat::AV_SAMPLE_FMT_S16].into_iter().find(|f| fmts.is_empty() || fmts.contains(f)).unwrap_or(ff::AVSampleFormat::AV_SAMPLE_FMT_FLTP);
		unsafe {
			let ctx = ff::avcodec_alloc_context3(codec);
			let c = &mut *ctx;
			c.sample_fmt = pick;
			c.sample_rate = cfg.sample_rate as i32;
			c.time_base = ff::AVRational { num: 1, den: cfg.sample_rate as i32 };
			c.bit_rate = cfg.bit_rate;
			c.flags |= ff::AV_CODEC_FLAG_GLOBAL_HEADER as i32;
			ff::av_channel_layout_default(&mut c.ch_layout, cfg.channels);
			let mut opts: *mut ff::AVDictionary = ptr::null_mut();
			let enc = Self::finish_open(ctx, codec, &mut opts)?;
			ff::av_dict_free(&mut opts);
			Ok((enc, pick))
		}
	}

	unsafe fn finish_open(mut ctx: *mut ff::AVCodecContext, codec: *const ff::AVCodec, opts: &mut *mut ff::AVDictionary) -> Result<Self> {
		let r = ff::avcodec_open2(ctx, codec, opts);
		if r < 0 {
			ff::avcodec_free_context(&mut ctx);
			bail!("avcodec_open2: {}", err_str(r));
		}
		Ok(Self { ctx, pkt: ff::av_packet_alloc() })
	}

	pub fn frame_size(&self) -> usize {
		let n = unsafe { (*self.ctx).frame_size };
		if n > 0 { n as usize } else { 1024 }
	}

	pub fn time_base(&self) -> ff::AVRational {
		unsafe { (*self.ctx).time_base }
	}

	pub fn params(&self) -> CodecParams {
		unsafe {
			let p = ff::avcodec_parameters_alloc();
			ff::avcodec_parameters_from_context(p, self.ctx);
			CodecParams(p)
		}
	}

	/// Sends one frame (or `None` to flush) and returns every packet that came out.
	pub fn encode(&mut self, frame: Option<&Frame>) -> Result<Vec<EncodedPacket>> {
		unsafe {
			let r = ff::avcodec_send_frame(self.ctx, frame.map_or(ptr::null(), |f| f.0));
			if r < 0 && r != AVERROR_EOF {
				bail!("avcodec_send_frame: {}", err_str(r));
			}
			let mut out = Vec::new();
			loop {
				let r = ff::avcodec_receive_packet(self.ctx, self.pkt);
				if r == AVERROR_EAGAIN || r == AVERROR_EOF {
					break;
				}
				check(r, "avcodec_receive_packet")?;
				let p = &*self.pkt;
				out.push(EncodedPacket {
					data: Arc::from(std::slice::from_raw_parts(p.data, p.size as usize)),
					pts: p.pts,
					dts: p.dts,
					duration: p.duration,
					key: p.flags & ff::AV_PKT_FLAG_KEY as i32 != 0,
				});
				ff::av_packet_unref(self.pkt);
			}
			Ok(out)
		}
	}
}

impl Drop for Encoder {
	fn drop(&mut self) {
		unsafe {
			ff::av_packet_free(&mut self.pkt);
			ff::avcodec_free_context(&mut self.ctx);
		}
	}
}

// ---------------------------------------------------------------- muxer

pub struct Muxer {
	oc: *mut ff::AVFormatContext,
	header_written: bool,
}

impl Muxer {
	pub fn create(path: &Path) -> Result<Self> {
		let p = cstr(&path.to_string_lossy());
		unsafe {
			let mut oc = ptr::null_mut();
			check(ff::avformat_alloc_output_context2(&mut oc, ptr::null(), ptr::null(), p.as_ptr()), "avformat_alloc_output_context2")?;
			if let Err(e) = check(ff::avio_open(&mut (*oc).pb, p.as_ptr(), ff::AVIO_FLAG_WRITE as i32), "avio_open") {
				ff::avformat_free_context(oc);
				return Err(e);
			}
			Ok(Self { oc, header_written: false })
		}
	}

	/// Adds a stream copying `params`; `title` shows up as the track name in editors and players.
	pub fn add_stream(&mut self, params: &CodecParams, title: &str, default: bool) -> Result<usize> {
		unsafe {
			let st = ff::avformat_new_stream(self.oc, ptr::null());
			if st.is_null() {
				bail!("avformat_new_stream failed");
			}
			check(ff::avcodec_parameters_copy((*st).codecpar, params.as_ptr()), "avcodec_parameters_copy")?;
			(*(*st).codecpar).codec_tag = 0;
			(*st).time_base = US;
			let (k1, k2, v) = (cstr("title"), cstr("handler_name"), cstr(title));
			ff::av_dict_set(&mut (*st).metadata, k1.as_ptr(), v.as_ptr(), 0);
			ff::av_dict_set(&mut (*st).metadata, k2.as_ptr(), v.as_ptr(), 0);
			if default {
				(*st).disposition |= ff::AV_DISPOSITION_DEFAULT as i32;
			}
			Ok((*st).index as usize)
		}
	}

	pub fn write_header(&mut self) -> Result<()> {
		let mut opts = Dict::new();
		opts.set("movflags", "+faststart");
		check(unsafe { ff::avformat_write_header(self.oc, &mut opts.0) }, "avformat_write_header")?;
		self.header_written = true;
		Ok(())
	}

	/// Writes one packet whose timestamps are in microseconds.
	pub fn write(&mut self, stream: usize, data: &[u8], pts_us: i64, dts_us: i64, dur_us: i64, key: bool) -> Result<()> {
		unsafe {
			let st = *(*self.oc).streams.add(stream);
			let tb = (*st).time_base;
			let pkt = ff::av_packet_alloc();
			check(ff::av_new_packet(pkt, data.len() as i32), "av_new_packet")?;
			ptr::copy_nonoverlapping(data.as_ptr(), (*pkt).data, data.len());
			(*pkt).pts = ff::av_rescale_q(pts_us, US, tb);
			(*pkt).dts = ff::av_rescale_q(dts_us, US, tb);
			(*pkt).duration = ff::av_rescale_q(dur_us, US, tb);
			(*pkt).stream_index = stream as i32;
			if key {
				(*pkt).flags |= ff::AV_PKT_FLAG_KEY as i32;
			}
			let r = ff::av_interleaved_write_frame(self.oc, pkt);
			let mut pkt = pkt;
			ff::av_packet_free(&mut pkt);
			check(r, "av_interleaved_write_frame").map(|_| ())
		}
	}

	pub fn finish(mut self) -> Result<()> {
		let r = unsafe { ff::av_write_trailer(self.oc) };
		self.close();
		check(r, "av_write_trailer").map(|_| ())
	}

	fn close(&mut self) {
		unsafe {
			if !self.oc.is_null() {
				ff::avio_closep(&mut (*self.oc).pb);
				ff::avformat_free_context(self.oc);
				self.oc = ptr::null_mut();
			}
		}
	}
}

impl Drop for Muxer {
	fn drop(&mut self) {
		if self.header_written && !self.oc.is_null() {
			unsafe { ff::av_write_trailer(self.oc) };
		}
		self.close();
	}
}

// ---------------------------------------------------------------- inspection

struct Input(*mut ff::AVFormatContext);

impl Input {
	fn open(path: &Path) -> Result<Self> {
		let p = cstr(&path.to_string_lossy());
		let mut ic = ptr::null_mut();
		unsafe {
			check(ff::avformat_open_input(&mut ic, p.as_ptr(), ptr::null(), ptr::null_mut()), "avformat_open_input")?;
			let me = Self(ic);
			check(ff::avformat_find_stream_info(ic, ptr::null_mut()), "avformat_find_stream_info")?;
			Ok(me)
		}
	}
}

impl Drop for Input {
	fn drop(&mut self) {
		unsafe { ff::avformat_close_input(&mut self.0) };
	}
}

pub fn duration_sec(path: &Path) -> f64 {
	Input::open(path).map(|i| unsafe { (*i.0).duration } as f64 / ff::AV_TIME_BASE as f64).unwrap_or(0.0).max(0.0)
}

/// Decodes the first video frame and writes it as a PNG `width` pixels wide.
pub fn thumbnail(path: &Path, out_png: &Path, width: u32) -> Result<()> {
	let input = Input::open(path)?;
	unsafe {
		let ic = input.0;
		let mut dec: *const ff::AVCodec = ptr::null();
		let si = ff::av_find_best_stream(ic, ff::AVMediaType::AVMEDIA_TYPE_VIDEO, -1, -1, &mut dec, 0);
		check(si, "av_find_best_stream")?;
		let mut dctx = ff::avcodec_alloc_context3(dec);
		let st = *(*ic).streams.add(si as usize);
		ff::avcodec_parameters_to_context(dctx, (*st).codecpar);
		let result = (|| -> Result<()> {
			check(ff::avcodec_open2(dctx, dec, ptr::null_mut()), "open decoder")?;
			let pkt = ff::av_packet_alloc();
			let frame = Frame(ff::av_frame_alloc());
			let mut got = false;
			let mut reads = 0;
			while !got && reads < 600 && ff::av_read_frame(ic, pkt) >= 0 {
				reads += 1;
				if (*pkt).stream_index == si {
					ff::avcodec_send_packet(dctx, pkt);
					got = ff::avcodec_receive_frame(dctx, frame.0) >= 0;
				}
				ff::av_packet_unref(pkt);
			}
			let mut pkt = pkt;
			ff::av_packet_free(&mut pkt);
			if !got {
				bail!("no frame decoded");
			}

			let (sw, sh) = ((*frame.0).width, (*frame.0).height);
			let h = ((width as i64 * sh as i64 / sw as i64) / 2 * 2).max(2) as u32;
			let sws = ff::sws_getContext(sw, sh, std::mem::transmute::<i32, ff::AVPixelFormat>((*frame.0).format), width as i32, h as i32, ff::AVPixelFormat::AV_PIX_FMT_RGB24, ff::SwsFlags::SWS_BILINEAR as i32, ptr::null_mut(), ptr::null_mut(), ptr::null());
			let mut rgb = Frame::video(ff::AVPixelFormat::AV_PIX_FMT_RGB24, width, h)?;
			ff::sws_scale(sws, (*frame.0).data.as_ptr() as *const *const u8, (*frame.0).linesize.as_ptr(), 0, sh, (*rgb.0).data.as_ptr(), (*rgb.0).linesize.as_ptr());
			ff::sws_freeContext(sws);

			let png = find_encoder("png").ok_or_else(|| anyhow!("png encoder missing"))?;
			let ectx = ff::avcodec_alloc_context3(png);
			(*ectx).width = width as i32;
			(*ectx).height = h as i32;
			(*ectx).pix_fmt = ff::AVPixelFormat::AV_PIX_FMT_RGB24;
			(*ectx).time_base = ff::AVRational { num: 1, den: 1 };
			let mut opts: *mut ff::AVDictionary = ptr::null_mut();
			let mut enc = Encoder::finish_open(ectx, png, &mut opts)?;
			rgb.set_pts(0);
			let mut pkts = enc.encode(Some(&rgb))?;
			pkts.extend(enc.encode(None)?);
			let first = pkts.first().ok_or_else(|| anyhow!("png encode produced nothing"))?;
			std::fs::write(out_png, &first.data[..])?;
			Ok(())
		})();
		ff::avcodec_free_context(&mut dctx);
		result
	}
}
