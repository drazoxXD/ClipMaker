//! Finds which encoders actually work on this PC: hardware encoders are opened once with a small
//! test configuration, since an encoder being compiled into FFmpeg says nothing about the GPU.

use super::{family, Family};
use crate::ff::{self, Dict, Encoder, VideoEncoderConfig};
use ffmpeg_sys_next::AVPixelFormat;
use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncoderInfo {
	pub id: String,
	pub codec: String,
	pub device: String,
	pub vendor: String,
	pub label: String,
}

const CANDIDATES: &[(&str, &str, &str)] = &[
	("h264_amf", "h264", "AMD AMF"),
	("hevc_amf", "hevc", "AMD AMF"),
	("av1_amf", "av1", "AMD AMF"),
	("h264_nvenc", "h264", "NVIDIA NVENC"),
	("hevc_nvenc", "hevc", "NVIDIA NVENC"),
	("av1_nvenc", "av1", "NVIDIA NVENC"),
	("h264_qsv", "h264", "Intel Quick Sync"),
	("hevc_qsv", "hevc", "Intel Quick Sync"),
	("av1_qsv", "av1", "Intel Quick Sync"),
	("libx264", "h264", "x264"),
	("libx265", "hevc", "x265"),
	("libsvtav1", "av1", "SVT-AV1"),
];

fn works(id: &str) -> bool {
	let Some(codec) = ff::find_encoder(id) else { return false };
	let fam = family(id);
	if matches!(fam, Family::X264 | Family::X265 | Family::SvtAv1) {
		return true;
	}
	let fmts = ff::supported_pix_fmts(codec);
	let pix = if fmts.is_empty() || fmts.contains(&AVPixelFormat::AV_PIX_FMT_NV12) { AVPixelFormat::AV_PIX_FMT_NV12 } else { fmts[0] };
	Encoder::open_video(VideoEncoderConfig {
		codec_name: id,
		width: 1280,
		height: 720,
		fps: 30,
		pix_fmt: pix,
		bit_rate: 4_000_000,
		max_rate: 0,
		buf_size: 0,
		gop: 30,
		b_frames: 0,
		threads: 0,
		full_range: false,
		global_quality: None,
		options: Dict::new(),
		hw_frames: None,
	})
	.is_ok()
}

pub fn probe() -> Vec<EncoderInfo> {
	// Missing GPUs make FFmpeg print errors (no nvcuda.dll, no MFX session...); that's expected here.
	ff::set_log_level(ffmpeg_sys_next::AV_LOG_QUIET as i32);
	let list = CANDIDATES
		.iter()
		.filter(|(id, ..)| works(id))
		.map(|(id, codec, vendor)| {
			let gpu = !id.starts_with("lib");
			let codec_label = match *codec {
				"h264" => "H.264",
				"hevc" => "HEVC",
				_ => "AV1",
			};
			EncoderInfo { id: id.to_string(), codec: codec.to_string(), device: if gpu { "gpu" } else { "cpu" }.into(), vendor: vendor.to_string(), label: format!("{codec_label} · {vendor} ({id})") }
		})
		.collect();
	ff::set_log_level(ffmpeg_sys_next::AV_LOG_ERROR as i32);
	list
}

/// The encoder to use when the saved one is missing: a GPU H.264 first, then x264.
pub fn default_encoder(list: &[EncoderInfo]) -> Option<&EncoderInfo> {
	list.iter().find(|e| e.device == "gpu" && e.codec == "h264").or_else(|| list.iter().find(|e| e.id == "libx264")).or_else(|| list.first())
}
