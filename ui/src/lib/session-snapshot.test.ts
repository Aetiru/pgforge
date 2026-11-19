import { describe, expect, it } from "vitest";
import { parseSaved } from "./session-snapshot";

describe("parseSaved", () => {
  it("sin nada guardado no hay pestañas", () => {
    expect(parseSaved(null)).toEqual([]);
    expect(parseSaved("")).toEqual([]);
  });

  it("un JSON roto no tira la aplicación", () => {
    expect(parseSaved("{no es json")).toEqual([]);
    expect(parseSaved('{"a":1}')).toEqual([]);
  });

  it("descarta las entradas con forma inválida y completa lo opcional", () => {
    const raw = JSON.stringify([
      { profileId: "p1", database: "db", title: "t", sql: "SELECT 1" },
      { profileId: 7, database: "db", title: "t", sql: "x" },
      { profileId: "p2", database: "db", title: "t", sql: "y", scriptPath: "/a.sql", savedId: 3 },
    ]);
    const result = parseSaved(raw);
    expect(result).toHaveLength(2);
    expect(result[0]).toMatchObject({ scriptPath: null, filePath: null, savedId: null });
    expect(result[1]).toMatchObject({ scriptPath: "/a.sql", savedId: 3, savedName: null });
  });

  it("corta en 40 pestañas", () => {
    const many = Array.from({ length: 60 }, (_, i) => ({
      profileId: "p",
      database: "d",
      title: String(i),
      sql: "x",
    }));
    expect(parseSaved(JSON.stringify(many))).toHaveLength(40);
  });
});
