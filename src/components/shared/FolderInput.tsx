import { ActionIcon, TextInput, Tooltip } from "@mantine/core";
import { IconFolderOpen, IconExternalLink } from "@tabler/icons-react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../../api";
import { useT } from "../../i18n";

interface FolderInputProps {
	value: string;
	onChange: (path: string) => void;
	disabled?: boolean;
}

export default function FolderInput({ value, onChange, disabled }: FolderInputProps) {
	const t = useT();

	const pick = async () => {
		const dir = await open({ directory: true, defaultPath: value || undefined });
		if (typeof dir === "string") onChange(dir);
	};

	return (
		<div className="flex w-full items-center gap-1.5">
			<TextInput size="xs" className="flex-1" readOnly disabled={disabled} value={value} onClick={disabled ? undefined : pick} styles={{ input: { cursor: "pointer", fontFamily: "monospace" } }} />
			<Tooltip label={t("action.browse")} openDelay={300}>
				<ActionIcon variant="light" disabled={disabled} onClick={pick} aria-label={t("action.browse")}>
					<IconFolderOpen size={14} />
				</ActionIcon>
			</Tooltip>
			<Tooltip label={t("action.openFolder")} openDelay={300}>
				<ActionIcon variant="light" color="gray" disabled={disabled || !value} onClick={() => api.openPath(value)} aria-label={t("action.openFolder")}>
					<IconExternalLink size={14} />
				</ActionIcon>
			</Tooltip>
		</div>
	);
}
