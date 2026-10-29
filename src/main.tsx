import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@mantine/core/styles.css";
import "./index.css";
import { MantineProvider } from "@mantine/core";
import App from "./App";
import { windowTheme } from "./theme";
import { SettingsProvider } from "./store";

createRoot(document.getElementById("root")!).render(
	<StrictMode>
		<MantineProvider defaultColorScheme="dark" forceColorScheme="dark" theme={windowTheme} cssVariablesResolver={() => ({ variables: {}, light: {}, dark: { "--mantine-color-placeholder": "#959595" } })}>
			<SettingsProvider>
				<App />
			</SettingsProvider>
		</MantineProvider>
	</StrictMode>,
);
