import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
	plugins: [react(), tailwindcss()],
	clearScreen: false,
	// Pinned to IPv4: on this machine "localhost" made Vite listen on ::1 only, which WebView2 can't reach.
	server: { host: "127.0.0.1", port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
	envPrefix: ["VITE_", "TAURI_"],
	build: {
		target: "chrome120",
		outDir: "dist",
		// The main window and the click-through notification overlay.
		rollupOptions: { input: { main: "index.html", overlay: "overlay.html" } },
	},
});
