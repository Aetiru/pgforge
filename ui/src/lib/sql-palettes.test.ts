import { describe, expect, it } from "vitest";

import { PALETTES, SQL_TOKENS, defaultPrefs, paletteCss, parsePrefs } from "./sql-palettes";

describe("las paletas", () => {
  it("todas traen todos los colores, en claro y en oscuro, y son hexadecimales", () => {
    for (const palette of PALETTES) {
      for (const theme of ["light", "dark"] as const) {
        for (const { key } of SQL_TOKENS) {
          expect(palette[theme][key], `${palette.id}.${theme}.${key}`).toMatch(/^#[0-9a-f]{6}$/i);
        }
      }
    }
  });

  it("los identificadores no se repiten entre paletas", () => {
    expect(new Set(PALETTES.map((p) => p.id)).size).toBe(PALETTES.length);
  });
});

describe("parsePrefs", () => {
  it("sin nada guardado devuelve lo de fábrica", () => {
    expect(parsePrefs(null)).toEqual(defaultPrefs());
  });

  it("una paleta que no existe vuelve a «actual»", () => {
    expect(parsePrefs({ palette: "inventada" }).palette).toBe("actual");
  });

  it("descarta colores que no son hexadecimales y conserva el resto", () => {
    const prefs = parsePrefs({
      palette: "custom",
      custom: { light: { keyword: "red; } body { display:none", string: "#112233" } },
    });
    expect(prefs.custom.light.keyword).toBe(defaultPrefs().custom.light.keyword);
    expect(prefs.custom.light.string).toBe("#112233");
  });

  it("un peso fuera de los permitidos no llega al CSS", () => {
    expect(parsePrefs({ keywordWeight: 900 }).keywordWeight).toBe(700);
    expect(parsePrefs({ keywordWeight: 400 }).keywordWeight).toBe(400);
  });
});

describe("paletteCss", () => {
  it("escribe las dos paletas y los ajustes", () => {
    const css = paletteCss({ ...defaultPrefs(), keywordWeight: 500, commentItalic: false });
    expect(css).toContain("html:root");
    expect(css).toContain('html[data-theme="dark"]');
    expect(css).toContain("--cm-keyword-weight: 500");
    expect(css).toContain("--cm-comment-style: normal");
  });

  it("escribe el bloque de Papel con los colores propios de la paleta", () => {
    const css = paletteCss({ ...defaultPrefs(), palette: "papel" });
    const paper = css.split('html[data-theme="paper"]')[1];
    expect(paper).toContain("--cm-comment: #6f6657");
  });

  it("una paleta sin colores propios de Papel usa los del claro, y Personalizada los suyos", () => {
    const base = defaultPrefs();
    const light = PALETTES[0].light;
    const paper = paletteCss(base).split('html[data-theme="paper"]')[1];
    expect(paper).toContain(`--cm-keyword: ${light.keyword}`);

    const custom = { ...base, palette: "custom", custom: { ...base.custom, light: { ...light, keyword: "#123456" } } };
    expect(paletteCss(custom).split('html[data-theme="paper"]')[1]).toContain("--cm-keyword: #123456");
  });

  it("sin tinte, los nombres toman el color del texto", () => {
    expect(paletteCss({ ...defaultPrefs(), identTint: false })).toContain("--cm-ident: var(--cm-text)");
    expect(paletteCss({ ...defaultPrefs(), identTint: true })).not.toContain("--cm-ident: var(--cm-text)");
  });
});
