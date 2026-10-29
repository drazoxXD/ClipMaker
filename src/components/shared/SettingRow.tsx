import type { ReactNode } from "react";
import SectionLabel from "./SectionLabel";

interface SettingRowProps {
	label: string;
	hint?: string;
	children: ReactNode;
	/** Width of the control column; wide controls (sliders) want more. */
	controlWidth?: string;
}

export default function SettingRow({ label, hint, children, controlWidth = "w-64" }: SettingRowProps) {
	return (
		<div className="flex items-center justify-between gap-4">
			<div className="min-w-0 flex-1">
				<SectionLabel tone="body">{label}</SectionLabel>
				{hint && <div className="mt-0.5 text-xs text-(--mantine-color-dark-2)">{hint}</div>}
			</div>
			<div className={`flex shrink-0 justify-end ${controlWidth}`}>{children}</div>
		</div>
	);
}
