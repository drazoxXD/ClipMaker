import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, EVENTS } from "../api";
import type { UpdateStatus } from "../types";

/** The updater's state, kept live from its events. */
export function useUpdate() {
	const [status, setStatus] = useState<UpdateStatus | null>(null);

	useEffect(() => {
		api.updateStatus().then(setStatus).catch(() => {});
		const un = listen<UpdateStatus>(EVENTS.update, (e) => setStatus(e.payload));
		return () => {
			un.then((f) => f());
		};
	}, []);

	return status;
}
