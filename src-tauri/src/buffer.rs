//! The replay buffer holds *encoded* packets, so its size is roughly bitrate × length rather than
//! raw frames. Memory mode keeps them in a deque; disk mode writes rolling ~2 s segment files to the
//! cache folder and keeps only an index in RAM. Both trim at video keyframes so a clip can always
//! start cleanly.

use crate::ff::CodecParams;
use anyhow::Result;
use std::{
	collections::{BTreeMap, VecDeque},
	fs::{self, File},
	io::{BufReader, BufWriter, Read, Write},
	path::PathBuf,
	sync::Arc,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StreamKind {
	Video,
	Audio,
}

#[derive(Clone)]
pub struct StreamInfo {
	pub kind: StreamKind,
	pub title: String,
	/// App tracks: leave this track out of a clip in which the app never made a sound.
	pub skip_if_silent: bool,
	pub params: Arc<CodecParams>,
}

#[derive(Clone)]
pub struct Packet {
	pub stream: u32,
	pub pts_us: i64,
	pub dts_us: i64,
	pub dur_us: i64,
	pub key: bool,
	/// Audio: the chunk this packet came from had audible sound. Video: always true.
	pub loud: bool,
	pub data: Arc<[u8]>,
}

/// Packets for one clip, with the streams they belong to.
pub struct Snapshot {
	pub streams: BTreeMap<u32, StreamInfo>,
	pub packets: Vec<Packet>,
	pub start_us: i64,
	pub end_us: i64,
}

const SEGMENT_US: i64 = 2_000_000;

struct Segment {
	path: PathBuf,
	last_dts: i64,
	bytes: u64,
}

struct DiskStore {
	dir: PathBuf,
	segments: VecDeque<Segment>,
	writer: Option<BufWriter<File>>,
	current_start: i64,
	next_id: u64,
}

impl DiskStore {
	fn new(dir: PathBuf) -> Result<Self> {
		let _ = fs::remove_dir_all(&dir);
		fs::create_dir_all(&dir)?;
		Ok(Self { dir, segments: VecDeque::new(), writer: None, current_start: i64::MIN, next_id: 0 })
	}

	fn push(&mut self, p: &Packet, video_key: bool) -> Result<()> {
		if self.writer.is_none() || (video_key && p.dts_us - self.current_start >= SEGMENT_US) {
			if let Some(mut w) = self.writer.take() {
				w.flush()?;
			}
			let path = self.dir.join(format!("seg{:08}.drx", self.next_id));
			self.next_id += 1;
			self.writer = Some(BufWriter::with_capacity(1 << 20, File::create(&path)?));
			self.current_start = p.dts_us;
			self.segments.push_back(Segment { path, last_dts: p.dts_us, bytes: 0 });
		}
		let w = self.writer.as_mut().unwrap();
		// Record: stream u32 | pts i64 | dts i64 | dur i64 | key u8 | len u32 | data
		w.write_all(&p.stream.to_le_bytes())?;
		w.write_all(&p.pts_us.to_le_bytes())?;
		w.write_all(&p.dts_us.to_le_bytes())?;
		w.write_all(&p.dur_us.to_le_bytes())?;
		w.write_all(&[p.key as u8 | (p.loud as u8) << 1])?;
		w.write_all(&(p.data.len() as u32).to_le_bytes())?;
		w.write_all(&p.data)?;
		let seg = self.segments.back_mut().unwrap();
		seg.last_dts = seg.last_dts.max(p.dts_us);
		seg.bytes += 33 + p.data.len() as u64;
		Ok(())
	}

	fn trim(&mut self, floor: i64) {
		// Never delete the segment being written.
		while self.segments.len() > 1 && self.segments[0].last_dts < floor {
			let s = self.segments.pop_front().unwrap();
			let _ = fs::remove_file(s.path);
		}
	}

	fn read(&mut self, from_dts: i64) -> Result<Vec<Packet>> {
		if let Some(w) = self.writer.as_mut() {
			w.flush()?;
		}
		let mut out = Vec::new();
		for seg in self.segments.iter().filter(|s| s.last_dts >= from_dts) {
			let mut r = BufReader::new(File::open(&seg.path)?);
			let mut head = [0u8; 33];
			while r.read_exact(&mut head).is_ok() {
				let i64_at = |o: usize| i64::from_le_bytes(head[o..o + 8].try_into().unwrap());
				let len = u32::from_le_bytes(head[29..33].try_into().unwrap()) as usize;
				let mut data = vec![0u8; len];
				if r.read_exact(&mut data).is_err() {
					break;
				}
				out.push(Packet { stream: u32::from_le_bytes(head[0..4].try_into().unwrap()), pts_us: i64_at(4), dts_us: i64_at(12), dur_us: i64_at(20), key: head[28] & 1 != 0, loud: head[28] & 2 != 0, data: data.into() });
			}
		}
		Ok(out)
	}

	fn bytes(&self) -> u64 {
		self.segments.iter().map(|s| s.bytes).sum()
	}
}

impl Drop for DiskStore {
	fn drop(&mut self) {
		self.writer.take();
		let _ = fs::remove_dir_all(&self.dir);
	}
}

enum Store {
	Memory { packets: VecDeque<Packet>, bytes: u64 },
	Disk(DiskStore),
}

pub struct ReplayBuffer {
	streams: BTreeMap<u32, StreamInfo>,
	store: Store,
	length_us: i64,
	/// DTS of every video keyframe still in the buffer, oldest first.
	keyframes: VecDeque<i64>,
	video_stream: Option<u32>,
	newest_dts: i64,
	pub last_error: Option<String>,
}

impl ReplayBuffer {
	pub fn new(length_sec: u32, disk_dir: Option<PathBuf>) -> Result<Self> {
		let store = match disk_dir {
			Some(dir) => Store::Disk(DiskStore::new(dir)?),
			None => Store::Memory { packets: VecDeque::new(), bytes: 0 },
		};
		Ok(Self { streams: BTreeMap::new(), store, length_us: length_sec as i64 * 1_000_000, keyframes: VecDeque::new(), video_stream: None, newest_dts: i64::MIN, last_error: None })
	}

	pub fn add_stream(&mut self, id: u32, info: StreamInfo) {
		if info.kind == StreamKind::Video {
			self.video_stream = Some(id);
		}
		self.streams.insert(id, info);
	}

	pub fn audio_titles(&self) -> Vec<String> {
		self.streams.values().filter(|s| s.kind == StreamKind::Audio).map(|s| s.title.clone()).collect()
	}

	pub fn push(&mut self, p: Packet) {
		let video_key = Some(p.stream) == self.video_stream && p.key;
		if Some(p.stream) == self.video_stream {
			self.newest_dts = self.newest_dts.max(p.dts_us);
			if p.key {
				self.keyframes.push_back(p.dts_us);
			}
		}

		match &mut self.store {
			Store::Memory { packets, bytes } => {
				*bytes += p.data.len() as u64;
				packets.push_back(p);
			}
			Store::Disk(d) => {
				if let Err(e) = d.push(&p, video_key) {
					self.last_error = Some(format!("cache write failed: {e}"));
				}
			}
		}
		self.trim();
	}

	/// Drops everything older than the keyframe that still covers the full replay length.
	fn trim(&mut self) {
		let want_from = self.newest_dts.saturating_sub(self.length_us);
		while self.keyframes.len() >= 2 && self.keyframes[1] <= want_from {
			self.keyframes.pop_front();
		}
		let Some(&floor) = self.keyframes.front() else { return };
		match &mut self.store {
			Store::Memory { packets, bytes } => {
				while packets.front().is_some_and(|p| p.dts_us < floor) {
					*bytes -= packets.pop_front().unwrap().data.len() as u64;
				}
			}
			Store::Disk(d) => d.trim(floor),
		}
	}

	pub fn buffered_sec(&self) -> f64 {
		match self.keyframes.front() {
			Some(&k) if self.newest_dts > k => ((self.newest_dts - k) as f64 / 1e6).min(self.length_us as f64 / 1e6),
			_ => 0.0,
		}
	}

	pub fn bytes(&self) -> u64 {
		match &self.store {
			Store::Memory { bytes, .. } => *bytes,
			Store::Disk(d) => d.bytes(),
		}
	}

	/// The last `length` seconds, starting at a keyframe, ready to mux.
	pub fn snapshot(&mut self) -> Result<Option<Snapshot>> {
		let want_from = self.newest_dts.saturating_sub(self.length_us);
		let Some(start) = self.keyframes.iter().rev().find(|&&k| k <= want_from).or(self.keyframes.front()).copied() else {
			return Ok(None);
		};
		let end = self.newest_dts;
		let raw: Vec<Packet> = match &mut self.store {
			Store::Memory { packets, .. } => packets.iter().filter(|p| p.dts_us >= start).cloned().collect(),
			Store::Disk(d) => d.read(start)?.into_iter().filter(|p| p.dts_us >= start).collect(),
		};
		let mut packets: Vec<Packet> = raw.into_iter().filter(|p| p.dts_us <= end + 200_000).collect();
		packets.sort_by_key(|p| p.dts_us);
		Ok(Some(Snapshot { streams: self.streams.clone(), packets, start_us: start, end_us: end }))
	}
}
