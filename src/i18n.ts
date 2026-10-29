import { useCallback } from "react";
import en from "./locales/en.json";
import hu from "./locales/hu.json";
import { useSettings } from "./store";
import type { Language } from "./types";

type Dict = { [key: string]: string | Dict };

/** Every shipped locale. To add one: drop a JSON next to en.json and list it here. */
export const LOCALES: Record<Language, { label: string; dict: Dict }> = {
	en: { label: "English", dict: en },
	hu: { label: "Magyar", dict: hu },
};

function lookup(dict: Dict, key: string): string | undefined {
	let node: string | Dict | undefined = dict;
	for (const part of key.split(".")) {
		if (typeof node !== "object") return undefined;
		node = node[part];
	}
	return typeof node === "string" ? node : undefined;
}

export function translate(lang: Language, key: string, vars?: Record<string, string | number>): string {
	const text = lookup(LOCALES[lang]?.dict ?? en, key) ?? lookup(en, key) ?? key;
	if (!vars) return text;
	return text.replace(/\{(\w+)\}/g, (_, name: string) => String(vars[name] ?? `{${name}}`));
}

export function useT() {
	const { settings } = useSettings();
	const lang = settings?.general.language ?? "en";
	return useCallback((key: string, vars?: Record<string, string | number>) => translate(lang, key, vars), [lang]);
}
