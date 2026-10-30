import { Button, Select, Slider, Switch } from "@mantine/core";
import { IconDownload, IconRefresh, IconVolume } from "@tabler/icons-react";
import { api } from "../../api";
import { useUpdate } from "../../hooks/useUpdate";
import { useSettings } from "../../store";
import { LOCALES, useT } from "../../i18n";
import { playSaveSound } from "../../utils/saveSound";
import type { Language } from "../../types";
import Panel from "../shared/Panel";
import SettingRow from "../shared/SettingRow";

export default function GeneralPage() {
	const t = useT();
	const { settings, update } = useSettings();
	const up = useUpdate();
	if (!settings) return null;
	const g = settings.general;
	const dev = up?.state === "dev";
	const busy = up?.state === "checking" || up?.state === "downloading" || up?.state === "installing";

	return (
		<div className="flex flex-col gap-2">
			<Panel title={t("general.app.title")}>
				<SettingRow label={t("general.app.language")}>
					<Select
						size="xs"
						className="w-full"
						allowDeselect={false}
						value={g.language}
						data={Object.entries(LOCALES).map(([value, l]) => ({ value, label: l.label }))}
						onChange={(v) => v && update((s) => void (s.general.language = v as Language))}
					/>
				</SettingRow>
				<SettingRow label={t("general.app.autostart")} hint={t("general.app.autostartHint")}>
					<Switch checked={g.autostart} onChange={(e) => update((s) => void (s.general.autostart = e.currentTarget.checked))} />
				</SettingRow>
				<SettingRow label={t("general.app.startMinimized")} hint={t("general.app.startMinimizedHint")}>
					<Switch checked={g.startMinimized} onChange={(e) => update((s) => void (s.general.startMinimized = e.currentTarget.checked))} />
				</SettingRow>
				<SettingRow label={t("general.app.startBuffer")} hint={t("general.app.startBufferHint")}>
					<Switch checked={g.startBufferOnLaunch} onChange={(e) => update((s) => void (s.general.startBufferOnLaunch = e.currentTarget.checked))} />
				</SettingRow>
			</Panel>

			<Panel title={t("general.feedback.title")} hint={t("general.feedback.hint")}>
				<SettingRow label={t("general.feedback.toast")}>
					<Switch checked={g.toastOnSave} onChange={(e) => update((s) => void (s.general.toastOnSave = e.currentTarget.checked))} />
				</SettingRow>
				<SettingRow label={t("general.feedback.sound")} hint={t("general.feedback.soundHint")}>
					<Switch checked={g.saveSound} onChange={(e) => update((s) => void (s.general.saveSound = e.currentTarget.checked))} />
				</SettingRow>
				<SettingRow label={t("general.feedback.volume")} controlWidth="w-80">
					<div className="flex w-full items-center gap-3">
						<Slider className="flex-1" disabled={!g.saveSound} min={0} max={100} value={g.saveSoundVolume} label={(v) => `${v}%`} onChange={(v) => update((s) => void (s.general.saveSoundVolume = v))} />
						<Button size="xs" variant="light" disabled={!g.saveSound} leftSection={<IconVolume size={14} />} onClick={() => playSaveSound(g.saveSoundVolume)}>
							{t("action.test")}
						</Button>
					</div>
				</SettingRow>
			</Panel>

			<Panel title={t("general.update.title")} hint={t("general.update.hint")}>
				<SettingRow label={t("general.update.auto")} hint={t("general.update.autoHint")}>
					<Switch checked={g.autoUpdate} disabled={dev} onChange={(e) => update((s) => void (s.general.autoUpdate = e.currentTarget.checked))} />
				</SettingRow>
				<SettingRow
					label={dev || !up?.current ? t("general.update.dev") : `${t("general.update.version")}: ${t("general.update.build", { n: up.current })}`}
					hint={up && !dev ? t(`general.update.state.${up.state}`, { n: up.latest ?? "", pct: `${Math.round(up.progress * 100)}%`, error: up.error ?? "" }) : undefined}
					controlWidth="w-80"
				>
					<div className="flex w-full justify-end gap-2">
						{up?.state === "ready" ? (
							<Button size="xs" color="green" variant="light" leftSection={<IconDownload size={14} />} onClick={() => api.installUpdate()}>
								{t("general.update.install")}
							</Button>
						) : (
							<Button size="xs" variant="light" disabled={dev || busy} leftSection={<IconRefresh size={14} />} onClick={() => api.checkUpdate()}>
								{t("general.update.check")}
							</Button>
						)}
					</div>
				</SettingRow>
			</Panel>
		</div>
	);
}
