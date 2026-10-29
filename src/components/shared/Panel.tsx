import type { ReactNode } from "react";
import { Card } from "@mantine/core";
import { CARD_TITLE, PANEL_CARD_STYLES } from "./panelStyles";
import { HorizontalDivider } from "./HorizontalDivider";

interface PanelProps {
	title: string;
	hint?: string;
	right?: ReactNode;
	children: ReactNode;
}

/** A settings card in the bank's panel style: condensed caps title, dim hint, divider, then rows. */
export default function Panel({ title, hint, right, children }: PanelProps) {
	return (
		<Card withBorder p={12} styles={PANEL_CARD_STYLES}>
			<div className="flex items-start justify-between gap-3">
				<div className="min-w-0">
					<div className={`${CARD_TITLE} text-white`}>{title}</div>
					{hint && <div className="mt-0.5 text-xs text-(--mantine-color-dark-0)">{hint}</div>}
				</div>
				{right}
			</div>
			<div className="my-3">
				<HorizontalDivider />
			</div>
			<div className="flex flex-col gap-3">{children}</div>
		</Card>
	);
}
