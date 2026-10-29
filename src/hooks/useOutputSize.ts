import { useSettings } from "../store";
import { outputSize } from "../utils/video";

/** The selected monitor and the clip size its resolution setting produces. */
export function useOutputSize() {
	const { settings, monitors } = useSettings();
	const monitor = monitors.find((m) => m.index === settings?.video.monitor) ?? monitors.find((m) => m.primary) ?? monitors[0];
	const monW = monitor?.width ?? 1920;
	const monH = monitor?.height ?? 1080;
	const size = settings ? outputSize(settings.video.resolution, monW, monH) : { width: monW, height: monH };
	return { monitor, monW, monH, ...size };
}
