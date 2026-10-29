import { Badge } from "@mantine/core";
import type { RecorderState } from "../../types";
import { useT } from "../../i18n";

const COLORS: Record<RecorderState, string> = {
	idle: "gray",
	starting: "yellow",
	buffering: "red",
	saving: "blue",
	error: "orange",
};

export default function StatePill({ state }: { state: RecorderState }) {
	const t = useT();
	return (
		<Badge variant="light" color={COLORS[state]} size="sm" leftSection={state === "buffering" ? <span className="rec-pulse inline-block h-1.5 w-1.5 rounded-full bg-(--mantine-color-red-5)" /> : undefined}>
			{t(`state.${state}`)}
		</Badge>
	);
}
