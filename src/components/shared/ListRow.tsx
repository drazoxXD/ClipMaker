import type { ReactNode } from "react";
import { notch } from "../../utils/notch";

const ROW_NOTCH = notch(10);

const SURFACES = {
	shell: "bg-(--mantine-color-dark-7)",
	card: "bg-(--mantine-color-dark-8)",
} as const;

interface ListRowProps {
	active: boolean;
	nav?: boolean;
	onClick: () => void;
	surface?: keyof typeof SURFACES;
	className?: string;
	children: ReactNode;
}

/** A clickable list entry, the bank's copy of the ls_phone ListRow: a dark left edge that lightens on hover, notched corner. A picked list row turns grey, a picked nav item blue. */
export default function ListRow({ active, nav = false, onClick, surface = "card", className, children }: ListRowProps) {
	const picked = nav ? "bg-(--bank-selected-bg) border-l-(--bank-selected-mark)" : "bg-(--bank-row-selected-bg) border-l-(--bank-row-selected-edge)";
	const tone = active ? picked : `${SURFACES[surface]} border-l-(--mantine-color-dark-6) hover:bg-(--mantine-color-dark-6) hover:border-l-(--mantine-color-dark-4)`;

	return (
		<button type="button" style={ROW_NOTCH} className={`flex w-full cursor-pointer items-center rounded-tr border-l-2 text-left transition-colors duration-220 ease-out ${tone} ${className ?? ""}`} onClick={onClick}>
			{children}
		</button>
	);
}
