//! Saved clips: naming, the recent list with cached PNG thumbnails, and file actions.

use crate::ff;
use base64::Engine;
use serde::Serialize;
use std::{
	collections::hash_map::DefaultHasher,
	fs,
	hash::{Hash, Hasher},
	path::{Path, PathBuf},
	time::UNIX_EPOCH,
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipInfo {
	pub path: String,
	pub name: String,
	pub size_bytes: u64,
	pub duration_sec: f64,
	pub created_at: u64,
	pub thumbnail: Option<String>,
}

const MAX_LISTED: usize = 60;

pub fn render_name(template: &str, app: &str, w: u32, h: u32, fps: u32) -> String {
	let now = chrono::Local::now();
	let name = template
		.replace("{app}", app)
		.replace("{game}", app)
		.replace("{date}", &now.format("%Y-%m-%d").to_string())
		.replace("{time}", &now.format("%H-%M-%S").to_string())
		.replace("{res}", &format!("{w}x{h}"))
		.replace("{fps}", &fps.to_string());
	let clean: String = name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect();
	let clean = clean.trim().trim_end_matches('.').to_string();
	if clean.is_empty() { "clip".into() } else { clean }
}

pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
	let mut p = dir.join(format!("{stem}.{ext}"));
	let mut n = 1;
	while p.exists() {
		p = dir.join(format!("{stem}_{n}.{ext}"));
		n += 1;
	}
	p
}

fn thumb_path(thumbs: &Path, clip: &Path) -> PathBuf {
	let mut h = DefaultHasher::new();
	clip.hash(&mut h);
	if let Ok(m) = fs::metadata(clip) {
		m.len().hash(&mut h);
	}
	thumbs.join(format!("{:016x}.png", h.finish()))
}

pub fn thumbnail_data_url(thumbs: &Path, clip: &Path) -> Option<String> {
	let tp = thumb_path(thumbs, clip);
	if !tp.exists() {
		fs::create_dir_all(thumbs).ok()?;
		if let Err(e) = ff::thumbnail(clip, &tp, 384) {
			log::debug!("thumbnail {}: {e}", clip.display());
			return None;
		}
	}
	let bytes = fs::read(&tp).ok()?;
	Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

pub fn info(path: &Path, thumbs: &Path) -> Option<ClipInfo> {
	let meta = fs::metadata(path).ok()?;
	let created = meta.created().or_else(|_| meta.modified()).ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
	Some(ClipInfo {
		path: path.to_string_lossy().into(),
		name: path.file_name()?.to_string_lossy().into(),
		size_bytes: meta.len(),
		duration_sec: ff::duration_sec(path),
		created_at: created,
		thumbnail: thumbnail_data_url(thumbs, path),
	})
}

pub fn list(dir: &Path, thumbs: &Path) -> Vec<ClipInfo> {
	let Ok(rd) = fs::read_dir(dir) else { return vec![] };
	let mut files: Vec<(u64, PathBuf)> = rd
		.flatten()
		.map(|e| e.path())
		.filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("mp4") || x.eq_ignore_ascii_case("mkv")))
		.filter_map(|p| {
			let t = fs::metadata(&p).ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
			Some((t, p))
		})
		.collect();
	files.sort_by(|a, b| b.0.cmp(&a.0));
	files.into_iter().take(MAX_LISTED).filter_map(|(_, p)| info(&p, thumbs)).collect()
}

/// Only files inside the clips folder may be deleted from the UI.
pub fn delete(dir: &Path, path: &Path, thumbs: &Path) -> anyhow::Result<()> {
	let canon_dir = fs::canonicalize(dir)?;
	let canon = fs::canonicalize(path)?;
	if !canon.starts_with(&canon_dir) {
		anyhow::bail!("not a clip in the clips folder");
	}
	let _ = fs::remove_file(thumb_path(thumbs, path));
	fs::remove_file(canon)?;
	Ok(())
}

pub fn open(path: &str) {
	let _ = std::process::Command::new("explorer").arg(path).spawn();
}

pub fn reveal(path: &str) {
	let _ = std::process::Command::new("explorer").arg(format!("/select,{path}")).spawn();
}
