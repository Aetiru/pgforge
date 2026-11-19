import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { RELATIONS } from "./fixtures";
import { starTransaction } from "./star";

function stateAt(doc: string, at: number) {
  return EditorState.create({ doc, selection: { anchor: at } });
}

describe("starTransaction", () => {
  it("reemplaza el asterisco por las columnas en un solo cambio", () => {
    const doc = "SELECT * FROM clientes";
    const state = stateAt(doc, 8);
    const spec = starTransaction(state, RELATIONS);
    expect(spec).not.toBeNull();
    const next = state.update(spec!).state.doc.toString();
    expect(next).toContain("id");
    expect(next).toContain("nombre");
    expect(next).not.toContain("*");
    expect(next.endsWith("FROM clientes")).toBe(true);
  });

  it("no hace nada si el cursor no toca un asterisco", () => {
    expect(starTransaction(stateAt("SELECT id FROM clientes", 3), RELATIONS)).toBeNull();
  });

  it("no expande el asterisco de un count(*)", () => {
    const doc = "SELECT count(*) FROM clientes";
    expect(starTransaction(stateAt(doc, doc.indexOf("*")), RELATIONS)).toBeNull();
  });

  it("deja el cursor al final de lo insertado", () => {
    const state = stateAt("SELECT * FROM clientes", 8);
    const next = state.update(starTransaction(state, RELATIONS)!).state;
    expect(next.doc.sliceString(0, next.selection.main.head)).toMatch(/nombre$/);
  });
});
