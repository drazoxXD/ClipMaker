import type { CSSProperties } from "react";

export function notch(size = 10): { clipPath: string } {
	return {
		clipPath: `polygon(0 0, 100% 0, 100% calc(100% - ${size}px), calc(100% - ${size}px) 100%, 0 100%)`,
	};
}

export function notchEdge(size: number, color: string): CSSProperties {
	return {
		backgroundImage: `linear-gradient(to top left, transparent calc(50% - 1px), ${color} calc(50% - 1px), ${color} calc(50% + 1px), transparent calc(50% + 1px))`,
		backgroundSize: `${size}px ${size}px`,
		backgroundPosition: "bottom right",
		backgroundOrigin: "border-box",
		backgroundRepeat: "no-repeat",
	};
}
