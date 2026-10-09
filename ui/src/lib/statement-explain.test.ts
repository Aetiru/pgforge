import { describe, expect, it } from "vitest";

import { needsValues, withValuesHint } from "./statement-explain";

describe("needsValues", () => {
  it("distingue una sentencia con parámetros de una sin ellos", () => {
    expect(needsValues("SELECT * FROM pedidos WHERE id = $1")).toBe(true);
    expect(needsValues("SELECT count(*) FROM pedidos")).toBe(false);
  });

  it("un cuerpo entre $$ no cuenta como parámetro", () => {
    expect(needsValues("DO $$ BEGIN PERFORM 1; END $$")).toBe(false);
  });
});

describe("withValuesHint", () => {
  it("nombra cada parámetro una sola vez y en orden numérico", () => {
    const text = withValuesHint("UPDATE t SET a = $2 WHERE b = $10 AND c = $2 AND d = $1");
    expect(text.split("\n")[0]).toContain("$1, $2, $10");
    expect(text.endsWith("AND d = $1")).toBe(true);
  });
});
