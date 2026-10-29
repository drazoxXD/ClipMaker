import { Checkbox, NumberInput, SegmentedControl, Select, Slider, Switch } from "@mantine/core";
import { IconLock, IconLockOpen } from "@tabler/icons-react";
import { useSettings } from "../../store";
import { useT } from "../../i18n";
import { useOutputSize } from "../../hooks/useOutputSize";
import { aspectLabel } from "../../utils/video";
import { DEFAULT_PRESET, encoderFamily, PRESETS, PROFILES, qualityRange, rateControls, TUNES } from "../../utils/encoderOptions";
import type { CapturePriority, Codec, EncodeDevice, EncoderInfo, RateControl, ResolutionPreset, Settings } from "../../types";
import Panel from "../shared/Panel";
import SettingRow from "../shared/SettingRow";
import Notice from "../shared/Notice";

const FPS_CHOICES = ["30", "60", "120", "144", "240"];
const RES_PRESETS: ResolutionPreset[] = ["native", "1440", "1080", "900", "720", "custom"];

/** Points the video settings at a working encoder after device or codec changed, resetting family-specific options. */
function applyEncoder(v: Settings["video"], enc: EncoderInfo) {
	const fam = encoderFamily(enc.id);
	v.encoder = enc.id;
	v.codec = enc.codec;
	v.device = enc.device;
	v.preset = DEFAULT_PRESET[fam];
	v.tune = TUNES[fam][0];
	v.profile = PROFILES[enc.codec].includes(v.profile) ? v.profile : PROFILES[enc.codec][PROFILES[enc.codec].length - 1];
	if (!rateControls(fam).includes(v.rateControl)) v.rateControl = "cbr";
}

export default function VideoPage() {
	const t = useT();
	const { settings, update, monitors, encoders } = useSettings();
	const { monW, monH, width, height } = useOutputSize();
	if (!settings) return null;

	const v = settings.video;
	const fam = encoderFamily(v.encoder);
	const deviceEncoders = encoders.filter((e) => e.device === v.device);
	const codecs = [...new Set(deviceEncoders.map((e) => e.codec))];
	const codecEncoders = deviceEncoders.filter((e) => e.codec === v.codec);
	const [qMin, qMax] = qualityRange(fam);
	const bitrateMode = v.rateControl === "cbr" || v.rateControl === "vbr";

	const setDevice = (device: EncodeDevice) => {
		const enc = encoders.find((e) => e.device === device && e.codec === v.codec) ?? encoders.find((e) => e.device === device);
		if (enc) update((s) => applyEncoder(s.video, enc));
	};
	const setCodec = (codec: Codec) => {
		const enc = deviceEncoders.find((e) => e.codec === codec);
		if (enc) update((s) => applyEncoder(s.video, enc));
	};

	const setCustomSize = (w: number, h: number, changed: "w" | "h") =>
		update((s) => {
			const r = s.video.resolution;
			if (r.keepAspect) {
				if (changed === "w") h = Math.round((w * monH) / monW);
				else w = Math.round((h * monW) / monH);
			}
			r.width = w;
			r.height = h;
		});

	return (
		<div className="flex flex-col gap-2">
			<Panel title={t("video.capture.title")} hint={t("video.capture.hint")}>
				<SettingRow label={t("video.capture.monitor")} controlWidth="w-80">
					<Select
						size="xs"
						className="w-full"
						allowDeselect={false}
						value={String(v.monitor)}
						data={monitors.map((m) => ({ value: String(m.index), label: `${m.primary ? "★ " : ""}${m.name} · ${m.width}×${m.height} @ ${m.refreshHz}Hz` }))}
						onChange={(val) => update((s) => void (s.video.monitor = Number(val)))}
					/>
				</SettingRow>
				<SettingRow label={t("video.capture.api")} hint={t("video.capture.apiHint")}>
					<SegmentedControl
						size="xs"
						fullWidth
						className="w-full"
						value={v.captureApi}
						data={[
							{ value: "auto", label: t("video.capture.apiAuto") },
							{ value: "wgc", label: t("video.capture.apiMonitor") },
							{ value: "dxgi", label: "DXGI" },
						]}
						onChange={(val) => update((s) => void (s.video.captureApi = val as Settings["video"]["captureApi"]))}
					/>
				</SettingRow>
				<SettingRow label={t("video.capture.priority")} hint={t(`video.capture.priorityHint.${v.priority}`)}>
					<SegmentedControl
						size="xs"
						fullWidth
						className="w-full"
						value={v.priority}
						data={[
							{ value: "game", label: t("video.capture.priorityGame") },
							{ value: "recording", label: t("video.capture.priorityRecording") },
						]}
						onChange={(val) => update((s) => void (s.video.priority = val as CapturePriority))}
					/>
				</SettingRow>
				<SettingRow label={t("video.capture.cursor")}>
					<Switch checked={v.captureCursor} onChange={(e) => update((s) => void (s.video.captureCursor = e.currentTarget.checked))} />
				</SettingRow>
			</Panel>

			<Panel title={t("video.output.title")} hint={t("video.output.hint", { w: width, h: height, aspect: aspectLabel(width, height) })}>
				<SettingRow label={t("video.output.resolution")} controlWidth="w-[26rem]">
					<SegmentedControl
						size="xs"
						fullWidth
						className="w-full"
						value={v.resolution.preset}
						data={RES_PRESETS.map((p) => ({ value: p, label: p === "native" ? t("video.output.native") : p === "custom" ? t("video.output.custom") : `${p}p` }))}
						onChange={(val) =>
							update((s) => {
								s.video.resolution.preset = val as ResolutionPreset;
								if (val === "custom") {
									s.video.resolution.width = width;
									s.video.resolution.height = height;
								}
							})
						}
					/>
				</SettingRow>

				{v.resolution.preset === "custom" && (
					<SettingRow label={t("video.output.customSize")} controlWidth="w-[26rem]">
						<div className="flex w-full items-center gap-2">
							<NumberInput size="xs" className="flex-1" min={64} max={7680} step={2} value={v.resolution.width} onChange={(val) => setCustomSize(Number(val) || 0, v.resolution.height, "w")} />
							<span className="text-(--mantine-color-dark-2)">×</span>
							<NumberInput size="xs" className="flex-1" min={64} max={4320} step={2} value={v.resolution.height} onChange={(val) => setCustomSize(v.resolution.width, Number(val) || 0, "h")} />
							<Checkbox
								size="xs"
								icon={({ className }) => (v.resolution.keepAspect ? <IconLock className={className} /> : <IconLockOpen className={className} />)}
								label={t("video.output.keepAspect")}
								checked={v.resolution.keepAspect}
								onChange={(e) => update((s) => void (s.video.resolution.keepAspect = e.currentTarget.checked))}
							/>
						</div>
					</SettingRow>
				)}

				<SettingRow label={t("video.output.fps")} controlWidth="w-[26rem]">
					<div className="flex w-full items-center gap-2">
						<SegmentedControl size="xs" className="flex-1" fullWidth value={FPS_CHOICES.includes(String(v.fps)) ? String(v.fps) : ""} data={FPS_CHOICES} onChange={(val) => update((s) => void (s.video.fps = Number(val)))} />
						<NumberInput size="xs" className="w-20" min={1} max={360} value={v.fps} onChange={(val) => update((s) => void (s.video.fps = Number(val) || 60))} />
					</div>
				</SettingRow>
			</Panel>

			<Panel title={t("video.encoder.title")} hint={t("video.encoder.hint")}>
				{encoders.length === 0 && <Notice>{t("video.encoder.noneFound")}</Notice>}

				<SettingRow label={t("video.encoder.device")} hint={t("video.encoder.deviceHint")}>
					<SegmentedControl
						size="xs"
						fullWidth
						className="w-full"
						value={v.device}
						data={[
							{ value: "cpu", label: "CPU", disabled: !encoders.some((e) => e.device === "cpu") },
							{ value: "gpu", label: "GPU", disabled: !encoders.some((e) => e.device === "gpu") },
						]}
						onChange={(val) => setDevice(val as EncodeDevice)}
					/>
				</SettingRow>
				<SettingRow label={t("video.encoder.codec")}>
					<SegmentedControl
						size="xs"
						fullWidth
						className="w-full"
						value={v.codec}
						data={(["h264", "hevc", "av1"] as Codec[]).map((c) => ({ value: c, label: c === "h264" ? "H.264" : c === "hevc" ? "HEVC" : "AV1", disabled: !codecs.includes(c) }))}
						onChange={(val) => setCodec(val as Codec)}
					/>
				</SettingRow>
				<SettingRow label={t("video.encoder.encoder")}>
					<Select
						size="xs"
						className="w-full"
						allowDeselect={false}
						value={v.encoder}
						data={codecEncoders.map((e) => ({ value: e.id, label: e.label }))}
						onChange={(val) => {
							const enc = encoders.find((e) => e.id === val);
							if (enc) update((s) => applyEncoder(s.video, enc));
						}}
					/>
				</SettingRow>
			</Panel>

			<Panel title={t("video.rate.title")} hint={t("video.rate.hint")}>
				<SettingRow label={t("video.rate.mode")} hint={t(`video.rate.modeHint.${v.rateControl}`)}>
					<SegmentedControl
						size="xs"
						fullWidth
						className="w-full"
						value={v.rateControl}
						data={rateControls(fam).map((r) => ({ value: r, label: r.toUpperCase() }))}
						onChange={(val) => update((s) => void (s.video.rateControl = val as RateControl))}
					/>
				</SettingRow>

				{bitrateMode && (
					<SettingRow label={t("video.rate.bitrate")} hint={t("video.rate.bitrateHint")} controlWidth="w-[26rem]">
						<div className="flex w-full items-center gap-3">
							<Slider className="flex-1" min={1000} max={150000} step={500} value={v.bitrateKbps} label={(val) => `${(val / 1000).toFixed(1)} Mbps`} onChange={(val) => update((s) => void (s.video.bitrateKbps = val))} />
							<NumberInput size="xs" className="w-28" min={500} max={500000} step={500} suffix=" kbps" value={v.bitrateKbps} onChange={(val) => update((s) => void (s.video.bitrateKbps = Number(val) || 1000))} />
						</div>
					</SettingRow>
				)}
				{v.rateControl === "vbr" && (
					<SettingRow label={t("video.rate.maxBitrate")} controlWidth="w-[26rem]">
						<NumberInput size="xs" className="w-full" min={v.bitrateKbps} max={500000} step={500} suffix=" kbps" value={v.maxBitrateKbps} onChange={(val) => update((s) => void (s.video.maxBitrateKbps = Number(val) || s.video.bitrateKbps))} />
					</SettingRow>
				)}
				{!bitrateMode && (
					<SettingRow label={t("video.rate.quality")} hint={t("video.rate.qualityHint")} controlWidth="w-[26rem]">
						<div className="flex w-full items-center gap-3">
							<Slider className="flex-1" min={qMin} max={qMax} value={v.quality} onChange={(val) => update((s) => void (s.video.quality = val))} />
							<NumberInput size="xs" className="w-20" min={qMin} max={qMax} value={v.quality} onChange={(val) => update((s) => void (s.video.quality = Number(val)))} />
						</div>
					</SettingRow>
				)}
			</Panel>

			<Panel title={t("video.advanced.title")} hint={t("video.advanced.hint")}>
				<SettingRow label={t("video.advanced.preset")} hint={t("video.advanced.presetHint")}>
					<Select size="xs" className="w-full" allowDeselect={false} value={v.preset} data={PRESETS[fam]} onChange={(val) => val && update((s) => void (s.video.preset = val))} />
				</SettingRow>
				<SettingRow label={t("video.advanced.profile")}>
					<Select size="xs" className="w-full" allowDeselect={false} value={v.profile} data={PROFILES[v.codec]} onChange={(val) => val && update((s) => void (s.video.profile = val))} />
				</SettingRow>
				<SettingRow label={t("video.advanced.tune")}>
					<Select size="xs" className="w-full" allowDeselect={false} value={v.tune} data={TUNES[fam]} onChange={(val) => val && update((s) => void (s.video.tune = val))} />
				</SettingRow>
				<SettingRow label={t("video.advanced.keyframe")} hint={t("video.advanced.keyframeHint")}>
					<NumberInput size="xs" className="w-full" min={0.5} max={10} step={0.5} decimalScale={1} suffix=" s" value={v.keyframeSec} onChange={(val) => update((s) => void (s.video.keyframeSec = Number(val) || 1))} />
				</SettingRow>
				<SettingRow label={t("video.advanced.bframes")} hint={t("video.advanced.bframesHint")}>
					<NumberInput size="xs" className="w-full" min={0} max={7} value={v.bFrames} onChange={(val) => update((s) => void (s.video.bFrames = Number(val) || 0))} />
				</SettingRow>
				{v.device === "cpu" && (
					<SettingRow label={t("video.advanced.threads")} hint={t("video.advanced.threadsHint")}>
						<NumberInput size="xs" className="w-full" min={0} max={64} value={v.threads} onChange={(val) => update((s) => void (s.video.threads = Number(val) || 0))} />
					</SettingRow>
				)}
				<SettingRow label={t("video.advanced.colorRange")}>
					<SegmentedControl
						size="xs"
						fullWidth
						className="w-full"
						value={v.colorRange}
						data={[
							{ value: "limited", label: t("video.advanced.limited") },
							{ value: "full", label: t("video.advanced.full") },
						]}
						onChange={(val) => update((s) => void (s.video.colorRange = val as Settings["video"]["colorRange"]))}
					/>
				</SettingRow>
			</Panel>
		</div>
	);
}
