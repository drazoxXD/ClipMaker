use std::{env, fs, path::PathBuf};

fn main() {
	// Copy the FFmpeg DLLs next to the built exe so `tauri dev` and `cargo run` find them.
	let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
	let bin = manifest.join("ffmpeg").join("bin");
	let out = PathBuf::from(env::var("OUT_DIR").unwrap());
	// OUT_DIR is target/<profile>/build/<crate>/out; the exe lives in target/<profile>.
	if let Some(profile_dir) = out.ancestors().nth(3) {
		if let Ok(entries) = fs::read_dir(&bin) {
			for e in entries.flatten() {
				let p = e.path();
				if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dll")) {
					let _ = fs::copy(&p, profile_dir.join(p.file_name().unwrap()));
				}
			}
		}
	}
	println!("cargo:rerun-if-changed=ffmpeg/bin");
	tauri_build::build()
}
