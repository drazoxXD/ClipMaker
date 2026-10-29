import type { ComponentType } from "react";
import { IconRefresh } from "@tabler/icons-react";

interface RingLoaderProps {
	size?: number;
	color?: string;
	innerColor?: string;
	icon?: ComponentType<{ size?: number; stroke?: number; color?: string }>;
}

export default function RingLoader({ size = 44, color = "var(--bank-accent)", innerColor = "var(--bank-accent-strong)", icon: Icon = IconRefresh }: RingLoaderProps) {
	const strokeWidth = size * 0.06;
	const radius = (size - strokeWidth) / 2;
	const innerRadius = radius * 0.6;
	const circumference = 2 * Math.PI * radius;
	const dashLength = circumference * 0.25;

	return (
		<div className="relative shrink-0" style={{ width: size, height: size }}>
			<svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} style={{ transform: "rotate(-90deg)" }}>
				<circle cx={size / 2} cy={size / 2} r={radius} fill="none" strokeWidth={strokeWidth} opacity={0.15} style={{ stroke: "var(--mantine-color-dark-4)" }} />
				<circle cx={size / 2} cy={size / 2} r={innerRadius} opacity={0.15} style={{ fill: innerColor }} />
				<circle
					cx={size / 2}
					cy={size / 2}
					r={radius}
					fill="none"
					strokeWidth={strokeWidth}
					strokeDasharray={`${dashLength} ${circumference - dashLength}`}
					strokeLinecap="round"
					style={{
						stroke: color,
						transformBox: "fill-box",
						transformOrigin: "center",
						animation: "loader-spin 1.5s linear infinite",
					}}
				/>
			</svg>

			<div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2">
				<Icon size={innerRadius * 1.4} stroke={1.5} color={color} />
			</div>
		</div>
	);
}
