import { invoke } from "@tauri-apps/api/core";
import type { AudioApp, AudioDevice, ClipInfo, EncoderInfo, MonitorInfo, RecorderStatus, Settings } from "./types";

export const api = {
	getSettings: () => invoke<Settings>("get_settings"),
	setSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
	listMonitors: () => invoke<MonitorInfo[]>("list_monitors"),
	listEncoders: () => invoke<EncoderInfo[]>("list_encoders"),
	listAudioApps: () => invoke<AudioApp[]>("list_audio_apps"),
	listMicDevices: () => invoke<AudioDevice[]>("list_mic_devices"),
	getStatus: () => invoke<RecorderStatus>("get_status"),
	startBuffer: () => invoke<void>("start_buffer_cmd"),
	stopBuffer: () => invoke<void>("stop_buffer_cmd"),
	saveClip: () => invoke<void>("save_clip_cmd"),
	listClips: () => invoke<ClipInfo[]>("list_clips"),
	deleteClip: (path: string) => invoke<void>("delete_clip", { path }),
	openPath: (path: string) => invoke<void>("open_path", { path }),
	revealPath: (path: string) => invoke<void>("reveal_path", { path }),
};

export const EVENTS = {
	status: "recorder://status",
	clipSaving: "recorder://clip-saving",
	clipSaved: "recorder://clip-saved",
	error: "recorder://error",
} as const;
