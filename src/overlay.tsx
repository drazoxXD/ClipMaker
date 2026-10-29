import { StrictMode, useEffect } from "react";
import { createRoot } from "react-dom/client";
import "@mantine/core/styles.css";
import "@mantine/notifications/styles.css";
import "./index.css";
import { MantineProvider } from "@mantine/core";
import { Notifications, notifications, useNotifications } from "@mantine/notifications";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { IconAlertTriangle, IconCameraRotate, IconDeviceSdCard } from "@tabler/icons-react";
import { listen } from "@tauri-apps/api/event";
import { api, EVENTS } from "./api";
import { translate } from "./i18n";
import { windowTheme } from "./theme";
import { formatBytes } from "./utils/video";
import type { ClipInfo, Language } from "./types";

const SHOW_MS = 2000;

// The overlay is a click-through, always-on-top window that is excluded from screen capture, so
// these notifications are seen by the user but never end up in a clip.
function Overlay() {
	useEffect(() => {
		let savingId: string | null = null;
		let savingVisible = false;
		let seq = 0;

		const prefs = async () => {
			const s = await api.getSettings().catch(() => null);
			return { lang: (s?.general.language ?? "en") as Language, enabled: s?.general.toastOnSave ?? true };
		};

		const unSaving = listen(EVENTS.clipSaving, async () => {
			const { lang, enabled } = await prefs();
			if (!enabled) return;
			const id = `save-${++seq}`;
			savingId = id;
			savingVisible = true;
			notifications.show({
				id,
				title: translate(lang, "overlay.saving"),
				message: translate(lang, "overlay.savingHint"),
				icon: <IconCameraRotate size={18} />,
				color: "blue",
				autoClose: SHOW_MS,
				withCloseButton: false,
				onClose: () => {
					if (savingId === id) savingVisible = false;
				},
			});
		});

		const unSaved = listen<ClipInfo>(EVENTS.clipSaved, async (e) => {
			const { lang, enabled } = await prefs();
			if (!enabled) return;
			const content = {
				title: translate(lang, "overlay.saved"),
				message: `${e.payload.name} · ${formatBytes(e.payload.sizeBytes)}`,
				icon: <IconDeviceSdCard size={18} />,
				color: "green",
				withCloseButton: false,
			};

			if (savingId && savingVisible) {
				// Still on screen: turn it into "saved" and restart its timer. Mantine only restarts the
				// timer when autoClose changes, hence the 1 ms nudge.
				notifications.update({ id: savingId, ...content, autoClose: SHOW_MS + 1 });
			} else {
				notifications.show({ ...content, autoClose: SHOW_MS });
			}
			savingId = null;
		});

		const unFailed = listen<string>(EVENTS.error, async (e) => {
			const { lang } = await prefs();
			const content = { title: translate(lang, "overlay.failed"), message: e.payload, icon: <IconAlertTriangle size={18} />, color: "red", autoClose: 4000, withCloseButton: false };
			if (savingId && savingVisible) notifications.update({ id: savingId, ...content });
			else notifications.show(content);
			savingId = null;
		});

		return () => {
			unSaving.then((f) => f());
			unSaved.then((f) => f());
			unFailed.then((f) => f());
		};
	}, []);

	// The backend shows this window just before a notification. Hide it again once the last one has
	// faded out: any always-on-top window over a fullscreen game forces desktop composition, which
	// costs the game FPS and adds input lag.
	const { notifications: shown } = useNotifications();
	useEffect(() => {
		if (shown.length > 0) return;
		const id = window.setTimeout(() => getCurrentWindow().hide().catch(() => {}), 500);
		return () => window.clearTimeout(id);
	}, [shown.length]);

	return null;
}

// index.css paints the main window's background; the overlay must stay see-through.
document.documentElement.style.background = "transparent";
document.body.style.background = "transparent";

createRoot(document.getElementById("root")!).render(
	<StrictMode>
		<MantineProvider defaultColorScheme="dark" forceColorScheme="dark" theme={windowTheme} cssVariablesResolver={() => ({ variables: {}, light: {}, dark: { "--mantine-color-body": "transparent" } })}>
			<Notifications position="top-right" limit={4} containerWidth={340} />
			<Overlay />
		</MantineProvider>
	</StrictMode>,
);
