import type { ReactNode } from "react";
import { Alert } from "@mantine/core";
import { IconAlertTriangle, IconInfoCircle } from "@tabler/icons-react";

const TONES = {
	warning: { color: "yellow", icon: IconAlertTriangle },
	info: { color: "blue", icon: IconInfoCircle },
} as const;

interface NoticeProps {
	tone?: keyof typeof TONES;
	size?: "xs" | "sm";
	children: ReactNode;
	className?: string;
}

export default function Notice({ tone = "warning", size = "sm", children, className }: NoticeProps) {
	const { color, icon: Icon } = TONES[tone];

	return (
		<Alert color={color} icon={<Icon size={14} />} p="xs" className={className} styles={{ message: { fontSize: `var(--mantine-font-size-${size})`, color: "var(--alert-color)" } }}>
			{children}
		</Alert>
	);
}
