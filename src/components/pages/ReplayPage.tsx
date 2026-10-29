import { NumberInput, SegmentedControl, Slider, TextInput } from "@mantine/core";
import { IconCpu, IconDatabase, IconDeviceSdCard, IconMusic, IconVideo } from "@tabler/icons-react";
import { useSettings } from "../../store";
import { useT } from "../../i18n";
import { useOutputSize } from "../../hooks/useOutputSize";
import { useAudioApps } from "../../hooks/useAudioApps";
import { estimateSize, formatDuration, formatMb } from "../../utils/video";
import type { BufferMode, Container } from "../../types";
import Panel from "../shared/Panel";
import SettingRow from "../shared/SettingRow";
import StatTile from "../shared/StatTile";
import FolderInput from "../shared/FolderInput";
import Notice from "../shared/Notice";
import { STAT_VALUE } from "../shared/panelStyles";

// The slider walks these stops so short lengths stay easy to pick; the number input takes any value.
const STOPS = [15, 30, 45, 60, 90, 120, 180, 240, 300, 420, 600, 900, 1200, 1800];

function nearestStop(sec: number) {
	let best = 0;
	STOPS.forEach((s, i) => {
		if (Math.abs(s - sec) < Math.abs(STOPS[best] - sec)) best = i;
	});
	return best;
}

export default function ReplayPage() {
	const t = useT();
	const { settings, update } = useSettings();
	const { width, height } = useOutputSize();
	const apps = useAudioApps(2000);
	if (!settings) return null;

	const r = settings.replay;
	const separated = settings.audio.separateApps ? apps.filter((a) => (settings.audio.appRules[a.exe.toLowerCase()] ?? settings.audio.defaultAppRule) === "separate").length : 0;
	const est = estimateSize(settings, width, height, separated);
	const approx = est.approx ? "~" : "";

	return (
		<div className="flex flex-col gap-2">
			<Panel title={t("replay.length.title")} hint={t("replay.length.hint")}>
				<div className="flex items-center gap-4">
					<Slider
						className="flex-1"
						min={0}
						max={STOPS.length - 1}
						step={1}
						value={nearestStop(r.lengthSec)}
						label={(i) => formatDuration(STOPS[i])}
						marks={[0, 3, 8, 10, 13].map((i) => ({ value: i, label: formatDuration(STOPS[i]) }))}
						onChange={(i) => update((s) => void (s.replay.lengthSec = STOPS[i]))}
						mb="lg"
					/>
					<NumberInput size="xs" className="w-28" min={5} max={7200} suffix=" s" value={r.lengthSec} onChange={(v) => update((s) => void (s.replay.lengthSec = Math.max(5, Number(v) || 30)))} />
				</div>

				<div className="grid grid-cols-4 gap-2">
					<StatTile label={t("replay.estimate.total")} icon={<IconDatabase size={18} color="var(--bank-accent)" />}>
						<div className={`${STAT_VALUE} text-white`}>
							{approx}
							{formatMb(est.totalMb)}
						</div>
					</StatTile>
					<StatTile label={t("replay.estimate.video")} icon={<IconVideo size={18} color="var(--mantine-color-dark-2)" />}>
						<div className={`${STAT_VALUE} text-white`}>
							{approx}
							{formatMb(est.videoMb)}
						</div>
					</StatTile>
					<StatTile label={t("replay.estimate.audio", { n: est.tracks })} icon={<IconMusic size={18} color="var(--mantine-color-dark-2)" />}>
						<div className={`${STAT_VALUE} text-white`}>{formatMb(est.audioMb)}</div>
					</StatTile>
					<StatTile label={t("replay.estimate.perApp")} icon={<IconMusic size={18} color="var(--mantine-color-dark-2)" />}>
						<div className={`${STAT_VALUE} text-white`}>+{formatMb(est.perAppTrackMb)}</div>
					</StatTile>
				</div>
				<div className="text-xs text-(--mantine-color-dark-2)">{t("replay.estimate.note", { w: width, h: height, fps: settings.video.fps })}</div>
				{est.approx && (
					<Notice tone="info" size="xs">
						{t("replay.estimate.approxNote")}
					</Notice>
				)}
			</Panel>

			<Panel title={t("replay.buffer.title")} hint={t("replay.buffer.hint")}>
				<SettingRow label={t("replay.buffer.mode")} hint={r.bufferMode === "memory" ? t("replay.buffer.memoryHint", { mb: formatMb(est.totalMb * 1.1) }) : t("replay.buffer.diskHint")}>
					<SegmentedControl
						size="xs"
						fullWidth
						className="w-full"
						value={r.bufferMode}
						data={[
							{ value: "memory", label: <span className="flex items-center justify-center gap-1.5"><IconCpu size={14} />{t("replay.buffer.memory")}</span> },
							{ value: "disk", label: <span className="flex items-center justify-center gap-1.5"><IconDeviceSdCard size={14} />{t("replay.buffer.disk")}</span> },
						]}
						onChange={(v) => update((s) => void (s.replay.bufferMode = v as BufferMode))}
					/>
				</SettingRow>
				<SettingRow label={t("replay.buffer.cacheDir")} hint={t("replay.buffer.cacheDirHint")} controlWidth="w-[28rem]">
					<FolderInput disabled={r.bufferMode !== "disk"} value={r.cacheDir} onChange={(p) => update((s) => void (s.replay.cacheDir = p))} />
				</SettingRow>
			</Panel>

			<Panel title={t("replay.output.title")} hint={t("replay.output.hint")}>
				<SettingRow label={t("replay.output.clipsDir")} controlWidth="w-[28rem]">
					<FolderInput value={settings.output.clipsDir} onChange={(p) => update((s) => void (s.output.clipsDir = p))} />
				</SettingRow>
				<SettingRow label={t("replay.output.container")} hint={t("replay.output.containerHint")}>
					<SegmentedControl
						size="xs"
						fullWidth
						className="w-full"
						value={settings.output.container}
						data={[
							{ value: "mp4", label: "MP4" },
							{ value: "mkv", label: "MKV" },
						]}
						onChange={(v) => update((s) => void (s.output.container = v as Container))}
					/>
				</SettingRow>
				<SettingRow label={t("replay.output.template")} hint={t("replay.output.templateHint")} controlWidth="w-[28rem]">
					<TextInput size="xs" className="w-full" value={settings.output.filenameTemplate} onChange={(e) => update((s) => void (s.output.filenameTemplate = e.currentTarget.value))} styles={{ input: { fontFamily: "monospace" } }} />
				</SettingRow>
			</Panel>
		</div>
	);
}
