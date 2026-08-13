import { describe, expect, it } from "vitest";

import { changesCatalog } from "./ddl-tags";

describe("changesCatalog", () => {
  it("reconoce el DDL por la primera palabra de la etiqueta", () => {
    expect(changesCatalog("CREATE TABLE")).toBe(true);
    expect(changesCatalog("DROP INDEX")).toBe(true);
    expect(changesCatalog("ALTER TABLE")).toBe(true);
    expect(changesCatalog("COMMENT")).toBe(true);
    expect(changesCatalog("REFRESH MATERIALIZED VIEW")).toBe(true);
    expect(changesCatalog("SECURITY LABEL")).toBe(true);
  });

  it("deja fuera lo que cambia filas y no objetos", () => {
    expect(changesCatalog("INSERT 0 1")).toBe(false);
    expect(changesCatalog("UPDATE 3")).toBe(false);
    expect(changesCatalog("DELETE 12")).toBe(false);
    expect(changesCatalog("TRUNCATE TABLE")).toBe(false);
    expect(changesCatalog("SELECT 1")).toBe(false);
    expect(changesCatalog("SET")).toBe(false);
  });

  it("no se cae con una etiqueta vacía ni con espacios de más", () => {
    expect(changesCatalog("")).toBe(false);
    expect(changesCatalog("   ")).toBe(false);
    expect(changesCatalog("  create   table  ")).toBe(true);
  });
});
