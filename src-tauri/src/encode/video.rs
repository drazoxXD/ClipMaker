//! Maps the video settings onto each encoder family's own option vocabulary, and runs the video
//! encoder thread: NV12 frames in, timestamped packets into the replay buffer.

use super::{family, Family};
use crate::{
	buffer::{Packet, ReplayBuffer},
	capture::RawFrame,
	ff::{self, Dict, Encoder, Frame, HwFrames, Scaler, VideoEncoderConfig},
	settings::VideoSettings,
};
use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use ffmpeg_sys_next::AVPixelFormat;
use parking_lot::Mutex;
use std::sync::Arc;

/// NV12 straight from the GPU whenever the encoder takes it; 10-bit formats for main10.
pub fn pick_pix_fmt(v: &VideoSettings) -> AVPixelFormat {
	let supported = ff::find_encoder(&v.encoder).map(ff::supported_pix_fmts).unwrap_or_default();
	let prefs: &[AVPixelFormat] = if v.profile == "main10" { &[AVPixelFormat::AV_PIX_FMT_P010LE, AVPixelFormat::AV_PIX_FMT_YUV420P10LE] } else { &[AVPixelFormat::AV_PIX_FMT_NV12, AVPixelFormat::AV_PIX_FMT_YUV420P] };
	prefs.iter().copied().find(|p| supported.is_empty() || supported.contains(p)).unwrap_or(if supported.is_empty() { AVPixelFormat::AV_PIX_FMT_NV12 } else { supported[0] })
}

pub fn build_config<'a>(v: &'a VideoSettings, w: u32, h: u32, pix_fmt: AVPixelFormat, b_frames: u32, hw: Option<&'a HwFrames>) -> VideoEncoderConfig<'a> {
	let fam = family(&v.encoder);
	let kbps = v.bitrate_kbps as i64 * 1000;
	let max = (v.max_bitrate_kbps.max(v.bitrate_kbps)) as i64 * 1000;
	let q = v.quality as i32;
	let mut o = Dict::new();
	let (mut bit_rate, mut max_rate, mut buf_size, mut global_quality) = (0i64, 0i64, 0i64, None);

	let cbr = |br: &mut i64, mr: &mut i64, bs: &mut i64| {
		*br = kbps;
		*mr = kbps;
		*bs = kbps;
	};
	let vbr = |br: &mut i64, mr: &mut i64, bs: &mut i64| {
		*br = kbps;
		*mr = max;
		*bs = max * 2;
	};

	match fam {
		Family::X264 | Family::X265 => {
			o.set("preset", &v.preset);
			if v.tune != "none" && !v.tune.is_empty() {
				o.set("tune", &v.tune);
			}
			o.set("profile", &v.profile);
			match v.rate_control.as_str() {
				"cbr" => {
					cbr(&mut bit_rate, &mut max_rate, &mut buf_size);
					if fam == Family::X264 {
						o.set("nal-hrd", "cbr");
					}
				}
				"vbr" => vbr(&mut bit_rate, &mut max_rate, &mut buf_size),
				"crf" => o.set("crf", q),
				_ => o.set("qp", q),
			}
		}
		Family::SvtAv1 => {
			o.set("preset", &v.preset);
			match v.rate_control.as_str() {
				"cbr" => {
					bit_rate = kbps;
					o.set("svtav1-params", "rc=2");
				}
				"vbr" => {
					bit_rate = kbps;
					max_rate = max;
				}
				_ => o.set("crf", q),
			}
		}
		Family::Amf => {
			o.set("quality", &v.preset);
			o.set("usage", &v.tune);
			// Variance-based adaptive quantization: more bits for detail, fewer for flat areas.
			o.set("vbaq", 1);
			if v.codec == "h264" {
				o.set("profile", if v.profile == "baseline" { "constrained_baseline" } else { v.profile.as_str() });
			}
			match v.rate_control.as_str() {
				"cbr" => {
					o.set("rc", "cbr");
					cbr(&mut bit_rate, &mut max_rate, &mut buf_size);
				}
				"vbr" => {
					o.set("rc", "vbr_peak");
					vbr(&mut bit_rate, &mut max_rate, &mut buf_size);
				}
				_ => {
					o.set("rc", "cqp");
					o.set("qp_i", q);
					o.set("qp_p", q);
					o.set("qp_b", q);
				}
			}
		}
		Family::Nvenc => {
			o.set("preset", &v.preset);
			// Spatial adaptive quantization, NVENC's counterpart of AMF's vbaq.
			o.set("spatial-aq", 1);
			o.set("tune", &v.tune);
			o.set("profile", &v.profile);
			match v.rate_control.as_str() {
				"cbr" => {
					o.set("rc", "cbr");
					cbr(&mut bit_rate, &mut max_rate, &mut buf_size);
				}
				"vbr" => {
					o.set("rc", "vbr");
					vbr(&mut bit_rate, &mut max_rate, &mut buf_size);
				}
				_ => {
					o.set("rc", "constqp");
					o.set("qp", q);
				}
			}
		}
		Family::Qsv => {
			o.set("preset", &v.preset);
			o.set("profile", &v.profile);
			match v.rate_control.as_str() {
				"cbr" => cbr(&mut bit_rate, &mut max_rate, &mut buf_size),
				"vbr" => vbr(&mut bit_rate, &mut max_rate, &mut buf_size),
				_ => global_quality = Some(q),
			}
		}
	}

	VideoEncoderConfig {
		codec_name: &v.encoder,
		width: w,
		height: h,
		fps: v.fps,
		pix_fmt,
		bit_rate,
		max_rate,
		buf_size,
		gop: ((v.keyframe_sec * v.fps as f64).round() as i32).max(1),
		b_frames: b_frames as i32,
		threads: if matches!(fam, Family::X264 | Family::X265 | Family::SvtAv1) { v.threads as i32 } else { 0 },
		full_range: v.color_range == "full",
		global_quality,
		options: o,
		hw_frames: hw,
	}
}

/// Opens the encoder; if the GPU refuses B-frames (common on AMD HEVC), retries without them.
pub fn open(v: &VideoSettings, w: u32, h: u32) -> Result<(Encoder, AVPixelFormat)> {
	let pix = pick_pix_fmt(v);
	match Encoder::open_video(build_config(v, w, h, pix, v.b_frames, None)) {
		Ok(e) => Ok((e, pix)),
		Err(e) if v.b_frames > 0 => {
			log::warn!("{}: {e}; retrying without B-frames", v.encoder);
			Ok((Encoder::open_video(build_config(v, w, h, pix, 0, None))?, pix))
		}
		Err(e) => Err(e),
	}
}

/// Whether this encoder can take the capture texture directly, with no CPU round trip.
pub fn zero_copy(v: &VideoSettings) -> bool {
	matches!(family(&v.encoder), Family::Amf | Family::Nvenc) && v.profile != "main10" && ff::find_encoder(&v.encoder).map(ff::supported_pix_fmts).is_some_and(|f| f.contains(&AVPixelFormat::AV_PIX_FMT_D3D11))
}

/// Opens a GPU encoder fed with D3D11 frames from hw (see zero_copy).
pub fn open_hw(v: &VideoSettings, w: u32, h: u32, hw: &HwFrames) -> Result<Encoder> {
	let pix = AVPixelFormat::AV_PIX_FMT_D3D11;
	match Encoder::open_video(build_config(v, w, h, pix, v.b_frames, Some(hw))) {
		Ok(e) => Ok(e),
		Err(e) if v.b_frames > 0 => {
			log::warn!("{}: {e}; retrying without B-frames", v.encoder);
			Encoder::open_video(build_config(v, w, h, pix, 0, Some(hw)))
		}
		Err(e) => Err(e),
	}
}

/// Puts encoder packets (timestamps in the encoder's time base, frame 0 at 	0_us) into the buffer.
pub fn store_packets(buffer: &Mutex<ReplayBuffer>, stream: u32, t0_us: i64, tb: ffmpeg_sys_next::AVRational, pkts: Vec<ff::EncodedPacket>) {
	if pkts.is_empty() {
		return;
	}
	let to_us = |t: i64| t0_us + t * 1_000_000 * tb.num as i64 / tb.den as i64;
	let frame_us = 1_000_000 * tb.num as i64 / tb.den as i64;
	let mut b = buffer.lock();
	for p in pkts {
		b.push(Packet { stream, pts_us: to_us(p.pts), dts_us: to_us(p.dts), dur_us: if p.duration > 0 { p.duration * frame_us } else { frame_us }, key: p.key, loud: true, data: p.data });
	}
}

pub struct VideoThread {
	pub encoder: Encoder,
	pub pix_fmt: AVPixelFormat,
	pub width: u32,
	pub height: u32,
	pub t0_us: i64,
	pub stream: u32,
	pub buffer: Arc<Mutex<ReplayBuffer>>,
}

impl VideoThread {
	pub fn run(mut self, rx: Receiver<RawFrame>, recycle: Sender<Vec<u8>>) {
		let tb = self.encoder.time_base();
		let mut scaler = (self.pix_fmt != AVPixelFormat::AV_PIX_FMT_NV12).then(|| Scaler::new(self.width, self.height, AVPixelFormat::AV_PIX_FMT_NV12, self.pix_fmt).ok()).flatten();

		let (stream, t0) = (self.stream, self.t0_us);
		let push = |buf: &Arc<Mutex<ReplayBuffer>>, pkts: Vec<ff::EncodedPacket>| store_packets(buf, stream, t0, tb, pkts);

		for raw in rx.iter() {
			let frame = (|| -> Result<Frame> {
				let mut nv12 = Frame::video(AVPixelFormat::AV_PIX_FMT_NV12, self.width, self.height)?;
				ff::fill_nv12(&mut nv12, &raw.data, self.width as usize, self.height as usize);
				Ok(match scaler.as_mut() {
					Some(s) => {
						let mut dst = Frame::video(self.pix_fmt, self.width, self.height)?;
						s.run(&nv12, &mut dst);
						dst
					}
					None => nv12,
				})
			})();
			let _ = recycle.try_send(raw.data);
			let Ok(mut frame) = frame else { continue };
			frame.set_pts(raw.index);
			match self.encoder.encode(Some(&frame)) {
				Ok(pkts) => push(&self.buffer, pkts),
				Err(e) => log::warn!("video encode: {e}"),
			}
		}
		if let Ok(pkts) = self.encoder.encode(None) {
			push(&self.buffer, pkts);
		}
	}
}
