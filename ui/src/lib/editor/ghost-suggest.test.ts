import { describe, expect, it } from "vitest";
import { advanceGhost, ghostDisplay, ghostPrefix, ghostRemainder } from "./ghost-suggest";

describe("ghostPrefix", () => {
  const doc = "SELECT 1;\nSELECT c.nombre, sum(p.total) FROM pedidos p";

  it("es la sentencia desde su inicio hasta el cursor, cuando el cursor está al final", () => {
    const start = doc.indexOf("SELECT c");
    expect(ghostPrefix(doc, doc.length, start, doc.length)).toBe("SELECT c.nombre, sum(p.total) FROM pedidos p");
  });

  it("no sugiere con el cursor en medio de la sentencia", () => {
    const start = doc.indexOf("SELECT c");
    expect(ghostPrefix(doc, start + 15, start, doc.length)).toBeNull();
  });

  it("tolera espacios y el punto y coma final tras el cursor", () => {
    const text = "SELECT * FROM clientes  ;";
    expect(ghostPrefix(text, "SELECT * FROM clientes".length, 0, text.length)).toBe("SELECT * FROM clientes");
  });

  it("con poco escrito no sugiere", () => {
    expect(ghostPrefix("SELECT *", 8, 0, 8)).toBeNull();
  });

  it("después de un punto y coma ya es otra sentencia", () => {
    const text = "SELECT * FROM clientes;";
    expect(ghostPrefix(text, text.length, 0, text.length)).toBeNull();
  });
});

describe("ghostRemainder y ghostDisplay", () => {
  it("lo que falta, solo si continúa lo escrito y agrega algo", () => {
    expect(ghostRemainder("SELECT * FROM a WHERE x = 1", "SELECT * FROM a")).toBe(" WHERE x = 1");
    expect(ghostRemainder("SELECT * FROM b", "SELECT * FROM a")).toBeNull();
    expect(ghostRemainder("SELECT * FROM a", "SELECT * FROM a")).toBeNull();
  });

  it("dibuja solo la primera línea, con puntos suspensivos si había más", () => {
    expect(ghostDisplay(" WHERE x = 1")).toBe(" WHERE x = 1");
    expect(ghostDisplay(" WHERE x = 1  \nORDER BY id")).toBe(" WHERE x = 1…");
  });
});

describe("advanceGhost", () => {
  const ghost = { pos: 10, rest: " WHERE x = 1" };

  it("escribir lo que venía lo consume", () => {
    expect(advanceGhost(ghost, { at: 10, removed: 0, inserted: " WH" })).toEqual({ pos: 13, rest: "ERE x = 1" });
  });

  it("escribir otra cosa lo descarta", () => {
    expect(advanceGhost(ghost, { at: 10, removed: 0, inserted: " JOIN" })).toBeNull();
  });

  it("borrar o editar en otro lugar lo descarta", () => {
    expect(advanceGhost(ghost, { at: 9, removed: 1, inserted: "" })).toBeNull();
    expect(advanceGhost(ghost, { at: 3, removed: 0, inserted: " " })).toBeNull();
  });

  it("al terminar de escribir la propuesta no queda nada", () => {
    expect(advanceGhost({ pos: 5, rest: "ab" }, { at: 5, removed: 0, inserted: "ab" })).toBeNull();
  });
});
