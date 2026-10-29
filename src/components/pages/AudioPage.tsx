import { useEffect, useState } from "react";
import { ActionIcon, Badge, NumberInput, SegmentedControl, Select, Slider, Switch } from "@mantine/core";
import { IconDeviceFloppy, IconTrash, IconVolume, IconVolumeOff } from "@tabler/icons-react";
import { useSettings } from "../../store";
import { useT } from "../../i18n";
import { api } from "../../api";
import { useAudioLevels } from "../../hooks/useAudioLevels";
import type { AppRule, AudioCodec, AudioDevice, MixMode, NoiseSuppression } from "../../types";
import Panel from "../shared/Panel";
import SettingRow from "../shared/SettingRow";
import EmptyState from "../shared/EmptyState";
import SectionLabel from "../shared/SectionLabel";
import Notice from "../shared/Notice";
import GateMeter from "../shared/GateMeter";
import SuppressionMeter from "../shared/SuppressionMeter";
import { invoke } from "@tauri-apps/api/core";

function LevelMeter({ peak }: { peak: number }) {
	const pct = Math.min(100, Math.sqrt(peak) * 100);
	return (
		<div className="h-1.5 w-24 overflow-hidden rounded-full bg-(--mantine-color-dark-6)">
			<div className="h-full rounded-full bg-(--mantine-color-green-6) transition-[width] duration-150" style={{ width: `${pct}%` }} />
		</div>
	);
}

/** One app's channel volume (0–200 %) with a mute toggle; applies live to the mix and its own track. */
function AppVolume({ value, onChange, label }: { value: number; onChange: (v: number) => void; label: string }) {
	const muted = value === 0;
	return (
		<div className="flex w-56 shrink-0 items-center gap-2">
			<ActionIcon size="sm" variant={muted ? "light" : "subtle"} color={muted ? "red" : "gray"} onClick={() => onChange(muted ? 100 : 0)} aria-label={label}>
				{muted ? <IconVolumeOff size={14} /> : <IconVolume size={14} />}
			</ActionIcon>
			<Slider className="flex-1" size="sm" min={0} max={200} step={5} value={value} label={(v) => `${v}%`} onChange={onChange} />
			<div className={`w-10 text-right text-xs font-semibold tabular-nums ${muted ? "text-(--mantine-color-red-4)" : "text-(--mantine-color-dark-0)"}`}>{value}%</div>
		</div>
	);
}

export default function AudioPage() {
	const t = useT();
	const { settings, update } = useSettings();
	const { apps, mic: micLevel } = useAudioLevels();
	const [mics, setMics] = useState<AudioDevice[]>([]);
	const [listening, setListeningState] = useState(false);

	// Listen plays the processed mic back on the default output; always switch it off when leaving the page.
	const setListening = (on: boolean) => {
		setListeningState(on);
		invoke("monitor_listen", { on }).catch(() => {});
	};
	useEffect(() => () => void invoke("monitor_listen", { on: false }).catch(() => {}), []);

	useEffect(() => {
		api.listMicDevices().then(setMics).catch(() => {});
	}, []);

	if (!settings) return null;
	const a = settings.audio;
	const ruleFor = (exe: string): AppRule => a.appRules[exe.toLowerCase()] ?? a.defaultAppRule;
	/** The exe already has saved settings (rule or volume): its row says so instead of listing it twice. */
	const isSaved = (exe: string) => exe.toLowerCase() in a.appRules || exe.toLowerCase() in a.appVolumes;
	const volumeFor = (exe: string) => a.appVolumes[exe.toLowerCase()] ?? 100;
	const setVolume = (exe: string, v: number) =>
		update((s) => {
			// 100 % is the default: keep the stored map to the apps you actually changed.
			if (v === 100) delete s.audio.appVolumes[exe.toLowerCase()];
			else s.audio.appVolumes[exe.toLowerCase()] = v;
		});
	const remembered = [...new Set([...Object.keys(a.appRules), ...Object.keys(a.appVolumes)])].filter((exe) => !apps.some((p) => p.exe.toLowerCase() === exe));
	const ruleData = [
		{ value: "separate", label: t("audio.rule.separate") },
		{ value: "mix", label: t("audio.rule.mix") },
	];

	return (
		<div className="flex flex-col gap-2">
			<Panel title={t("audio.tracks.title")} hint={t("audio.tracks.hint")}>
				<SettingRow label={t("audio.tracks.system")} hint={t("audio.tracks.systemHint")}>
					<Switch checked={a.systemTrack} onChange={(e) => update((s) => void (s.audio.systemTrack = e.currentTarget.checked))} />
				</SettingRow>
				<SettingRow label={t("audio.tracks.mixMode")} hint={t(`audio.tracks.mixModeHint.${a.mixMode}`)}>
					<SegmentedControl
						size="xs"
						fullWidth
						className="w-full"
						disabled={!a.systemTrack}
						value={a.mixMode}
						data={[
							{ value: "apps", label: t("audio.tracks.mixApps") },
							{ value: "loopback", label: t("audio.tracks.mixLoopback") },
						]}
						onChange={(val) => update((s) => void (s.audio.mixMode = val as MixMode))}
					/>
				</SettingRow>
				<SettingRow label={t("audio.tracks.separate")} hint={t("audio.tracks.separateHint")}>
					<Switch checked={a.separateApps} onChange={(e) => update((s) => void (s.audio.separateApps = e.currentTarget.checked))} />
				</SettingRow>
				<SettingRow label={t("audio.tracks.defaultRule")} hint={t("audio.tracks.defaultRuleHint")}>
					<SegmentedControl size="xs" fullWidth className="w-full" value={a.defaultAppRule} data={ruleData} onChange={(val) => update((s) => void (s.audio.defaultAppRule = val as AppRule))} />
				</SettingRow>
			</Panel>

			<Panel title={t("audio.apps.title")} hint={t("audio.apps.hint")}>
				{!a.separateApps && <Notice tone="info" size="xs">{t("audio.apps.disabled")}</Notice>}
				{a.mixMode === "loopback" && <Notice tone="info" size="xs">{t("audio.apps.loopbackVolume")}</Notice>}
				{apps.length === 0 ?
					<EmptyState icon={<IconVolume size={16} />} title={t("audio.apps.empty")} hint={t("audio.apps.emptyHint")} />
				:	apps.map((app) => (
						<div key={app.exe.toLowerCase()} className={`flex items-center gap-3 rounded px-3 py-2 bg-(--mantine-color-dark-8) border border-(--mantine-color-dark-6) transition-opacity duration-300 ${app.active ? "" : "opacity-55"}`}>
							<div className="min-w-0 flex-1">
								<div className="truncate text-sm font-semibold text-white">{app.name}</div>
								<div className="truncate text-xs text-(--mantine-color-dark-2)">
									{app.exe}
									{isSaved(app.exe) && (
										<Badge ml={8} size="xs" variant="light" color="blue" leftSection={<IconDeviceFloppy size={10} />}>
											{t("audio.apps.saved")}
										</Badge>
									)}
									{app.tracked && <span className="ml-2 text-(--mantine-color-red-4)">● {t("audio.apps.recording")}</span>}
									{!app.active && <span className="ml-2 text-(--mantine-color-dark-2)">· {t("audio.apps.quiet")}</span>}
								</div>
							</div>
							<LevelMeter peak={app.peak} />
							<AppVolume value={volumeFor(app.exe)} onChange={(v) => setVolume(app.exe, v)} label={t("audio.apps.mute")} />
							<SegmentedControl size="xs" className="w-56" fullWidth value={ruleFor(app.exe)} data={ruleData} onChange={(val) => update((s) => void (s.audio.appRules[app.exe.toLowerCase()] = val as AppRule))} />
						</div>
					))
				}

				{remembered.length > 0 && (
					<>
						<div className="mt-1 text-[11px] font-semibold uppercase tracking-[0.08em] text-(--mantine-color-dark-2)">{t("audio.apps.remembered")}</div>
						{remembered.map((exe) => (
							<div key={exe} className="flex items-center gap-3 rounded px-3 py-1.5 bg-(--mantine-color-dark-8) border border-(--mantine-color-dark-6)">
								<div className="min-w-0 flex-1 truncate text-sm text-(--mantine-color-dark-0)">{exe}</div>
								{a.appRules[exe] && <div className="text-xs uppercase text-(--mantine-color-dark-2)">{t(`audio.rule.${a.appRules[exe]}`)}</div>}
								<AppVolume value={volumeFor(exe)} onChange={(v) => setVolume(exe, v)} label={t("audio.apps.mute")} />
								<ActionIcon
									size="sm"
									variant="subtle"
									color="gray"
									onClick={() =>
										update((s) => {
											delete s.audio.appRules[exe];
											delete s.audio.appVolumes[exe];
										})
									}
									aria-label={t("action.delete")}
								>
									<IconTrash size={14} />
								</ActionIcon>
							</div>
						))}
					</>
				)}
			</Panel>

			<Panel title={t("audio.mic.title")} hint={t("audio.mic.hint")} right={<Switch checked={a.mic.enabled} onChange={(e) => update((s) => void (s.audio.mic.enabled = e.currentTarget.checked))} />}>
				<SettingRow label={t("audio.mic.device")} controlWidth="w-80">
					<Select
						size="xs"
						className="w-full"
						disabled={!a.mic.enabled}
						value={a.mic.deviceId ?? "__default"}
						allowDeselect={false}
						data={[{ value: "__default", label: t("audio.mic.default") }, ...mics.map((m) => ({ value: m.id, label: m.name }))]}
						onChange={(val) => update((s) => void (s.audio.mic.deviceId = val === "__default" ? null : val))}
					/>
				</SettingRow>
				<SettingRow label={t("audio.mic.ownTrack")} hint={t("audio.mic.ownTrackHint")}>
					<Switch disabled={!a.mic.enabled} checked={a.mic.ownTrack} onChange={(e) => update((s) => void (s.audio.mic.ownTrack = e.currentTarget.checked))} />
				</SettingRow>
				<SettingRow label={t("audio.mic.volume")} controlWidth="w-80">
					<Slider className="w-full" disabled={!a.mic.enabled} min={0} max={200} value={a.mic.volume} label={(v) => `${v}%`} onChange={(v) => update((s) => void (s.audio.mic.volume = v))} />
				</SettingRow>
				<SettingRow label={t("audio.mic.suppression")} hint={t("audio.mic.suppressionHint")}>
					<SegmentedControl
						size="xs"
						fullWidth
						className="w-full"
						disabled={!a.mic.enabled}
						value={a.mic.suppression}
						data={[
							{ value: "off", label: t("audio.mic.suppressionOff") },
							{ value: "rnnoise", label: "RNNoise" },
						]}
						onChange={(v) => update((s) => void (s.audio.mic.suppression = v as NoiseSuppression))}
					/>
				</SettingRow>
				{a.mic.suppression !== "off" && a.mic.enabled && <SuppressionMeter level={micLevel} />}
				{a.mic.suppression !== "off" && (
					<SettingRow label={t("audio.mic.strength")} hint={t("audio.mic.strengthHint")} controlWidth="w-80">
						<Slider className="w-full" disabled={!a.mic.enabled} min={0} max={100} value={a.mic.suppressionStrength} label={(v) => `${v}%`} onChange={(v) => update((s) => void (s.audio.mic.suppressionStrength = v))} />
					</SettingRow>
				)}
				<div className="mt-1">
					<div className="mb-2">
						<SectionLabel tone="body">{t("audio.mic.gate")}</SectionLabel>
						<div className="mt-0.5 text-xs text-(--mantine-color-dark-2)">{t("audio.mic.gateHint")}</div>
					</div>
					<GateMeter level={a.mic.enabled ? micLevel : null} gateDb={a.mic.gateDb} disabled={!a.mic.enabled} onGateChange={(v) => update((s) => void (s.audio.mic.gateDb = v))} listening={listening} onListenChange={setListening} />
					{micLevel?.error && <Notice className="mt-2">{micLevel.error}</Notice>}
				</div>
			</Panel>

			<Panel title={t("audio.encoding.title")}>
				<SettingRow label={t("audio.encoding.codec")}>
					<SegmentedControl
						size="xs"
						fullWidth
						className="w-full"
						value={a.codec}
						data={[
							{ value: "aac", label: "AAC" },
							{ value: "opus", label: "Opus" },
						]}
						onChange={(val) => update((s) => void (s.audio.codec = val as AudioCodec))}
					/>
				</SettingRow>
				<SettingRow label={t("audio.encoding.bitrate")} hint={t("audio.encoding.bitrateHint")}>
					<NumberInput size="xs" className="w-full" min={64} max={512} step={32} suffix=" kbps" value={a.bitrateKbps} onChange={(v) => update((s) => void (s.audio.bitrateKbps = Number(v) || 160))} />
				</SettingRow>
				<SettingRow label={t("audio.encoding.sampleRate")}>
					<SegmentedControl
						size="xs"
						fullWidth
						className="w-full"
						value={String(a.sampleRate)}
						data={[
							{ value: "44100", label: "44.1 kHz" },
							{ value: "48000", label: "48 kHz" },
						]}
						onChange={(val) => update((s) => void (s.audio.sampleRate = Number(val)))}
					/>
				</SettingRow>
			</Panel>
		</div>
	);
}
