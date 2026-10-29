import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AudioLevels } from "../types";

/** Live app list and mic level, streamed by the backend ~15×/s while a component holds this hook. */
export function useAudioLevels() {
	const [levels, setLevels] = useState<AudioLevels>({ apps: [], mic: null });

	useEffect(() => {
		invoke("monitor_start").catch(() => {});
		const un = listen<AudioLevels>("audio://levels", (e) => setLevels(e.payload));
		return () => {
			invoke("monitor_stop").catch(() => {});
			un.then((f) => f());
		};
	}, []);

	return levels;
}
