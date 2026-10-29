import { useT } from "../../i18n";
import type { MicLevel } from "../../types";

const MIN_DB = -90;
const pct = (db: number) => Math.max(0, Math.min(100, ((db - MIN_DB) / -MIN_DB) * 100));

function Bar({ label, db, color }: { label: string; db: number | null; color: string }) {
	return (
		<div className="flex items-center gap-2">
			<div className="w-28 shrink-0 text-[11px] font-semibold uppercase tracking-[0.08em] text-(--mantine-color-dark-1)">{label}</div>
			<div className="relative h-2.5 flex-1 overflow-hidden rounded-full bg-(--mantine-color-dark-8) border border-(--mantine-color-dark-6)">
				<div className="absolute inset-y-0 left-0 rounded-full transition-[width] duration-75" style={{ width: `${db === null ? 0 : pct(db)}%`, background: color }} />
			</div>
			<div className="w-14 shrink-0 text-right text-xs font-semibold tabular-nums text-white">{db === null ? "—" : `${db.toFixed(0)} dB`}</div>
		</div>
	);
}

/** What noise suppression is doing right now: input vs. output level, and RNNoise's voice detection. */
export default function SuppressionMeter({ level }: { level: MicLevel | null }) {
	const t = useT();
	const removed = level ? Math.max(0, level.inputDb - level.rmsDb) : 0;
	const voice = level?.voice ?? null;

	return (
		<div className="flex flex-col gap-1.5 rounded px-3 py-2.5 bg-(--mantine-color-dark-8) border border-(--mantine-color-dark-6)">
			<Bar label={t("audio.mic.meterInput")} db={level?.inputDb ?? null} color="var(--mantine-color-dark-2)" />
			<Bar label={t("audio.mic.meterAfter")} db={level?.rmsDb ?? null} color="var(--bank-accent)" />
			<div className="flex items-center gap-2">
				<div className="w-28 shrink-0 text-[11px] font-semibold uppercase tracking-[0.08em] text-(--mantine-color-dark-1)">{t("audio.mic.meterVoice")}</div>
				<div className="relative h-2.5 flex-1 overflow-hidden rounded-full bg-(--mantine-color-dark-8) border border-(--mantine-color-dark-6)">
					<div className="absolute inset-y-0 left-0 rounded-full transition-[width] duration-100" style={{ width: `${(voice ?? 0) * 100}%`, background: (voice ?? 0) > 0.5 ? "var(--mantine-color-green-6)" : "var(--mantine-color-dark-3)" }} />
				</div>
				<div className="w-14 shrink-0 text-right text-xs font-semibold tabular-nums text-white">{voice === null ? "—" : `${Math.round(voice * 100)}%`}</div>
			</div>
			<div className="text-xs text-(--mantine-color-dark-2)">{level ? t("audio.mic.meterRemoved", { db: removed.toFixed(0) }) : t("audio.mic.meterNoMic")}</div>
		</div>
	);
}
