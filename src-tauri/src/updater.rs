//! Updates from the GitHub releases. CI bakes its run number into the exe (CLIPMAKER_BUILD), and each
//! release is tagged build-<run number>. A newer build's installer is downloaded in the background;
//! installing hands off to a hidden PowerShell that waits for us to exit, runs the installer silently
//! and starts the new version with the same arguments. Local builds have no number and never update.

use anyhow::{anyhow, Context, Result};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::{
	io::{Read, Write},
	os::windows::process::CommandExt,
	path::PathBuf,
	sync::atomic::{AtomicBool, Ordering},
	time::Duration,
};
use tauri::{AppHandle, Emitter};

const LATEST: &str = "https://api.github.com/repos/drazoxXD/ClipMaker/releases/latest";
const ASSET: &str = "ClipMaker_Setup.exe";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn current_build() -> Option<u32> {
	option_env!("CLIPMAKER_BUILD").and_then(|b| b.parse().ok())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
	pub current: Option<u32>,
	pub latest: Option<u32>,
	/// "dev" (local build), "idle", "checking", "upToDate", "downloading", "ready", "installing", "error".
	pub state: String,
	/// 0..1 while downloading.
	pub progress: f32,
	pub error: Option<String>,
}

pub struct Updater {
	status: RwLock<UpdateStatus>,
	installer: Mutex<Option<PathBuf>>,
	busy: AtomicBool,
}

struct Release {
	build: u32,
	url: String,
	size: u64,
}

fn agent() -> ureq::Agent {
	ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout_read(Duration::from_secs(30)).user_agent("ClipMaker-updater").build()
}

fn latest_release() -> Result<Release> {
	let body = agent().get(LATEST).set("Accept", "application/vnd.github+json").call()?.into_string()?;
	let v: serde_json::Value = serde_json::from_str(&body)?;
	let tag = v["tag_name"].as_str().ok_or_else(|| anyhow!("release has no tag"))?;
	let build = tag.strip_prefix("build-").and_then(|n| n.parse().ok()).ok_or_else(|| anyhow!("unexpected release tag {tag}"))?;
	let asset = v["assets"].as_array().and_then(|a| a.iter().find(|a| a["name"] == ASSET)).ok_or_else(|| anyhow!("{tag} has no {ASSET}"))?;
	Ok(Release { build, url: asset["browser_download_url"].as_str().unwrap_or_default().to_string(), size: asset["size"].as_u64().unwrap_or(0) })
}

impl Updater {
	pub fn new() -> Self {
		let current = current_build();
		let state = if current.is_some() { "idle" } else { "dev" };
		Self { status: RwLock::new(UpdateStatus { current, latest: None, state: state.into(), progress: 0.0, error: None }), installer: Mutex::new(None), busy: AtomicBool::new(false) }
	}

	pub fn status(&self) -> UpdateStatus {
		self.status.read().clone()
	}

	fn set(&self, app: &AppHandle, f: impl FnOnce(&mut UpdateStatus)) {
		let st = {
			let mut s = self.status.write();
			f(&mut s);
			s.clone()
		};
		let _ = app.emit("update://status", st);
	}

	/// Looks for a newer build and downloads its installer. Returns whether one is ready to install.
	pub fn check(&self, app: &AppHandle) -> bool {
		let Some(current) = current_build() else { return false };
		if self.installer.lock().is_some() {
			return true;
		}
		if self.busy.swap(true, Ordering::SeqCst) {
			return false;
		}
		self.set(app, |s| {
			s.state = "checking".into();
			s.error = None;
		});
		let result = self.check_inner(app, current);
		self.busy.store(false, Ordering::SeqCst);
		match result {
			Ok(ready) => ready,
			Err(e) => {
				log::warn!("update check: {e:#}");
				self.set(app, |s| {
					s.state = "error".into();
					s.error = Some(format!("{e:#}"));
				});
				false
			}
		}
	}

	fn check_inner(&self, app: &AppHandle, current: u32) -> Result<bool> {
		let rel = latest_release()?;
		self.set(app, |s| s.latest = Some(rel.build));
		if rel.build <= current {
			self.set(app, |s| s.state = "upToDate".into());
			return Ok(false);
		}

		let path = std::env::temp_dir().join(format!("ClipMaker_Setup_build-{}.exe", rel.build));
		let have = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
		if rel.size == 0 || have != rel.size {
			self.set(app, |s| {
				s.state = "downloading".into();
				s.progress = 0.0;
			});
			let part = path.with_extension("part");
			let mut reader = agent().get(&rel.url).call()?.into_reader();
			let mut file = std::fs::File::create(&part).with_context(|| format!("creating {}", part.display()))?;
			let mut buf = vec![0u8; 256 * 1024];
			let (mut done, mut shown) = (0u64, 0f32);
			loop {
				let n = reader.read(&mut buf)?;
				if n == 0 {
					break;
				}
				file.write_all(&buf[..n])?;
				done += n as u64;
				let p = if rel.size > 0 { done as f32 / rel.size as f32 } else { 0.0 };
				if p - shown >= 0.02 {
					shown = p;
					self.set(app, |s| s.progress = p);
				}
			}
			drop(file);
			if rel.size > 0 && done != rel.size {
				return Err(anyhow!("download ended at {done} of {} bytes", rel.size));
			}
			std::fs::rename(&part, &path)?;
		}
		*self.installer.lock() = Some(path);
		self.set(app, |s| {
			s.state = "ready".into();
			s.progress = 1.0;
		});
		Ok(true)
	}

	/// Installs the downloaded build and restarts into it. The app exits here.
	pub fn install(&self, app: &AppHandle) -> Result<()> {
		let setup = self.installer.lock().clone().ok_or_else(|| anyhow!("no update downloaded"))?;
		let exe = std::env::current_exe()?;
		let q = |p: &std::path::Path| p.display().to_string().replace('\'', "''");
		let args: Vec<String> = std::env::args().skip(1).map(|a| format!("'{}'", a.replace('\'', "''"))).collect();
		let relaunch = if args.is_empty() { String::new() } else { format!(" -ArgumentList {}", args.join(",")) };
		let script = format!(
			"Wait-Process -Id {} -ErrorAction SilentlyContinue; Start-Process -FilePath '{}' -ArgumentList '/S' -Wait; Start-Process -FilePath '{}'{}",
			std::process::id(),
			q(&setup),
			q(&exe),
			relaunch
		);
		std::process::Command::new("powershell").args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-Command", &script]).creation_flags(CREATE_NO_WINDOW).spawn().context("starting the installer")?;
		self.set(app, |s| s.state = "installing".into());
		app.exit(0);
		Ok(())
	}
}
