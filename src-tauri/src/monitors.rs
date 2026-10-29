use serde::Serialize;
use windows::{
	core::BOOL,
	Win32::{
		Foundation::{LPARAM, RECT},
		Graphics::Gdi::{EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW, DEVMODEW, ENUM_CURRENT_SETTINGS, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW},
		UI::WindowsAndMessaging::MONITORINFOF_PRIMARY,
	},
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
	pub index: u32,
	pub name: String,
	pub width: u32,
	pub height: u32,
	pub refresh_hz: u32,
	pub primary: bool,
	#[serde(skip)]
	pub handle: isize,
}

unsafe extern "system" fn collect(hmon: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
	let list = &mut *(data.0 as *mut Vec<HMONITOR>);
	list.push(hmon);
	true.into()
}

pub fn list() -> Vec<MonitorInfo> {
	let mut handles: Vec<HMONITOR> = Vec::new();
	unsafe {
		let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut handles as *mut _ as isize));
	}

	handles
		.into_iter()
		.enumerate()
		.filter_map(|(i, h)| unsafe {
			let mut info = MONITORINFOEXW::default();
			info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
			if !GetMonitorInfoW(h, &mut info as *mut _ as *mut MONITORINFO).as_bool() {
				return None;
			}
			let mut dm = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
			let _ = EnumDisplaySettingsW(windows::core::PCWSTR(info.szDevice.as_ptr()), ENUM_CURRENT_SETTINGS, &mut dm);
			let r = info.monitorInfo.rcMonitor;
			let (w, h_px) = if dm.dmPelsWidth > 0 { (dm.dmPelsWidth, dm.dmPelsHeight) } else { ((r.right - r.left) as u32, (r.bottom - r.top) as u32) };
			Some(MonitorInfo {
				index: i as u32,
				name: format!("Display {}", i + 1),
				width: w,
				height: h_px,
				refresh_hz: dm.dmDisplayFrequency.max(1),
				primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
				handle: h.0 as isize,
			})
		})
		.collect()
}

/// The configured monitor, or the primary one when the index no longer exists.
pub fn resolve(index: u32) -> Option<MonitorInfo> {
	let all = list();
	all.iter().find(|m| m.index == index).cloned().or_else(|| all.iter().find(|m| m.primary).cloned()).or_else(|| all.into_iter().next())
}
