import { useCallback, useEffect, useState } from "react";
import { ActionIcon, Button, Tooltip } from "@mantine/core";
import { IconFolder, IconMovie, IconPlayerPlay, IconRefresh, IconTrash } from "@tabler/icons-react";
import { useSettings } from "../../store";
import { useT } from "../../i18n";
import { api } from "../../api";
import { formatBytes, formatDuration } from "../../utils/video";
import type { ClipInfo } from "../../types";
import Panel from "../shared/Panel";
import EmptyState from "../shared/EmptyState";

export default function ClipsPage() {
	const t = useT();
	const { settings, lastClip } = useSettings();
	const [clips, setClips] = useState<ClipInfo[]>([]);
	const [confirm, setConfirm] = useState<string | null>(null);

	const refresh = useCallback(() => {
		api.listClips().then(setClips).catch(() => setClips([]));
	}, []);

	useEffect(refresh, [refresh, lastClip, settings?.output.clipsDir]);

	const remove = async (path: string) => {
		await api.deleteClip(path);
		setConfirm(null);
		refresh();
	};

	return (
		<Panel
			title={t("clips.title")}
			hint={t("clips.hint", { n: clips.length, dir: settings?.output.clipsDir ?? "" })}
			right={
				<div className="flex gap-2">
					<Button size="xs" variant="light" color="gray" leftSection={<IconRefresh size={14} />} onClick={refresh}>
						{t("action.refresh")}
					</Button>
					<Button size="xs" variant="light" leftSection={<IconFolder size={14} />} onClick={() => settings && api.openPath(settings.output.clipsDir)}>
						{t("action.openFolder")}
					</Button>
				</div>
			}
		>
			{clips.length === 0 ?
				<EmptyState icon={<IconMovie size={16} />} title={t("clips.empty")} hint={t("clips.emptyHint", { key: settings?.hotkeys.saveClip ?? "" })} />
			:	<div className="grid grid-cols-3 gap-2">
					{clips.map((clip, i) => (
						<div key={clip.path} className="bank-row-in overflow-hidden rounded bg-(--mantine-color-dark-8) border border-(--mantine-color-dark-6)" style={{ ["--bank-delay" as string]: `${Math.min(i, 12) * 30}ms` }}>
							<button type="button" className="group relative block aspect-[21/9] w-full cursor-pointer bg-black" onClick={() => api.openPath(clip.path)}>
								{clip.thumbnail ?
									<img src={clip.thumbnail} alt="" className="h-full w-full object-cover" />
								:	<div className="flex h-full items-center justify-center text-(--mantine-color-dark-4)">
										<IconMovie size={28} />
									</div>
								}
								<div className="absolute inset-0 flex items-center justify-center bg-black/50 opacity-0 transition-opacity group-hover:opacity-100">
									<IconPlayerPlay size={28} color="white" />
								</div>
								<div className="absolute right-1.5 bottom-1.5 rounded px-1.5 py-0.5 text-[11px] font-semibold tabular-nums bg-black/70 text-white">{formatDuration(clip.durationSec)}</div>
							</button>
							<div className="flex items-center gap-2 px-2.5 py-2">
								<div className="min-w-0 flex-1">
									<div className="truncate text-xs font-semibold text-white">{clip.name}</div>
									<div className="text-[11px] tabular-nums text-(--mantine-color-dark-2)">
										{new Date(clip.createdAt * 1000).toLocaleString()} · {formatBytes(clip.sizeBytes)}
									</div>
								</div>
								<Tooltip label={t("action.showInFolder")} openDelay={300}>
									<ActionIcon size="sm" variant="subtle" color="gray" onClick={() => api.revealPath(clip.path)} aria-label={t("action.showInFolder")}>
										<IconFolder size={14} />
									</ActionIcon>
								</Tooltip>
								<Tooltip label={confirm === clip.path ? t("clips.confirmDelete") : t("action.delete")} openDelay={confirm === clip.path ? 0 : 300} opened={confirm === clip.path || undefined} color="red">
									<ActionIcon size="sm" variant={confirm === clip.path ? "filled" : "subtle"} color="red" onClick={() => (confirm === clip.path ? remove(clip.path) : setConfirm(clip.path))} onMouseLeave={() => confirm === clip.path && setConfirm(null)} aria-label={t("action.delete")}>
										<IconTrash size={14} />
									</ActionIcon>
								</Tooltip>
							</div>
						</div>
					))}
				</div>
			}
		</Panel>
	);
}
