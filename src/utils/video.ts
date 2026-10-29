import type { Codec, ResolutionPreset, Settings } from "../types";

export const PRESET_HEIGHTS: Record<Exclude<ResolutionPreset, "native" | "custom">, number> = {
	"1440": 1440,
	"1080": 1080,
	"900": 900,
	"720": 720,
};

const even = (n: number) => Math.max(2, Math.round(n / 2) * 2);

/** Output size for a preset on a given monitor. The width follows the monitor's own aspect, so 21:9 stays 21:9: no bars. */
export function outputSize(settings: Settings["video"]["resolution"], monW: number, monH: number): { width: number; height: number } {
	if (settings.preset === "native") return { width: even(monW), height: even(monH) };
	if (settings.preset === "custom") return { width: even(settings.width), height: even(settings.height) };

	const h = Math.min(PRESET_HEIGHTS[settings.preset], monH);
	return { width: even((h * monW) / monH), height: even(h) };
}

export function aspectLabel(w: number, h: number): string {
	const r = w / h;
	if (Math.abs(r - 64 / 27) < 0.03 || Math.abs(r - 43 / 18) < 0.03) return "21:9";
	if (Math.abs(r - 32 / 9) < 0.03) return "32:9";
	if (Math.abs(r - 16 / 9) < 0.02) return "16:9";
	if (Math.abs(r - 16 / 10) < 0.02) return "16:10";
	if (Math.abs(r - 4 / 3) < 0.02) return "4:3";
	return r.toFixed(2) + ":1";
}

// Rough bits-per-pixel at the reference quality (23) for constant-quality modes; good enough for an estimate.
const CODEC_EFFICIENCY: Record<Codec, number> = { h264: 1, hevc: 0.65, av1: 0.55 };

export function estimatedVideoKbps(video: Settings["video"], width: number, height: number): { kbps: number; approx: boolean } {
	if (video.rateControl === "cbr") return { kbps: video.bitrateKbps, approx: false };
	if (video.rateControl === "vbr") return { kbps: video.bitrateKbps, approx: true };

	const bpp = 0.1 * Math.pow(2, (23 - video.quality) / 6) * CODEC_EFFICIENCY[video.codec];
	return { kbps: Math.round((bpp * width * height * video.fps) / 1000), approx: true };
}

export interface SizeEstimate {
	totalMb: number;
	videoMb: number;
	audioMb: number;
	perAppTrackMb: number;
	tracks: number;
	approx: boolean;
}

export function estimateSize(settings: Settings, width: number, height: number, separatedApps: number): SizeEstimate {
	const { kbps, approx } = estimatedVideoKbps(settings.video, width, height);
	const a = settings.audio;
	let tracks = 0;
	if (a.systemTrack) tracks++;
	if (a.mic.enabled && a.mic.ownTrack) tracks++;
	if (a.separateApps) tracks += separatedApps;

	const secs = settings.replay.lengthSec;
	const toMb = (k: number) => (k * secs) / 8 / 1024;
	const videoMb = toMb(kbps);
	const audioMb = toMb(a.bitrateKbps * tracks);

	return { totalMb: videoMb + audioMb, videoMb, audioMb, perAppTrackMb: toMb(a.bitrateKbps), tracks, approx };
}

export function formatMb(mb: number): string {
	if (mb >= 1024) return `${(mb / 1024).toFixed(2)} GB`;
	return `${mb.toFixed(mb < 10 ? 1 : 0)} MB`;
}

export function formatBytes(b: number): string {
	return formatMb(b / 1024 / 1024);
}

export function formatDuration(sec: number): string {
	const s = Math.round(sec);
	const m = Math.floor(s / 60);
	const r = s % 60;
	if (m >= 60) return `${Math.floor(m / 60)}:${String(m % 60).padStart(2, "0")}:${String(r).padStart(2, "0")}`;
	return `${m}:${String(r).padStart(2, "0")}`;
}
