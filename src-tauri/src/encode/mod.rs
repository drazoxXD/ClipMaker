pub mod audio;
pub mod probe;
pub mod video;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
	X264,
	X265,
	SvtAv1,
	Amf,
	Nvenc,
	Qsv,
}

pub fn family(id: &str) -> Family {
	match id {
		"libx264" => Family::X264,
		"libx265" => Family::X265,
		"libsvtav1" => Family::SvtAv1,
		s if s.ends_with("_amf") => Family::Amf,
		s if s.ends_with("_nvenc") => Family::Nvenc,
		_ => Family::Qsv,
	}
}

/// Presets, tunes and profiles each family accepts (the same lists as the UI's encoderOptions.ts),
/// default first. Options left over from another encoder (AMF's "balanced" preset on NVENC) make the
/// encoder refuse to open, so `fit_options` swaps them for the family's defaults.
fn presets(fam: Family) -> &'static [&'static str] {
	match fam {
		Family::X264 | Family::X265 => &["veryfast", "ultrafast", "superfast", "faster", "fast", "medium", "slow", "slower", "veryslow"],
		Family::SvtAv1 => &["10", "13", "12", "11", "9", "8", "7", "6", "5", "4"],
		Family::Amf => &["balanced", "speed", "quality"],
		Family::Nvenc => &["p4", "p1", "p2", "p3", "p5", "p6", "p7"],
		Family::Qsv => &["medium", "veryfast", "faster", "fast", "slow", "slower", "veryslow"],
	}
}

fn tunes(fam: Family) -> &'static [&'static str] {
	match fam {
		Family::X264 => &["none", "film", "animation", "grain", "stillimage", "fastdecode", "zerolatency"],
		Family::X265 => &["none", "grain", "animation", "fastdecode", "zerolatency"],
		Family::Amf => &["transcoding", "ultralowlatency", "lowlatency", "webcam", "highquality"],
		Family::Nvenc => &["hq", "ll", "ull", "lossless"],
		Family::SvtAv1 | Family::Qsv => &["none"],
	}
}

fn profiles(codec: &str) -> &'static [&'static str] {
	match codec {
		"h264" => &["high", "baseline", "main"],
		"hevc" => &["main", "main10"],
		_ => &["main"],
	}
}

/// The codec an encoder id produces.
pub fn codec_of(id: &str) -> &'static str {
	match id {
		"libx264" => "h264",
		"libx265" => "hevc",
		"libsvtav1" => "av1",
		s if s.starts_with("h264") => "h264",
		s if s.starts_with("hevc") => "hevc",
		_ => "av1",
	}
}

/// Makes codec, preset, tune and profile valid for the selected encoder. Returns whether anything changed.
pub fn fit_options(v: &mut crate::settings::VideoSettings) -> bool {
	let fam = family(&v.encoder);
	let codec = codec_of(&v.encoder);
	let mut changed = false;
	let mut fit = |value: &mut String, allowed: &[&str]| {
		if !allowed.contains(&value.as_str()) {
			*value = allowed[0].to_string();
			changed = true;
		}
	};
	fit(&mut v.codec, &[codec]);
	fit(&mut v.preset, presets(fam));
	fit(&mut v.tune, tunes(fam));
	fit(&mut v.profile, profiles(codec));
	changed
}
