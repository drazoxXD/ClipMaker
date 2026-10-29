import type { Codec, RateControl } from "../types";

/** The encoder families FFmpeg exposes; each takes its own preset/profile/tune vocabulary. */
export type EncoderFamily = "x264" | "x265" | "svtav1" | "amf" | "nvenc" | "qsv";

export function encoderFamily(id: string): EncoderFamily {
	if (id === "libx264") return "x264";
	if (id === "libx265") return "x265";
	if (id === "libsvtav1") return "svtav1";
	if (id.endsWith("_amf")) return "amf";
	if (id.endsWith("_nvenc")) return "nvenc";
	return "qsv";
}

const X26X_PRESETS = ["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"];

export const PRESETS: Record<EncoderFamily, string[]> = {
	x264: X26X_PRESETS,
	x265: X26X_PRESETS,
	svtav1: ["13", "12", "11", "10", "9", "8", "7", "6", "5", "4"],
	amf: ["speed", "balanced", "quality"],
	nvenc: ["p1", "p2", "p3", "p4", "p5", "p6", "p7"],
	qsv: ["veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"],
};

export const DEFAULT_PRESET: Record<EncoderFamily, string> = {
	x264: "veryfast",
	x265: "veryfast",
	svtav1: "10",
	amf: "balanced",
	nvenc: "p4",
	qsv: "medium",
};

export const PROFILES: Record<Codec, string[]> = {
	h264: ["baseline", "main", "high"],
	hevc: ["main", "main10"],
	av1: ["main"],
};

export const TUNES: Record<EncoderFamily, string[]> = {
	x264: ["none", "film", "animation", "grain", "stillimage", "fastdecode", "zerolatency"],
	x265: ["none", "grain", "animation", "fastdecode", "zerolatency"],
	svtav1: ["none"],
	amf: ["transcoding", "ultralowlatency", "lowlatency", "webcam", "highquality"],
	nvenc: ["hq", "ll", "ull", "lossless"],
	qsv: ["none"],
};

/** Which rate-control modes each family supports; CRF is x26x/SVT only, CQP everywhere. */
export function rateControls(family: EncoderFamily): RateControl[] {
	if (family === "x264" || family === "x265" || family === "svtav1") return ["cbr", "vbr", "crf", "cqp"];
	return ["cbr", "vbr", "cqp"];
}

export function qualityRange(family: EncoderFamily): [number, number] {
	return family === "svtav1" ? [0, 63] : [0, 51];
}
