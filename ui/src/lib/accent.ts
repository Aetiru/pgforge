/** Los acentos disponibles y la lectura defensiva de lo guardado; sin DOM, para poder probarse. */

import { t } from "./i18n.svelte";
export type Accent = "blue" | "violet" | "teal" | "orange";

export const ACCENTS: { id: Accent; label: string; swatch: string }[] = [
  { id: "blue", get label() { return t("Azul"); }, swatch: "#2563eb" },
  { id: "violet", get label() { return t("Violeta"); }, swatch: "#7c3aed" },
  { id: "teal", get label() { return t("Verde azulado"); }, swatch: "#0d9488" },
  { id: "orange", get label() { return t("Naranja"); }, swatch: "#ea580c" },
];

export function parseAccent(raw: string | null): Accent {
  return ACCENTS.some((item) => item.id === raw) ? (raw as Accent) : "blue";
}

