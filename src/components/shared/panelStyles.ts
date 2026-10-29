export const PANEL_CARD_STYLES = {
	root: {
		backgroundColor: "var(--mantine-color-dark-7)",
		borderColor: "var(--mantine-color-dark-6)",
	},
} as const;

export const DANGER_CARD_STYLES = {
	root: {
		backgroundColor: "var(--mantine-color-dark-7)",
		borderColor: "var(--mantine-color-red-3)",
	},
} as const;

export const CARD_TITLE = "font-condensed text-[15px] font-semibold uppercase leading-tight tracking-[0.04em]";

export const PANEL_HEADING = "font-condensed text-lg font-bold uppercase leading-tight tracking-[0.03em]";

export const CAPS_LABEL = "text-[11px] font-semibold uppercase tracking-[0.08em]";

export const HERO_VALUE = "text-xl leading-none font-bold tabular-nums";

export const STAT_VALUE = "text-lg leading-none font-bold tabular-nums";

export const SCORE_TRACK_FILL = "linear-gradient(90deg, var(--mantine-color-yellow-7), var(--mantine-color-yellow-5))";

export const CONTROL_HEIGHT = 30;

export const CONTROL_LABEL = {
	fontSize: 11,
	fontWeight: 600,
	letterSpacing: "0.08em",
	textTransform: "uppercase" as const,
};

export const SEGMENT_STYLES = {
	root: { height: CONTROL_HEIGHT },
	label: { display: "flex", alignItems: "center", justifyContent: "center", height: "100%", paddingBlock: 0, lineHeight: 1 },
};
