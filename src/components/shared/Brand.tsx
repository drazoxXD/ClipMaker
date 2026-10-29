import { IconPlayerRecordFilled } from "@tabler/icons-react";

export default function Brand() {
	return (
		<div className="flex items-center gap-2.5">
			<div className="flex h-8 w-8 items-center justify-center rounded bg-(--bank-selected-bg) border border-(--bank-selected-border)">
				<IconPlayerRecordFilled size={16} color="var(--mantine-color-red-5)" />
			</div>
			<div className="leading-none">
				<div className="font-condensed text-lg font-bold uppercase tracking-[0.06em] text-white">Drazox</div>
				<div className="text-[10px] font-semibold uppercase tracking-[0.18em] text-(--mantine-color-dark-2)">Recorder</div>
			</div>
		</div>
	);
}
