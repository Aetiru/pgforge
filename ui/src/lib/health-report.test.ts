import { describe, expect, it } from "vitest";
import { buildReport, toHtml, toMarkdown, type ReportInput } from "./health-report";

const base: ReportInput = {
  generatedAt: new Date("2026-03-01T10:00:00Z"),
  server: "Producción",
  database: "ventas",
  health: { level: "warn", issues: [{ level: "warn", text: "Conexiones al 80 %" }] },
  metrics: null,
  connections: null,
  sequences: [
    {
      schema: "public",
      name: "pedidos_id_seq",
      dataType: "integer",
      lastValue: 1_900_000_000,
      maxValue: 2_147_483_647,
      incrementBy: 1,
      cycle: false,
      usedRatio: 0.88,
      remaining: 247_483_647,
      level: "review",
      perDay: 500_000,
      daysLeft: 495,
    },
    {
      schema: "public",
      name: "otra_seq",
      dataType: "bigint",
      lastValue: 10,
      maxValue: 9e18,
      incrementBy: 1,
      cycle: false,
      usedRatio: 0,
      remaining: 9e18,
      level: "ok",
      perDay: null,
      daysLeft: null,
    },
  ],
  wraparound: null,
  autovacuum: null,
  cache: null,
  space: null,
  checkpoints: null,
  config: null,
  unavailable: [["Caché", "permiso denegado"]],
};

describe("informe de salud", () => {
  it("solo incluye lo que pide algo", () => {
    const sections = buildReport(base);
    const titles = sections.map((s) => s.title);
    expect(titles).toEqual(["Resumen", "Secuencias por agotarse", "Lo que no se pudo leer"]);
    expect(sections[1].table?.rows).toHaveLength(1);
  });

  it("dice lo que no se pudo leer en vez de omitirlo", () => {
    expect(toMarkdown(base)).toContain("Caché: permiso denegado");
  });

  it("escribe el veredicto y las tablas en Markdown", () => {
    const md = toMarkdown(base);
    expect(md).toContain("# Informe de salud · Producción · ventas");
    expect(md).toContain("Hay cosas para revisar.");
    expect(md).toContain("| public.pedidos_id_seq | integer | 88.0 % |");
  });

  it("escapa el HTML y las barras de las celdas de Markdown", () => {
    const hostile: ReportInput = {
      ...base,
      server: "<img src=x onerror=alert(1)>",
      unavailable: [["a|b", "x\ny"]],
    };
    expect(toHtml(hostile)).not.toContain("<img");
    expect(toHtml(hostile)).toContain("&lt;img");
    const md = toMarkdown({
      ...hostile,
      sequences: [{ ...base.sequences![0], name: "a|b" }],
    });
    expect(md).toContain("a\\|b");
  });

  it("el HTML es un archivo solo, sin recursos externos", () => {
    const html = toHtml(base);
    expect(html.startsWith("<!doctype html>")).toBe(true);
    expect(html).not.toMatch(/https?:\/\//);
    expect(html).toContain("prefers-color-scheme: dark");
  });
});
