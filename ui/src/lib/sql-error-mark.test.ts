import { describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { Decoration, type DecorationSet } from "@codemirror/view";
import { errorMarkField, markOf, setErrorMark } from "./sql-error-mark";

/** Las posiciones del conjunto, que es lo único que se mira acá. */
function ranges(set: DecorationSet): Array<{ from: number; to: number }> {
  const out: Array<{ from: number; to: number }> = [];
  set.between(0, 1e9, (from, to) => {
    out.push({ from, to });
  });
  return out;
}

function stateWith(doc: string, mark: { at: number; message: string } | null): EditorState {
  const state = EditorState.create({ doc, extensions: [errorMarkField] });
  return state.update({ effects: setErrorMark.of(markOf(doc, mark)) }).state;
}

describe("markOf", () => {
  it("subraya la palabra entera y no un solo carácter", () => {
    const sql = "SELECT * FROM clientes WHERE nada = 1";
    const at = sql.indexOf("nada");
    expect(ranges(markOf(sql, { at, message: "no existe" }))).toEqual([
      { from: at, to: at + "nada".length },
    ]);
  });

  it("no marca nada si la posición cae fuera del texto", () => {
    expect(ranges(markOf("SELECT 1", { at: 62, message: "lejos" }))).toEqual([]);
    expect(ranges(markOf("SELECT 1", null))).toEqual([]);
  });

  it("cuenta en caracteres y no en unidades UTF-16", () => {
    // El emoji ocupa dos unidades UTF-16, así que pasar el número del núcleo tal cual correría la
    // marca una posición hacia atrás.
    const sql = "SELECT '🙂', malo";
    const marks = ranges(markOf(sql, { at: [...sql].indexOf("m"), message: "no existe" }));
    expect(sql.slice(marks[0].from, marks[0].to)).toBe("malo");
  });
});

describe("errorMarkField", () => {
  it("mueve la marca con el texto que se escribe antes", () => {
    const sql = "SELECT malo";
    const state = stateWith(sql, { at: sql.indexOf("malo"), message: "no existe" });

    const after = state.update({ changes: { from: 0, insert: "-- nota\n" } }).state;
    const [mark] = ranges(after.field(errorMarkField));
    expect(after.doc.sliceString(mark.from, mark.to)).toBe("malo");
  });

  it("suelta la marca cuando se borra la palabra que señalaba", () => {
    const sql = "SELECT malo";
    const state = stateWith(sql, { at: sql.indexOf("malo"), message: "no existe" });

    const after = state.update({ changes: { from: 0, to: sql.length, insert: "" } }).state;
    expect(ranges(after.field(errorMarkField))).toEqual([]);
  });

  it("no deja posiciones fuera del documento al vaciarlo, ni en la transacción siguiente", () => {
    // La regresión: con la marca en un `DecorationSet` fijo, vaciar el editor la dejaba apuntando al
    // 62 de un documento de largo 0. CodeMirror no fallaba ahí sino en la primera transacción que
    // viniera después —un clic alcanzaba— con «Position 62 is out of range for changeset of length
    // 0», y en todas las que siguieran.
    const sql = "SELECT * FROM clientes WHERE estado = 'x' AND fecha > now() AND malo";
    const at = sql.indexOf("malo");
    expect(at).toBeGreaterThan(60);

    const state = stateWith(sql, { at, message: "no existe" });
    expect(ranges(state.field(errorMarkField))).toHaveLength(1);

    const vacio = state.update({ changes: { from: 0, to: sql.length, insert: "" } }).state;
    expect(vacio.doc.length).toBe(0);
    for (const { from, to } of ranges(vacio.field(errorMarkField))) {
      expect(to).toBeLessThanOrEqual(0);
      expect(from).toBeLessThanOrEqual(0);
    }

    // Y la de después, que es donde saltaba de verdad.
    expect(() => vacio.update({ selection: { anchor: 0 } })).not.toThrow();
    expect(() => vacio.update({ changes: { from: 0, insert: "SELECT 1" } })).not.toThrow();
  });

  it("el efecto reemplaza la marca en vez de acumularla", () => {
    const sql = "SELECT malo, peor";
    const state = stateWith(sql, { at: sql.indexOf("malo"), message: "no existe" });

    const otra = state.update({ effects: setErrorMark.of(markOf(sql, { at: sql.indexOf("peor"), message: "tampoco" })) }).state;
    const marks = ranges(otra.field(errorMarkField));
    expect(marks).toHaveLength(1);
    expect(sql.slice(marks[0].from, marks[0].to)).toBe("peor");

    const limpio = state.update({ effects: setErrorMark.of(Decoration.none) }).state;
    expect(ranges(limpio.field(errorMarkField))).toEqual([]);
  });
});
