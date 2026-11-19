import { describe, expect, it } from "vitest";
import { parseTsv, planPaste } from "./tsv";

describe("parseTsv", () => {
  it("separa por tabulador y por línea, y descarta el salto final de Excel", () => {
    expect(parseTsv("a\tb\r\n1\t2\r\n")).toEqual([["a", "b"], ["1", "2"]]);
    expect(parseTsv("a\tb\n1\t2")).toEqual([["a", "b"], ["1", "2"]]);
  });

  it("una celda entre comillas puede llevar tabuladores, saltos y comillas", () => {
    expect(parseTsv('"a\tb"\t"línea 1\nlínea 2"\t"dice ""hola"""')).toEqual([["a\tb", "línea 1\nlínea 2", 'dice "hola"']]);
  });

  it("celdas vacías se conservan", () => {
    expect(parseTsv("a\t\tc\n\t\t")).toEqual([["a", "", "c"], ["", "", ""]]);
  });

  it("una comilla en medio de un valor no abre una cadena", () => {
    expect(parseTsv('5"\tx')).toEqual([['5"', "x"]]);
  });

  it("texto vacío son cero filas", () => {
    expect(parseTsv("")).toEqual([]);
  });
});

describe("planPaste", () => {
  const columns = [
    { name: "id", generated: true },
    { name: "nombre", generated: false },
    { name: "estado", generated: false },
  ];

  it("sin encabezado empareja por posición con las columnas escribibles", () => {
    const plan = planPaste([["Ana", "activo"], ["Luis", ""]], columns);
    expect(typeof plan).toBe("object");
    if (typeof plan === "string") return;
    expect(plan.columns).toEqual(["nombre", "estado"]);
    expect(plan.rows[0].get(1)).toBe("Ana");
    expect(plan.rows[1].get(2)).toBeNull();
  });

  it("con encabezado empareja por nombre, en cualquier orden", () => {
    const plan = planPaste([["estado", "nombre"], ["baja", "Ana"]], columns);
    if (typeof plan === "string") throw new Error(plan);
    expect(plan.usedHeader).toBe(true);
    expect(plan.rows[0].get(1)).toBe("Ana");
    expect(plan.rows[0].get(2)).toBe("baja");
  });

  it("no deja pegar en una columna generada", () => {
    expect(planPaste([["id", "nombre"], ["1", "Ana"]], columns)).toMatch(/generadas/);
  });

  it("avisa cuando no hay nada o solo encabezados", () => {
    expect(planPaste([], columns)).toMatch(/nada/);
    expect(planPaste([["nombre", "estado"]], columns)).toMatch(/Solo hay encabezados/);
  });
});
