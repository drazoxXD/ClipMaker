//! One audio track: interleaved f32 stereo in, AAC/Opus packets out, timestamped on the master clock.

use crate::{
	buffer::{Packet, ReplayBuffer},
	ff::{CodecParams, Encoder, Frame, AudioEncoderConfig},
};
use anyhow::Result;
use ffmpeg_sys_next::AVSampleFormat;
use parking_lot::Mutex;
use std::sync::Arc;

pub const CHANNELS: usize = 2;
/// Sample magnitude below which a chunk counts as silent (about -60 dBFS).
const SILENCE: f32 = 0.001;

pub struct AudioTrack {
	enc: Encoder,
	fmt: AVSampleFormat,
	frame_size: usize,
	rate: u32,
	pending: Vec<f32>,
	samples_sent: i64,
	start_us: i64,
	stream: u32,
	buffer: Arc<Mutex<ReplayBuffer>>,
}

impl AudioTrack {
	pub fn new(codec: &str, rate: u32, bitrate_kbps: u32, start_us: i64, stream: u32, buffer: Arc<Mutex<ReplayBuffer>>) -> Result<Self> {
		let name = if codec == "opus" { "libopus" } else { "aac" };
		let (enc, fmt) = Encoder::open_audio(AudioEncoderConfig { codec_name: name, sample_rate: rate, channels: CHANNELS as i32, bit_rate: bitrate_kbps as i64 * 1000 })?;
		let frame_size = enc.frame_size();
		Ok(Self { enc, fmt, frame_size, rate, pending: Vec::new(), samples_sent: 0, start_us, stream, buffer })
	}

	pub fn params(&self) -> CodecParams {
		self.enc.params()
	}

	/// Microsecond timestamp of the next sample to be written.
	pub fn position_us(&self) -> i64 {
		self.start_us + ((self.samples_sent + (self.pending.len() / CHANNELS) as i64) * 1_000_000 / self.rate as i64)
	}

	pub fn push_silence(&mut self, frames: usize) {
		self.pending.resize(self.pending.len() + frames * CHANNELS, 0.0);
		self.drain();
	}

	pub fn push(&mut self, interleaved: &[f32]) {
		self.pending.extend_from_slice(interleaved);
		self.drain();
	}

	fn drain(&mut self) {
		let n = self.frame_size;
		while self.pending.len() >= n * CHANNELS {
			let chunk: Vec<f32> = self.pending.drain(..n * CHANNELS).collect();
			if let Err(e) = self.encode_chunk(&chunk) {
				log::warn!("audio encode: {e}");
			}
		}
	}

	fn encode_chunk(&mut self, chunk: &[f32]) -> Result<()> {
		let n = chunk.len() / CHANNELS;
		let mut f = Frame::audio(self.fmt, n, CHANNELS as i32, self.rate)?;
		unsafe {
			match self.fmt {
				AVSampleFormat::AV_SAMPLE_FMT_FLTP => {
					for c in 0..CHANNELS {
						let dst = f.plane(c) as *mut f32;
						for i in 0..n {
							*dst.add(i) = chunk[i * CHANNELS + c];
						}
					}
				}
				AVSampleFormat::AV_SAMPLE_FMT_FLT => {
					std::ptr::copy_nonoverlapping(chunk.as_ptr(), f.plane(0) as *mut f32, chunk.len());
				}
				_ => {
					let dst = f.plane(0) as *mut i16;
					for (i, s) in chunk.iter().enumerate() {
						*dst.add(i) = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
					}
				}
			}
		}
		f.set_pts(self.samples_sent);
		self.samples_sent += n as i64;
		let pkts = self.enc.encode(Some(&f))?;
		let loud = chunk.iter().any(|s| s.abs() > SILENCE);
		self.store(pkts, loud);
		Ok(())
	}

	fn store(&self, pkts: Vec<crate::ff::EncodedPacket>, loud: bool) {
		if pkts.is_empty() {
			return;
		}
		let tb = self.enc.time_base();
		let to_us = |t: i64| self.start_us + t * 1_000_000 * tb.num as i64 / tb.den as i64;
		let mut b = self.buffer.lock();
		for p in pkts {
			b.push(Packet { stream: self.stream, pts_us: to_us(p.pts), dts_us: to_us(p.dts), dur_us: p.duration * 1_000_000 * tb.num as i64 / tb.den as i64, key: true, loud, data: p.data });
		}
	}

	pub fn flush(&mut self) {
		if let Ok(pkts) = self.enc.encode(None) {
			self.store(pkts, false);
		}
	}
}
