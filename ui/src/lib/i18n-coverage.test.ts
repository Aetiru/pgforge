import { describe, expect, it } from "vitest";
import { en } from "./locales/en";

/**
 * Cada `t("…")` con texto literal tiene que tener su traducción al inglés. Es el único freno contra
 * dejar una pantalla a medio traducir sin enterarse: una entrada que falta se ve en español y
 * nada más falla. Lee los fuentes con `import.meta.glob` (sin tipos de Node en la interfaz).
 */
const SOURCES = import.meta.glob<string>("/src/**/*.{svelte,ts}", {
  query: "?raw",
  import: "default",
  eager: true,
});

function counted(path: string): boolean {
  return !/\.test\.ts$/.test(path) && !/\/i18n[^/]*$/.test(path) && !path.includes("/locales/");
}

// Texto literal entre comillas dobles, simples o invertidas (sin `${}`), como primer argumento.
const CALL = /(?<![\w.$])t\(\s*(?:"((?:\\.|[^"\\])*)"|'((?:\\.|[^'\\])*)'|`((?:\\.|[^`\\$])*)`)/g;

function unescape(text: string): string {
  return text.replace(/\\(["'`\\])/g, "$1").replace(/\\n/g, "\n");
}

describe("cobertura de traducciones", () => {
  it("todo t() literal tiene entrada en inglés", () => {
    const missing: string[] = [];
    for (const [file, text] of Object.entries(SOURCES)) {
      if (!counted(file)) continue;
      for (const match of text.matchAll(CALL)) {
        const key = unescape(match[1] ?? match[2] ?? match[3] ?? "");
        if (!(key in en)) missing.push(`${file}: ${key}`);
      }
    }
    expect(missing).toEqual([]);
  });

  it("ninguna entrada en inglés queda vacía", () => {
    const bad = Object.entries(en)
      .filter(([, value]) => value.trim() === "")
      .map(([source]) => source);
    expect(bad).toEqual([]);
  });

  it("los parámetros de cada traducción coinciden con los del original", () => {
    const names = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",");
    const bad = Object.entries(en).filter(([es, value]) => names(es) !== names(value));
    expect(bad.map(([es]) => es)).toEqual([]);
  });
});
