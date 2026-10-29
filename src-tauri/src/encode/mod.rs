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
