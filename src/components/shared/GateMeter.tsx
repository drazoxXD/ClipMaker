import { useEffect, useRef, useState } from "react";
import { Button, Slider } from "@mantine/core";
import { IconHeadphones, IconHeadphonesOff, IconMicrophone, IconMicrophoneOff } from "@tabler/icons-react";
import { useT } from "../../i18n";
import type { MicLevel } from "../../types";

const MIN_DB = -90;
const HISTORY = 150; // ~10 s at the monitor's 15 updates per second
const GATE_OFF = -90;

const pct = (db: number) => Math.max(0, Math.min(100, ((db - MIN_DB) / -MIN_DB) * 100));

interface Sample {
	raw: number;
	db: number;
	open: boolean;
}

interface GateMeterProps {
	level: MicLevel | null;
	gateDb: number;
	disabled?: boolean;
	onGateChange: (db: number) => void;
	listening: boolean;
	onListenChange: (on: boolean) => void;
}

/**
 * The noise gate, made visible: live level against the threshold line, a scrolling history coloured
 * by whether the gate let you through, and the threshold slider on the same scale right underneath.
 */
export default function GateMeter({ level, gateDb, disabled, onGateChange, listening, onListenChange }: GateMeterProps) {
	const t = useT();
	const canvas = useRef<HTMLCanvasElement>(null);
	const history = useRef<Sample[]>([]);
	const [peakHold, setPeakHold] = useState(MIN_DB);
	const gateOn = gateDb > GATE_OFF;
	const db = level?.rmsDb ?? MIN_DB;
	const open = level?.gateOpen ?? false;

	// Peak marker that falls back slowly, like a real meter.
	useEffect(() => {
		if (!level) return;
		setPeakHold((p) => Math.max(level.peakDb, p - 1.5));
		history.current.push({ raw: level.inputDb, db: level.rmsDb, open: level.gateOpen });
		if (history.current.length > HISTORY) history.current.shift();
	}, [level]);

	// The history graph: one bar per update, green when heard, grey when gated, threshold dashed.
	useEffect(() => {
		const c = canvas.current;
		if (!c) return;
		const dpr = window.devicePixelRatio || 1;
		const w = c.clientWidth;
		const h = c.clientHeight;
		if (c.width !== w * dpr || c.height !== h * dpr) {
			c.width = w * dpr;
			c.height = h * dpr;
		}
		const g = c.getContext("2d");
		if (!g) return;
		const css = getComputedStyle(c);
		const color = (v: string) => css.getPropertyValue(v).trim();
		g.setTransform(dpr, 0, 0, dpr, 0, 0);
		g.clearRect(0, 0, w, h);

		const barW = w / HISTORY;
		const y = (d: number) => h - (pct(d) / 100) * h;
		const samples = history.current;
		samples.forEach((s, i) => {
			const x = w - (samples.length - i) * barW;
			// The raw input as a faint ghost behind: the gap is what noise suppression took away.
			g.fillStyle = color("--mantine-color-dark-5");
			g.fillRect(x, y(s.raw), Math.max(1, barW - 0.5), h - y(s.raw));
			g.fillStyle = s.open ? color("--mantine-color-green-6") : color("--mantine-color-dark-3");
			g.fillRect(x, y(s.db), Math.max(1, barW - 0.5), h - y(s.db));
		});

		if (gateOn) {
			g.strokeStyle = color("--mantine-color-yellow-5");
			g.setLineDash([4, 3]);
			g.lineWidth = 1;
			g.beginPath();
			g.moveTo(0, y(gateDb) + 0.5);
			g.lineTo(w, y(gateDb) + 0.5);
			g.stroke();
		}
	}, [level, gateDb, gateOn]);

	const status =
		disabled || !level ? { color: "dark", icon: IconMicrophoneOff, text: t("audio.mic.meterNoMic") }
		: !gateOn ? { color: "blue", icon: IconMicrophone, text: t("audio.mic.meterNoGate") }
		: open ? { color: "green", icon: IconMicrophone, text: t("audio.mic.meterOpen") }
		: { color: "gray", icon: IconMicrophoneOff, text: t("audio.mic.meterClosed") };
	const StatusIcon = status.icon;
	const statusBg = status.color === "green" ? "bg-[rgba(64,192,87,0.12)] border-(--mantine-color-green-8)" : status.color === "blue" ? "bg-(--bank-selected-bg) border-(--bank-selected-border)" : "bg-(--mantine-color-dark-8) border-(--mantine-color-dark-5)";
	const statusText = status.color === "green" ? "text-(--mantine-color-green-4)" : status.color === "blue" ? "text-(--bank-selected-text)" : "text-(--mantine-color-dark-1)";

	return (
		<div className={`flex flex-col gap-2 ${disabled ? "opacity-50" : ""}`}>
			<div className={`flex items-center gap-3 rounded border px-3 py-2 transition-colors duration-150 ${statusBg}`}>
				<StatusIcon size={20} className={statusText} />
				<div className={`flex-1 font-condensed text-lg font-bold uppercase tracking-[0.04em] ${statusText}`}>{status.text}</div>
				<Button size="xs" variant={listening ? "filled" : "light"} color={listening ? "green" : "gray"} disabled={disabled} leftSection={listening ? <IconHeadphones size={14} /> : <IconHeadphonesOff size={14} />} onClick={() => onListenChange(!listening)}>
					{t("audio.mic.listen")}
				</Button>
				<div className="text-right">
					<div className="text-sm font-bold tabular-nums text-white">{level ? `${db.toFixed(0)} dB` : "—"}</div>
					{gateOn && <div className="text-[11px] tabular-nums text-(--mantine-color-yellow-4)">{t("audio.mic.gateAt", { db: gateDb })}</div>}
				</div>
			</div>

			{/* Live bar on the same scale as the slider below: the muted zone left of the yellow line. */}
			<div className="relative h-5 overflow-hidden rounded bg-(--mantine-color-dark-8) border border-(--mantine-color-dark-6)">
				{gateOn && <div className="absolute inset-y-0 left-0 bg-[rgba(250,82,82,0.08)]" style={{ width: `${pct(gateDb)}%` }} />}
				<div className={`absolute inset-y-0 left-0 transition-[width] duration-75 ${open || !gateOn ? "bg-(--mantine-color-green-6)" : "bg-(--mantine-color-dark-3)"}`} style={{ width: `${level ? pct(db) : 0}%` }} />
				{level && <div className="absolute inset-y-0 w-0.5 bg-white/70" style={{ left: `${pct(peakHold)}%` }} />}
				{gateOn && <div className="absolute inset-y-0 w-0.5 bg-(--mantine-color-yellow-5)" style={{ left: `calc(${pct(gateDb)}% - 1px)` }} />}
			</div>

			<canvas ref={canvas} className="h-16 w-full rounded bg-(--mantine-color-dark-8) border border-(--mantine-color-dark-6)" />

			<Slider
				disabled={disabled}
				min={MIN_DB}
				max={0}
				value={gateDb}
				color="yellow"
				label={(v) => (v <= GATE_OFF ? t("audio.mic.gateOff") : `${v} dB`)}
				marks={[-90, -60, -40, -20, 0].map((v) => ({ value: v, label: v === -90 ? t("audio.mic.gateOff") : `${v}` }))}
				onChange={onGateChange}
				mb="md"
			/>
			<div className="text-xs text-(--mantine-color-dark-2)">{t("audio.mic.meterHint")}</div>
		</div>
	);
}
