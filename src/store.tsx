import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, EVENTS } from "./api";
import { playSaveSound } from "./utils/saveSound";
import type { ClipInfo, EncoderInfo, MonitorInfo, RecorderStatus, Settings } from "./types";

interface SettingsContextValue {
	settings: Settings | null;
	/** Applies a change locally at once and persists it after a short debounce, so sliders stay smooth. */
	update: (fn: (draft: Settings) => void) => void;
	monitors: MonitorInfo[];
	encoders: EncoderInfo[];
	status: RecorderStatus | null;
	lastClip: ClipInfo | null;
	saveError: string | null;
}

const SettingsContext = createContext<SettingsContextValue | null>(null);

const SAVE_DEBOUNCE_MS = 400;

export function SettingsProvider({ children }: { children: ReactNode }) {
	const [settings, setSettings] = useState<Settings | null>(null);
	const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
	const [encoders, setEncoders] = useState<EncoderInfo[]>([]);
	const [status, setStatus] = useState<RecorderStatus | null>(null);
	const [lastClip, setLastClip] = useState<ClipInfo | null>(null);
	const [saveError, setSaveError] = useState<string | null>(null);
	const timer = useRef<number | undefined>(undefined);
	const settingsRef = useRef<Settings | null>(null);
	settingsRef.current = settings;

	useEffect(() => {
		api.getSettings().then(setSettings);
		api.listMonitors().then(setMonitors);
		api.listEncoders().then(setEncoders);
		api.getStatus().then(setStatus);

		const unStatus = listen<RecorderStatus>(EVENTS.status, (e) => setStatus(e.payload));
		const unClip = listen<ClipInfo>(EVENTS.clipSaved, (e) => {
			setLastClip(e.payload);
			const g = settingsRef.current?.general;
			if (g?.saveSound) playSaveSound(g.saveSoundVolume);
		});
		return () => {
			unStatus.then((f) => f());
			unClip.then((f) => f());
		};
	}, []);

	const update = useCallback((fn: (draft: Settings) => void) => {
		setSettings((prev) => {
			if (!prev) return prev;
			const next = structuredClone(prev);
			fn(next);

			window.clearTimeout(timer.current);
			timer.current = window.setTimeout(() => {
				api.setSettings(next)
					.then(() => setSaveError(null))
					.catch((e) => setSaveError(String(e)));
			}, SAVE_DEBOUNCE_MS);

			return next;
		});
	}, []);

	return <SettingsContext.Provider value={{ settings, update, monitors, encoders, status, lastClip, saveError }}>{children}</SettingsContext.Provider>;
}

export function useSettings() {
	const ctx = useContext(SettingsContext);
	if (!ctx) throw new Error("useSettings outside SettingsProvider");
	return ctx;
}
