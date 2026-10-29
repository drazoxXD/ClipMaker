import { useState } from "react";
import { ActionIcon, Button } from "@mantine/core";
import { IconDeviceFloppy, IconKeyboard, IconLayoutDashboard, IconMinus, IconMovie, IconSettings, IconStack2, IconVideo, IconVolume, IconX } from "@tabler/icons-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSettings } from "./store";
import { useT } from "./i18n";
import { api } from "./api";
import Brand from "./components/shared/Brand";
import ListRow from "./components/shared/ListRow";
import SectionLabel from "./components/shared/SectionLabel";
import StatePill from "./components/shared/StatePill";
import RingLoader from "./components/shared/RingLoader";
import OverviewPage from "./components/pages/OverviewPage";
import VideoPage from "./components/pages/VideoPage";
import AudioPage from "./components/pages/AudioPage";
import ReplayPage from "./components/pages/ReplayPage";
import ClipsPage from "./components/pages/ClipsPage";
import HotkeysPage from "./components/pages/HotkeysPage";
import GeneralPage from "./components/pages/GeneralPage";

export type Page = "overview" | "video" | "audio" | "replay" | "clips" | "hotkeys" | "general";

const ICONS: Record<Page, typeof IconLayoutDashboard> = {
	overview: IconLayoutDashboard,
	video: IconVideo,
	audio: IconVolume,
	replay: IconStack2,
	clips: IconMovie,
	hotkeys: IconKeyboard,
	general: IconSettings,
};

const NAV_GROUPS: { label?: string; pages: Page[] }[] = [{ pages: ["overview", "clips"] }, { label: "nav.groupRecording", pages: ["video", "audio", "replay"] }, { label: "nav.groupApp", pages: ["hotkeys", "general"] }];

export default function App() {
	const t = useT();
	const { settings, status } = useSettings();
	const [page, setPage] = useState<Page>("overview");
	const win = getCurrentWindow();

	if (!settings) {
		return (
			<div className="flex h-full items-center justify-center bg-(--mantine-color-dark-9)">
				<RingLoader />
			</div>
		);
	}

	const buffering = status?.state === "buffering";

	return (
		<div className="flex h-full w-full overflow-hidden bg-(--mantine-color-dark-7)">
			<div className="flex w-56 shrink-0 flex-col bg-(--mantine-color-dark-9) border-r border-r-(--mantine-color-dark-6)">
				<div data-tauri-drag-region className="flex h-16 shrink-0 items-center px-4 border-b border-b-(--mantine-color-dark-6)">
					<Brand />
				</div>

				<div className="flex-1 overflow-y-auto px-3 py-3">
					<div className="flex flex-col gap-4">
						{NAV_GROUPS.map((group) => (
							<div key={group.pages.join("-")} className="flex flex-col gap-1">
								{group.label && (
									<div className="mb-1 flex items-center gap-2 px-1">
										<SectionLabel>{t(group.label)}</SectionLabel>
										<div className="h-px flex-1 bg-(--mantine-color-dark-6)" />
									</div>
								)}
								{group.pages.map((id) => {
									const Icon = ICONS[id];
									const active = page === id;
									return (
										<ListRow key={id} active={active} nav surface="shell" onClick={() => setPage(id)} className={`gap-2.5 px-2.5 py-2 ${active ? "text-(--bank-selected-text)" : "text-(--mantine-color-dark-0) hover:text-white"}`}>
											<Icon size={16} />
											<div className="flex-1 truncate text-left text-xs font-semibold uppercase tracking-wide">{t(`nav.${id}`)}</div>
										</ListRow>
									);
								})}
							</div>
						))}
					</div>
				</div>

				<div className="px-3 py-3 border-t border-t-(--mantine-color-dark-6)">
					<Button fullWidth color={buffering ? "red" : "blue"} variant="light" leftSection={<IconDeviceFloppy size={14} />} disabled={!buffering} onClick={() => api.saveClip()}>
						{t("action.saveClip")}
					</Button>
				</div>
			</div>

			<div className="flex min-w-0 flex-1 flex-col">
				<div data-tauri-drag-region className="flex h-16 shrink-0 items-center justify-between px-5 bg-(--mantine-color-dark-9) border-b border-b-(--mantine-color-dark-6)">
					<div data-tauri-drag-region className="flex items-center gap-2">
						<div data-tauri-drag-region className="truncate font-condensed text-xl font-bold uppercase leading-tight tracking-[0.04em] text-white">
							{t(`nav.${page}`)}
						</div>
						{status && <StatePill state={status.state} />}
					</div>

					<div className="flex items-center gap-2">
						<ActionIcon variant="light" color="gray" radius="xl" onClick={() => win.minimize()} aria-label={t("action.minimize")}>
							<IconMinus size={16} />
						</ActionIcon>
						<ActionIcon variant="light" color="red" radius="xl" onClick={() => win.close()} aria-label={t("action.close")}>
							<IconX size={16} />
						</ActionIcon>
					</div>
				</div>

				<div className="min-h-0 flex-1 overflow-y-auto p-3 bg-(--mantine-color-dark-9)">
					<div key={page} className="bank-swap-in">
						{page === "overview" && <OverviewPage onNavigate={setPage} />}
						{page === "video" && <VideoPage />}
						{page === "audio" && <AudioPage />}
						{page === "replay" && <ReplayPage />}
						{page === "clips" && <ClipsPage />}
						{page === "hotkeys" && <HotkeysPage />}
						{page === "general" && <GeneralPage />}
					</div>
				</div>
			</div>
		</div>
	);
}
