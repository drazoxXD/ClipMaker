import type { ReactNode } from "react";
import SectionLabel from "./SectionLabel";

interface StatTileProps {
	label: string;
	icon?: ReactNode;
	children: ReactNode;
}

export default function StatTile({ label, icon, children }: StatTileProps) {
	return (
		<div className="flex items-center gap-3 rounded px-3 py-2 bg-(--mantine-color-dark-8) border border-(--mantine-color-dark-6)">
			{icon}
			<div className="min-w-0 flex-1">
				<SectionLabel>{label}</SectionLabel>
				<div className="mt-1">{children}</div>
			</div>
		</div>
	);
}
