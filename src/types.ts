// Mirrors src-tauri/src/settings.rs (serde camelCase). Keep both in step.

export type Language = "en" | "hu";
/** "auto": the fullscreen game window in front (like Game DVR), else the monitor. */
export type CaptureApi = "auto" | "wgc" | "dxgi";
export type ResolutionPreset = "native" | "1440" | "1080" | "900" | "720" | "custom";
export type EncodeDevice = "cpu" | "gpu";
export type Codec = "h264" | "hevc" | "av1";
export type RateControl = "cbr" | "vbr" | "cqp" | "crf";
export type ColorRange = "limited" | "full";
/** "separate": own track (and still in the system mix). "mix": only in the system mix. */
export type AppRule = "separate" | "mix";
export type AudioCodec = "aac" | "opus";
/** "apps": the mix is built from one channel per app (per-app volume). "loopback": one capture of everything. */
export type MixMode = "apps" | "loopback";
export type BufferMode = "memory" | "disk";
export type Container = "mp4" | "mkv";

export interface GeneralSettings {
	language: Language;
	autostart: boolean;
	startMinimized: boolean;
	startBufferOnLaunch: boolean;
	toastOnSave: boolean;
	saveSound: boolean;
	saveSoundVolume: number;
}

export interface ResolutionSettings {
	preset: ResolutionPreset;
	width: number;
	height: number;
	keepAspect: boolean;
}

export interface VideoSettings {
	monitor: number;
	captureApi: CaptureApi;
	captureCursor: boolean;
	resolution: ResolutionSettings;
	fps: number;
	device: EncodeDevice;
	codec: Codec;
	encoder: string;
	rateControl: RateControl;
	bitrateKbps: number;
	maxBitrateKbps: number;
	quality: number;
	preset: string;
	profile: string;
	tune: string;
	keyframeSec: number;
	bFrames: number;
	threads: number;
	colorRange: ColorRange;
	priority: CapturePriority;
}

/** "game": the game keeps GPU/CPU priority. "recording": the recorder is boosted (fewer drops, lower game FPS). */
export type CapturePriority = "game" | "recording";

export interface MicSettings {
	enabled: boolean;
	deviceId: string | null;
	ownTrack: boolean;
	volume: number;
	gateDb: number;
	suppression: NoiseSuppression;
	suppressionStrength: number;
}

/** Noise suppressors, in the order the UI offers them. RNNoise first; more can join later. */
export type NoiseSuppression = "off" | "rnnoise";

export interface AudioSettings {
	systemTrack: boolean;
	separateApps: boolean;
	defaultAppRule: AppRule;
	appRules: Record<string, AppRule>;
	mixMode: MixMode;
	/** Per-app volume in percent (0–200) by lowercase exe; 100 when absent. */
	appVolumes: Record<string, number>;
	mic: MicSettings;
	codec: AudioCodec;
	bitrateKbps: number;
	sampleRate: number;
}

export interface ReplaySettings {
	lengthSec: number;
	bufferMode: BufferMode;
	cacheDir: string;
}

export interface OutputSettings {
	clipsDir: string;
	container: Container;
	filenameTemplate: string;
}

export interface HotkeySettings {
	saveClip: string;
	toggleBuffer: string;
	openWindow: string;
}

export interface Settings {
	general: GeneralSettings;
	video: VideoSettings;
	audio: AudioSettings;
	replay: ReplaySettings;
	output: OutputSettings;
	hotkeys: HotkeySettings;
}

export interface MonitorInfo {
	index: number;
	name: string;
	width: number;
	height: number;
	refreshHz: number;
	primary: boolean;
}

export interface EncoderInfo {
	id: string;
	codec: Codec;
	device: EncodeDevice;
	vendor: string;
	label: string;
}

export interface AudioApp {
	pid: number;
	exe: string;
	name: string;
	peak: number;
	tracked: boolean;
}

export interface LiveApp extends AudioApp {
	/** False while the app is only lingering in the list after going quiet. */
	active: boolean;
}

export interface MicLevel {
	/** Raw mic level before any processing, dBFS. */
	inputDb: number;
	/** dBFS after noise suppression and volume: what the gate compares against. */
	rmsDb: number;
	peakDb: number;
	gateOpen: boolean;
	/** RNNoise voice probability 0–1, null when suppression is off. */
	voice: number | null;
	listening: boolean;
	error: string | null;
}

export interface AudioLevels {
	apps: LiveApp[];
	mic: MicLevel | null;
}

export interface AudioDevice {
	id: string;
	name: string;
	isDefault: boolean;
}

export type RecorderState = "idle" | "starting" | "buffering" | "saving" | "error";

export interface RecorderStatus {
	state: RecorderState;
	bufferedSec: number;
	bufferBytes: number;
	fpsActual: number;
	droppedFrames: number;
	encoder: string;
	width: number;
	height: number;
	audioTracks: string[];
	captureTarget: string;
	error: string | null;
}

export interface ClipInfo {
	path: string;
	name: string;
	sizeBytes: number;
	durationSec: number;
	createdAt: number;
	thumbnail: string | null;
}
