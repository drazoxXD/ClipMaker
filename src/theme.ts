import { ActionIcon, Badge, Button, createTheme, mergeThemeOverrides, SegmentedControl, ThemeIcon, type CSSProperties } from "@mantine/core";
import { notch, notchEdge } from "./utils/notch";

const BUTTON_NOTCH_SIZE = 7;

function notchedRoot(props: { variant?: string; disabled?: boolean; radius?: unknown }): { root: CSSProperties } {
	if (props.radius === "xl") return { root: {} };

	const root: CSSProperties = notch(BUTTON_NOTCH_SIZE);

	if (props.variant === "default" && !props.disabled) {
		Object.assign(root, notchEdge(BUTTON_NOTCH_SIZE, "var(--mantine-color-default-border)"));
	}

	return { root };
}

// Mantine's dimmed grey (#828282) only reaches ~4:1 on the dark panels; this keeps the
// same step below dark-1 while clearing 4.5:1 on dark-6 through dark-9.
const DARK = ["#C9C9C9", "#B8B8B8", "#999999", "#696969", "#424242", "#3B3B3B", "#2E2E2E", "#242424", "#1F1F1F", "#141414"] as const;

const FONT_FAMILY = '"Barlow", system-ui, sans-serif';

export const theme = createTheme({
	primaryColor: "blue",
	focusRing: "never",
	fontFamily: FONT_FAMILY,
	headings: { fontFamily: FONT_FAMILY },
	colors: { dark: [...DARK] },
	components: {
		Button: Button.extend({ styles: (_theme, props) => notchedRoot(props) }),
		ActionIcon: ActionIcon.extend({ styles: (_theme, props) => notchedRoot(props) }),
		ThemeIcon: ThemeIcon.extend({ defaultProps: { radius: "sm" } }),
		// Mantine's xs and sm badges render 9px and 10px text; status words need 11px to stay legible at 1080p.
		Badge: Badge.extend({ defaultProps: { radius: "sm" }, vars: (_theme, props) => (props.size === "xs" || props.size === "sm" ? { root: { "--badge-fz": "11px", "--badge-height": "18px" } } : { root: {} }) }),
		SegmentedControl: SegmentedControl.extend({
			defaultProps: { radius: "sm", color: "blue", transitionDuration: 350, transitionTimingFunction: "cubic-bezier(0.22, 1, 0.36, 1)" },
			styles: {
				root: { backgroundColor: "var(--mantine-color-dark-8)", border: "1px solid var(--mantine-color-dark-6)" },
				label: { fontSize: 12, fontWeight: 600, letterSpacing: "0.025em", textTransform: "uppercase" },
			},
		}),
	},
});

const BUTTON_LABEL: CSSProperties = { fontSize: 12, fontWeight: 600, textTransform: "uppercase", letterSpacing: "0.025em" };

export const windowTheme = mergeThemeOverrides(
	theme,
	createTheme({
		components: {
			Button: Button.extend({ styles: (_theme, props) => ({ ...notchedRoot(props), label: BUTTON_LABEL }) }),
		},
	}),
);
