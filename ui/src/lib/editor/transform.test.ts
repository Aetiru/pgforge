import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { transformTransaction } from "./transform";

function apply(doc: string, anchor: number, head: number, kind: Parameters<typeof transformTransaction>[1]) {
  const state = EditorState.create({ doc, selection: { anchor, head } });
  const spec = transformTransaction(state, kind);
  return spec ? state.update(spec).state.doc.toString() : null;
}

describe("transformTransaction", () => {
  it("con selección toca solo la selección", () => {
    expect(apply("a\nb\nc", 0, 3, "joinComma")).toBe("a, b\nc");
  });

  it("sin selección toca la sentencia donde está el cursor y no el resto del script", () => {
    const doc = "select 1;\nselect 2;";
    const result = apply(doc, 2, 2, "upper");
    expect(result).toBe("SELECT 1;\nselect 2;");
  });

  it("devuelve null cuando el texto no cambia", () => {
    expect(apply("SELECT 1", 0, 8, "upper")).toBeNull();
  });
});
