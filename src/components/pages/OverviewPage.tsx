import { Button, Card, Progress } from "@mantine/core";
import { IconDeviceFloppy, IconFolder, IconPlayerPlay, IconPlayerRecordFilled, IconPlayerStop } from "@tabler/icons-react";
import { useSettings } from "../../store";
import { useT } from "../../i18n";
import { api } from "../../api";
import { useOutputSize } from "../../hooks/useOutputSize";
import { aspectLabel, estimateSize, formatBytes, formatDuration, formatMb } from "../../utils/video";
import { CARD_TITLE, HERO_VALUE, PANEL_CARD_STYLES, STAT_VALUE } from "../shared/panelStyles";
import StatTile from "../shared/StatTile";
import Notice from "../shared/Notice";
import { VerticalDivider } from "../shared/VerticalDivider";
import type { Page } from "../../App";

export default function OverviewPage({ onNavigate }: { onNavigate: (p: Page) => void }) {
	const t = useT();
	const { settings, status, lastClip } = useSettings();
	const { width, height } = useOutputSize();
	if (!settings) return null;

	const live = status?.state === "buffering" || status?.state === "saving";
	const est = estimateSize(settings, width, height, 0);
	const buffered = status?.bufferedSec ?? 0;
	const fill = Math.min(100, (buffered / settings.replay.lengthSec) * 100);

	return (
		<div className="flex flex-col gap-2">
			{status?.error && <Notice>{status.error}</Notice>}

			<Card withBorder p={12} styles={PANEL_CARD_STYLES}>
				<div className="flex items-center gap-4">
					<div className={`flex h-20 w-20 shrink-0 items-center justify-center rounded-full border ${live ? "border-(--mantine-color-red-8) bg-[rgba(250,82,82,0.08)]" : "border-(--mantine-color-dark-5) bg-(--mantine-color-dark-8)"}`}>
						<IconPlayerRecordFilled size={34} className={live ? "rec-pulse" : ""} color={live ? "var(--mantine-color-red-5)" : "var(--mantine-color-dark-3)"} />
					</div>

					<div className="min-w-0 flex-1">
						<div className={`${CARD_TITLE} text-white`}>{live ? t("overview.liveTitle") : t("overview.idleTitle")}</div>
						<div className="mt-0.5 text-xs text-(--mantine-color-dark-0)">{live ? t("overview.liveHint", { key: settings.hotkeys.saveClip, len: formatDuration(settings.replay.lengthSec) }) : t("overview.idleHint")}</div>
						<div className="mt-3 flex items-center gap-3">
							<Progress value={fill} color={live ? "red" : "gray"} size="sm" className="flex-1" transitionDuration={400} />
							<div className="w-28 text-right text-xs font-semibold tabular-nums text-(--mantine-color-dark-0)">
								{formatDuration(buffered)} / {formatDuration(settings.replay.lengthSec)}
							</div>
						</div>
					</div>

					<VerticalDivider />

					<div className="flex w-44 shrink-0 flex-col gap-2">
						{live ?
							<Button color="gray" variant="default" leftSection={<IconPlayerStop size={14} />} onClick={() => api.stopBuffer()}>
								{t("action.stopBuffer")}
							</Button>
						:	<Button color="red" leftSection={<IconPlayerPlay size={14} />} loading={status?.state === "starting"} onClick={() => api.startBuffer()}>
								{t("action.startBuffer")}
							</Button>
						}
						<Button variant="light" leftSection={<IconDeviceFloppy size={14} />} disabled={!live} loading={status?.state === "saving"} onClick={() => api.saveClip()}>
							{t("action.saveClip")}
						</Button>
					</div>
				</div>
			</Card>

			<div className="grid grid-cols-4 gap-2">
				<StatTile label={t("overview.output")}>
					<div className={`${STAT_VALUE} text-white`}>
						{width}×{height}
					</div>
					<div className="mt-1 text-xs text-(--mantine-color-dark-2)">
						{aspectLabel(width, height)} · {settings.video.fps} FPS
						{status?.captureTarget && live ? ` · ${status.captureTarget}` : ""}
					</div>
				</StatTile>
				<StatTile label={t("overview.encoder")}>
					<div className={`${STAT_VALUE} truncate text-white`}>{status?.encoder || settings.video.encoder}</div>
					<div className="mt-1 text-xs uppercase text-(--mantine-color-dark-2)">
						{settings.video.rateControl} · {settings.video.rateControl === "cbr" || settings.video.rateControl === "vbr" ? `${(settings.video.bitrateKbps / 1000).toFixed(1)} Mbps` : `Q ${settings.video.quality}`}
					</div>
				</StatTile>
				<StatTile label={t("overview.bufferSize")}>
					<div className={`${STAT_VALUE} text-white`}>{formatBytes(status?.bufferBytes ?? 0)}</div>
					<div className="mt-1 text-xs text-(--mantine-color-dark-2)">{t("overview.estimate", { mb: formatMb(est.totalMb) })}</div>
				</StatTile>
				<StatTile label={t("overview.performance")}>
					<div className={`${STAT_VALUE} text-white`}>{(status?.fpsActual ?? 0).toFixed(0)} FPS</div>
					<div className={`mt-1 text-xs ${status?.droppedFrames ? "text-(--mantine-color-yellow-4)" : "text-(--mantine-color-dark-2)"}`}>{t("overview.dropped", { n: status?.droppedFrames ?? 0 })}</div>
				</StatTile>
			</div>

			<div className="grid grid-cols-2 gap-2">
				<Card withBorder p={12} styles={PANEL_CARD_STYLES}>
					<div className={`${CARD_TITLE} text-white`}>{t("overview.tracks")}</div>
					<div className="mt-2 flex flex-wrap gap-1.5">
						{(status?.audioTracks.length ? status.audioTracks : []).map((tr) => (
							<div key={tr} className="rounded px-2 py-1 text-xs font-semibold bg-(--mantine-color-dark-8) border border-(--mantine-color-dark-6) text-(--mantine-color-dark-0)">
								{tr}
							</div>
						))}
						{!status?.audioTracks.length && <div className="text-xs text-(--mantine-color-dark-2)">{t("overview.noTracks")}</div>}
					</div>
					<Button mt="sm" size="xs" variant="subtle" color="gray" onClick={() => onNavigate("audio")}>
						{t("overview.audioSettings")}
					</Button>
				</Card>

				<Card withBorder p={12} styles={PANEL_CARD_STYLES}>
					<div className={`${CARD_TITLE} text-white`}>{t("overview.lastClip")}</div>
					{lastClip ?
						<div className="mt-2 flex items-center gap-3">
							<div className="min-w-0 flex-1">
								<div className="truncate text-sm font-semibold text-white">{lastClip.name}</div>
								<div className={`${HERO_VALUE} mt-1 text-(--bank-accent)`}>{formatBytes(lastClip.sizeBytes)}</div>
								<div className="mt-1 text-xs text-(--mantine-color-dark-2)">{formatDuration(lastClip.durationSec)}</div>
							</div>
							<Button size="xs" variant="light" leftSection={<IconFolder size={14} />} onClick={() => api.revealPath(lastClip.path)}>
								{t("action.showInFolder")}
							</Button>
						</div>
					:	<div className="mt-2 text-xs text-(--mantine-color-dark-2)">{t("overview.noClip")}</div>}
				</Card>
			</div>
		</div>
	);
}
