/** Los acentos disponibles y la lectura defensiva de lo guardado; sin DOM, para poder probarse. */
export type Accent = "blue" | "violet" | "teal" | "orange";

export const ACCENTS: { id: Accent; label: string; swatch: string }[] = [
  { id: "blue", label: "Azul", swatch: "#2563eb" },
  { id: "violet", label: "Violeta", swatch: "#7c3aed" },
  { id: "teal", label: "Verde azulado", swatch: "#0d9488" },
  { id: "orange", label: "Naranja", swatch: "#ea580c" },
];

export function parseAccent(raw: string | null): Accent {
  return ACCENTS.some((item) => item.id === raw) ? (raw as Accent) : "blue";
}

