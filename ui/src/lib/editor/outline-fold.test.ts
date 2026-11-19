import { describe, expect, it } from "vitest";
import { outline } from "../sql-outline";
import { statementFold } from "./outline-fold";

const script = "SELECT 1;\nSELECT a,\n  b,\n  c\nFROM t;\nSELECT 2;";

function fold(line: number) {
  const items = outline(script);
  const starts = [0];
  for (let i = 0; i < script.length; i += 1) if (script[i] === "\n") starts.push(i + 1);
  const from = starts[line];
  const to = line + 1 < starts.length ? starts[line + 1] - 1 : script.length;
  return statementFold(items, script, from, to);
}

describe("statementFold", () => {
  it("pliega una sentencia de varias líneas desde el final de la primera", () => {
    const found = fold(1);
    expect(found).not.toBeNull();
    expect(script.slice(found!.from, found!.to)).toContain("FROM t");
    expect(script.slice(0, found!.from).endsWith("SELECT a,")).toBe(true);
  });

  it("no pliega una sentencia de una línea", () => {
    expect(fold(0)).toBeNull();
    expect(fold(5)).toBeNull();
  });

  it("no pliega una línea del medio de una sentencia", () => {
    expect(fold(2)).toBeNull();
  });
});
