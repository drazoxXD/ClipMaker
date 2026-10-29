import { CAPS_LABEL } from "./panelStyles";

const TONES = {
	dim: "text-(--mantine-color-dark-2)",
	body: "text-(--mantine-color-dark-0)",
	strong: "text-white",
} as const;

interface SectionLabelProps {
	children: React.ReactNode;
	className?: string;
	tone?: keyof typeof TONES;
}

export default function SectionLabel({ children, className, tone = "dim" }: SectionLabelProps) {
	return <div className={`${CAPS_LABEL} ${TONES[tone]} ${className ?? ""}`}>{children}</div>;
}
