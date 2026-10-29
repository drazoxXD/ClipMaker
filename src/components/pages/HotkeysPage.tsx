import { useState } from "react";
import { ActionIcon, Button } from "@mantine/core";
import { IconKeyboard, IconX } from "@tabler/icons-react";
import { useSettings } from "../../store";
import { useT } from "../../i18n";
import type { HotkeySettings } from "../../types";
import Panel from "../shared/Panel";
import SettingRow from "../shared/SettingRow";

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta"]);

/** Turns a keydown into a Tauri accelerator string such as "Ctrl+Shift+F8". */
function accelerator(e: React.KeyboardEvent): string | null {
	if (MODIFIER_KEYS.has(e.key)) return null;
	const parts: string[] = [];
	if (e.ctrlKey) parts.push("Ctrl");
	if (e.altKey) parts.push("Alt");
	if (e.shiftKey) parts.push("Shift");
	if (e.metaKey) parts.push("Super");

	let key = e.code;
	if (key.startsWith("Key")) key = key.slice(3);
	else if (key.startsWith("Digit")) key = key.slice(5);
	parts.push(key);
	return parts.join("+");
}

function HotkeyInput({ value, onChange }: { value: string; onChange: (v: string) => void }) {
	const t = useT();
	const [listening, setListening] = useState(false);

	return (
		<div className="flex w-full items-center gap-1.5">
			<Button
				size="xs"
				fullWidth
				variant={listening ? "light" : "default"}
				color={listening ? "blue" : "gray"}
				leftSection={<IconKeyboard size={14} />}
				onClick={() => setListening(true)}
				onBlur={() => setListening(false)}
				onKeyDown={(e) => {
					if (!listening) return;
					e.preventDefault();
					if (e.key === "Escape") return setListening(false);
					const acc = accelerator(e);
					if (acc) {
						onChange(acc);
						setListening(false);
					}
				}}
			>
				{listening ? t("hotkeys.press") : value || t("hotkeys.none")}
			</Button>
			<ActionIcon variant="light" color="gray" disabled={!value} onClick={() => onChange("")} aria-label={t("action.clear")}>
				<IconX size={14} />
			</ActionIcon>
		</div>
	);
}

export default function HotkeysPage() {
	const t = useT();
	const { settings, update } = useSettings();
	if (!settings) return null;

	const row = (key: keyof HotkeySettings) => (
		<SettingRow key={key} label={t(`hotkeys.${key}`)} hint={t(`hotkeys.${key}Hint`)} controlWidth="w-72">
			<HotkeyInput value={settings.hotkeys[key]} onChange={(v) => update((s) => void (s.hotkeys[key] = v))} />
		</SettingRow>
	);

	return (
		<Panel title={t("hotkeys.title")} hint={t("hotkeys.hint")}>
			{row("saveClip")}
			{row("toggleBuffer")}
			{row("openWindow")}
		</Panel>
	);
}
