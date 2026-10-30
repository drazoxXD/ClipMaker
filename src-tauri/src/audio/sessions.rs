//! Which apps are making sound right now: the default output's audio sessions with their peak meters.

use super::wasapi;
use parking_lot::Mutex;
use serde::Serialize;
use std::{
	collections::{HashMap, HashSet},
	time::{Duration, Instant},
};
use windows::{
	core::{Interface, PWSTR},
	Win32::{
		Foundation::{CloseHandle, S_OK},
		Media::Audio::{IAudioSessionControl2, Endpoints::IAudioMeterInformation},
		System::{
			Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS},
			Threading::{GetExitCodeProcess, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION},
		},
		UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
	},
};

/// A peak above this counts as "making sound".
pub const SOUND_THRESHOLD: f32 = 0.0005;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioApp {
	pub pid: u32,
	pub exe: String,
	pub name: String,
	pub peak: f32,
	pub tracked: bool,
	/// Every separate process tree of this exe (e.g. several VLC instances); each is captured, and
	/// they all share the exe's volume and rule. `pid` is the first of them.
	#[serde(skip)]
	pub roots: Vec<u32>,
}

pub fn exe_path(pid: u32) -> Option<String> {
	unsafe {
		let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
		let mut buf = [0u16; 1024];
		let mut len = buf.len() as u32;
		let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
		let _ = CloseHandle(h);
		ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
	}
}

pub fn exe_name(pid: u32) -> Option<String> {
	exe_path(pid).and_then(|p| std::path::Path::new(&p).file_name().map(|f| f.to_string_lossy().into_owned()))
}

pub fn pretty_name(exe: &str) -> String {
	let stem = exe.strip_suffix(".exe").or_else(|| exe.strip_suffix(".EXE")).unwrap_or(exe);
	let mut c = stem.chars();
	c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

pub fn is_alive(pid: u32) -> bool {
	unsafe {
		let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
		let mut code = 0u32;
		let ok = GetExitCodeProcess(h, &mut code).is_ok();
		let _ = CloseHandle(h);
		ok && code == 259 // STILL_ACTIVE
	}
}

/// `own_tree`, refreshed at most every 2 s: the process snapshot is too slow to take 15 times a second.
fn own_tree_cached() -> HashSet<u32> {
	static CACHE: Mutex<Option<(Instant, HashSet<u32>)>> = Mutex::new(None);
	let mut c = CACHE.lock();
	match c.as_ref() {
		Some((at, set)) if at.elapsed() < Duration::from_secs(2) => set.clone(),
		_ => {
			let set = own_tree();
			*c = Some((Instant::now(), set.clone()));
			set
		}
	}
}

/// Exe names by PID; looked up once per process.
fn exe_name_cached(pid: u32) -> String {
	static CACHE: Mutex<Option<HashMap<u32, String>>> = Mutex::new(None);
	let mut c = CACHE.lock();
	let map = c.get_or_insert_with(HashMap::new);
	if map.len() > 512 {
		map.clear();
	}
	map.entry(pid).or_insert_with(|| exe_name(pid).unwrap_or_else(|| format!("pid{pid}"))).clone()
}

/// Our own process and everything it spawned (the WebView2 processes play the UI's sounds).
pub fn own_tree() -> HashSet<u32> {
	let me = std::process::id();
	let mut parents: HashMap<u32, u32> = HashMap::new();
	unsafe {
		if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
			let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
			if Process32FirstW(snap, &mut e).is_ok() {
				loop {
					parents.insert(e.th32ProcessID, e.th32ParentProcessID);
					if Process32NextW(snap, &mut e).is_err() {
						break;
					}
				}
			}
			let _ = CloseHandle(snap);
		}
	}
	parents
		.keys()
		.copied()
		.filter(|&pid| {
			let mut p = pid;
			for _ in 0..16 {
				if p == me {
					return true;
				}
				match parents.get(&p) {
					Some(&pp) if pp != p && pp != 0 => p = pp,
					_ => return false,
				}
			}
			false
		})
		.chain(Some(me))
		.collect()
}

/// Apps making sound right now (plus ones already on their own track), loudest first. Needs COM.
pub fn list(tracked: &HashSet<u32>) -> Vec<AudioApp> {
	list_all(tracked).into_iter().filter(|a| a.peak > SOUND_THRESHOLD || a.tracked).collect()
}

/// pid → (parent pid, exe file name) for every process, refreshed at most every 2 s.
fn process_table() -> HashMap<u32, (u32, String)> {
	static CACHE: Mutex<Option<(Instant, HashMap<u32, (u32, String)>)>> = Mutex::new(None);
	let mut c = CACHE.lock();
	if let Some((at, t)) = c.as_ref() {
		if at.elapsed() < Duration::from_secs(2) {
			return t.clone();
		}
	}
	let mut table = HashMap::new();
	unsafe {
		if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
			let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
			if Process32FirstW(snap, &mut e).is_ok() {
				loop {
					let len = e.szExeFile.iter().position(|&ch| ch == 0).unwrap_or(e.szExeFile.len());
					table.insert(e.th32ProcessID, (e.th32ParentProcessID, String::from_utf16_lossy(&e.szExeFile[..len])));
					if Process32NextW(snap, &mut e).is_err() {
						break;
					}
				}
			}
			let _ = CloseHandle(snap);
		}
	}
	*c = Some((Instant::now(), table.clone()));
	table
}

/// The top process of `pid`'s run of same-exe ancestors: Brave's audio comes from a child process,
/// and capturing the root's process tree covers every Brave process at once.
fn exe_root(pid: u32, table: &HashMap<u32, (u32, String)>) -> u32 {
	let Some((_, exe)) = table.get(&pid) else { return pid };
	let mut root = pid;
	for _ in 0..32 {
		match table.get(&root) {
			Some((parent, _)) if *parent != root && table.get(parent).is_some_and(|(_, e)| e.eq_ignore_ascii_case(exe)) => root = *parent,
			_ => break,
		}
	}
	root
}

/// Every app with an audio session on the default output, silent or not, loudest first: one entry
/// per exe (its sessions merged, `pid` = the exe's root process). Needs COM.
pub fn list_all(tracked: &HashSet<u32>) -> Vec<AudioApp> {
	let mine = own_tree_cached();
	let table = process_table();
	let mut by_exe: HashMap<String, AudioApp> = HashMap::new();
	let Ok(mgr) = wasapi::default_render_sessions() else { return vec![] };
	unsafe {
		let Ok(en) = mgr.GetSessionEnumerator() else { return vec![] };
		for i in 0..en.GetCount().unwrap_or(0) {
			let Ok(ctl) = en.GetSession(i) else { continue };
			let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else { continue };
			if ctl2.IsSystemSoundsSession() == S_OK {
				continue;
			}
			let Ok(pid) = ctl2.GetProcessId() else { continue };
			// Windows keeps the sessions of exited processes around for a while; capturing those would
			// restart a dead capture every pass.
			if pid == 0 || mine.contains(&pid) || !table.contains_key(&pid) {
				continue;
			}
			let peak = ctl.cast::<IAudioMeterInformation>().and_then(|m| m.GetPeakValue()).unwrap_or(0.0);
			let root = exe_root(pid, &table);
			let exe = table.get(&root).map(|(_, e)| e.clone()).unwrap_or_else(|| exe_name_cached(pid));
			let entry = by_exe.entry(exe.to_lowercase()).or_insert_with(|| AudioApp { pid: root, name: pretty_name(&exe), exe, peak: 0.0, tracked: false, roots: Vec::new() });
			entry.peak = entry.peak.max(peak);
			if !entry.roots.contains(&root) {
				entry.roots.push(root);
			}
			entry.tracked |= tracked.contains(&root);
		}
	}
	let mut v: Vec<AudioApp> = by_exe.into_values().collect();
	v.sort_by(|a, b| b.peak.partial_cmp(&a.peak).unwrap_or(std::cmp::Ordering::Equal));
	v
}

/// The exe stem of the foreground window's process, used for {app} in clip names.
pub fn foreground_app() -> String {
	unsafe {
		let hwnd = GetForegroundWindow();
		let mut pid = 0u32;
		GetWindowThreadProcessId(hwnd, Some(&mut pid));
		exe_name(pid).map(|e| e.trim_end_matches(".exe").trim_end_matches(".EXE").to_string()).unwrap_or_else(|| "Desktop".into())
	}
}
