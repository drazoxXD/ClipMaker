//! One master clock for every source: QueryPerformanceCounter in microseconds.
//! Video ticks, audio packets (whose QPC positions WASAPI reports in 100 ns units) and the
//! replay buffer all live on this timeline, which is what keeps tracks in sync.

use std::sync::OnceLock;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

fn freq() -> i64 {
	static FREQ: OnceLock<i64> = OnceLock::new();
	*FREQ.get_or_init(|| {
		let mut f = 0i64;
		unsafe { QueryPerformanceFrequency(&mut f).ok() };
		f.max(1)
	})
}

/// Microseconds on the QPC timeline.
pub fn now_us() -> i64 {
	let mut c = 0i64;
	unsafe { QueryPerformanceCounter(&mut c).ok() };
	(c as i128 * 1_000_000 / freq() as i128) as i64
}

/// WASAPI's `u64QPCPosition` is already QPC converted to 100 ns units.
pub fn hns_to_us(hns: u64) -> i64 {
	(hns / 10) as i64
}

/// Sleeps until `deadline_us`. std's sleep uses a high-resolution waitable timer on Windows, so this
/// lands within ~0.5 ms without spinning a CPU core the game could use.
pub fn sleep_until(deadline_us: i64) {
	let left = deadline_us - now_us();
	if left > 0 {
		std::thread::sleep(std::time::Duration::from_micros(left as u64));
	}
}
