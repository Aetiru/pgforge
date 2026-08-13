import { describe, expect, it } from "vitest";
import { filterHits, matchesKind, parseQuery } from "./tree-query";
import type { SearchHit } from "./ipc";

describe("parseQuery", () => {
  it("un prefijo conocido acota y sale del texto", () => {
    expect(parseQuery("t:factura")).toEqual({
      text: "factura",
      kinds: ["table", "partitionedTable", "foreignTable"],
    });
  });

  it("el prefijo solo lista la familia entera", () => {
    expect(parseQuery("s:")).toEqual({ text: "", kinds: ["schema"] });
  });

  it("una letra que no es prefijo queda como texto: un nombre puede tener dos puntos", () => {
    expect(parseQuery("x:algo")).toEqual({ text: "x:algo", kinds: null });
    expect(parseQuery("http://algo")).toEqual({ text: "http://algo", kinds: null });
  });

  it("no distingue mayúsculas en el prefijo, pero no toca el texto", () => {
    expect(parseQuery("V:Activos")).toEqual({
      text: "Activos",
      kinds: ["view", "materializedView"],
    });
  });

  it("sin nada escrito no acota nada", () => {
    expect(parseQuery("   ")).toEqual({ text: "", kinds: null });
  });
});

describe("matchesKind", () => {
  it("sin prefijo entra todo, incluso lo que no es un objeto del catálogo", () => {
    const query = parseQuery("factura");
    expect(matchesKind(query, "table")).toBe(true);
    expect(matchesKind(query, null)).toBe(true);
    expect(matchesKind(query, { folder: "tables" })).toBe(true);
  });

  it("con prefijo, una carpeta no es su contenido", () => {
    const query = parseQuery("t:");
    expect(matchesKind(query, { folder: "tables" })).toBe(false);
    expect(matchesKind(query, "table")).toBe(true);
    expect(matchesKind(query, "view")).toBe(false);
  });

  it("una familia abarca lo que el usuario no distingue", () => {
    expect(matchesKind(parseQuery("v:"), "materializedView")).toBe(true);
    expect(matchesKind(parseQuery("f:"), "procedure")).toBe(true);
    expect(matchesKind(parseQuery("t:"), "foreignTable")).toBe(true);
  });
});

describe("filterHits", () => {
  const hits = [
    { kind: "table", label: "facturas" },
    { kind: "view", label: "facturas_activas" },
    { kind: "sequence", label: "facturas_id_seq" },
  ] as SearchHit[];

  it("acota lo que devolvió el servidor sin volver a preguntarle", () => {
    expect(filterHits(parseQuery("t:factura"), hits).map((hit) => hit.label)).toEqual(["facturas"]);
  });

  it("sin prefijo devuelve todo tal cual", () => {
    expect(filterHits(parseQuery("factura"), hits)).toHaveLength(3);
  });
});
