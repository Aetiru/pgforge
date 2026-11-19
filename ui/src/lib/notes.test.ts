import { describe, expect, it } from "vitest";
import { isEmpty, matchesNote, noteHint, normalizeTags, noteKey, parseNotes } from "./notes";

describe("notas de objetos", () => {
  it("las etiquetas se limpian, sin # ni repetidas", () => {
    expect(normalizeTags("#Facturación, #no-tocar facturación  #X")).toEqual(["facturación", "no-tocar", "x"]);
    expect(normalizeTags("   ")).toEqual([]);
  });

  it("hay un tope de etiquetas", () => {
    const many = Array.from({ length: 20 }, (_, i) => `t${i}`).join(" ");
    expect(normalizeTags(many)).toHaveLength(10);
  });

  it("la clave distingue servidor, base, esquema y nombre", () => {
    expect(noteKey("a", "db", "public", "t")).not.toBe(noteKey("a", "db", "otro", "t"));
    expect(noteKey("a", "db", "public", "t")).not.toBe(noteKey("b", "db", "public", "t"));
  });

  it("lo guardado se sanea", () => {
    const map = parseNotes(JSON.stringify({ k: { note: "hola", tags: ["#A", 3, "a"] }, malo: 3, vacio: { note: "", tags: [] } }));
    expect(map).toEqual({ k: { note: "hola", tags: ["a"] } });
    expect(parseNotes("no es json")).toEqual({});
    expect(parseNotes(null)).toEqual({});
  });

  it("vacío es sin texto y sin etiquetas", () => {
    expect(isEmpty({ note: "  ", tags: [] })).toBe(true);
    expect(isEmpty({ note: "", tags: ["x"] })).toBe(false);
  });

  it("busca por etiqueta (prefijo) o por el texto de la nota", () => {
    const note = { note: "Se llena desde el ETL de las 02:00", tags: ["facturación", "no-tocar"] };
    expect(matchesNote(note, "#fact")).toBe(true);
    expect(matchesNote(note, "#NO-")).toBe(true);
    expect(matchesNote(note, "#otra")).toBe(false);
    expect(matchesNote(note, "etl")).toBe(true);
    expect(matchesNote(note, "")).toBe(false);
    expect(matchesNote(null, "etl")).toBe(false);
    expect(matchesNote(note, "#")).toBe(false);
  });

  it("la pista junta etiquetas y un recorte de la nota", () => {
    expect(noteHint({ note: "hola   mundo", tags: ["a", "b"] })).toBe("#a #b · hola mundo");
    expect(noteHint(null)).toBe("");
  });
});
