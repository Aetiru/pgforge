import { describe, expect, it } from "vitest";
import { columnNameOfKey, parseViews, withView, withoutView, type GridView } from "./grid-views";

const view = (name: string, over: Partial<GridView> = {}): GridView => ({
  name,
  hidden: [],
  order: null,
  filter: "",
  ...over,
});

describe("vistas de la grilla", () => {
  it("guardar con un nombre que ya existe lo reemplaza, sin distinguir mayúsculas", () => {
    const next = withView([view("Hoy", { filter: "a" })], view("hoy", { filter: "b" }));
    expect(next).toHaveLength(1);
    expect(next[0]).toMatchObject({ name: "hoy", filter: "b" });
  });

  it("un nombre vacío no se guarda", () => {
    expect(withView([], view("   "))).toEqual([]);
  });

  it("hay un tope por tabla", () => {
    let list: GridView[] = [];
    for (let i = 0; i < 30; i++) list = withView(list, view(`v${i}`));
    expect(list).toHaveLength(20);
    expect(list.at(-1)?.name).toBe("v29");
  });

  it("borra por nombre", () => {
    expect(withoutView([view("a"), view("b")], "a").map((v) => v.name)).toEqual(["b"]);
  });

  it("la clave de la grilla se vuelve nombre de columna", () => {
    expect(columnNameOfKey("12-total")).toBe("total");
    expect(columnNameOfKey("3-1-raro")).toBe("1-raro");
  });

  it("sanea lo guardado", () => {
    const map = parseViews(
      JSON.stringify({
        k: [{ name: "ok", hidden: ["a", 3], order: { column: "x", descending: true }, filter: "y" }, { nombre: 1 }, 5],
        malo: "x",
      }),
    );
    expect(map).toEqual({ k: [{ name: "ok", hidden: ["a"], order: { column: "x", descending: true }, filter: "y" }] });
    expect(parseViews("???")).toEqual({});
  });
});
