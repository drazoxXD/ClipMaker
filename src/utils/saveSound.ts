/**
 * The clip-saved chime, synthesised in the webview. The webview runs under the recorder's own
 * process tree, which every capture excludes, so the chime is heard by the user but never recorded.
 */
export function playSaveSound(volume: number) {
	const ctx = new AudioContext();
	const gain = ctx.createGain();
	gain.connect(ctx.destination);
	const level = Math.max(0, Math.min(1, volume / 100)) * 0.35;

	[880, 1320].forEach((freq, i) => {
		const osc = ctx.createOscillator();
		const env = ctx.createGain();
		osc.type = "sine";
		osc.frequency.value = freq;
		const start = ctx.currentTime + i * 0.09;
		env.gain.setValueAtTime(0, start);
		env.gain.linearRampToValueAtTime(level, start + 0.01);
		env.gain.exponentialRampToValueAtTime(0.0001, start + 0.25);
		osc.connect(env).connect(gain);
		osc.start(start);
		osc.stop(start + 0.3);
	});

	window.setTimeout(() => ctx.close(), 600);
}
