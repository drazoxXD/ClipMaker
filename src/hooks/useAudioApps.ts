import { useEffect, useState } from "react";
import { api } from "../api";
import type { AudioApp } from "../types";

/** Polls the apps that are making sound right now. */
export function useAudioApps(intervalMs = 750) {
	const [apps, setApps] = useState<AudioApp[]>([]);

	useEffect(() => {
		let alive = true;
		const tick = () =>
			api.listAudioApps()
				.then((a) => alive && setApps(a))
				.catch(() => {});
		tick();
		const id = window.setInterval(tick, intervalMs);
		return () => {
			alive = false;
			window.clearInterval(id);
		};
	}, [intervalMs]);

	return apps;
}
