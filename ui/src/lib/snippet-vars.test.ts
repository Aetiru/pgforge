import { describe, expect, it } from "vitest";
import { fillVariables, variablesOf } from "./snippet-vars";

describe("variablesOf", () => {
  it("lee nombre y valor por omisión, sin repetir", () => {
    const body = "SELECT * FROM ${tabla:clientes} LIMIT ${n:10}; -- ${tabla:otra}";
    expect(variablesOf(body)).toEqual([
      { name: "tabla", default: "clientes" },
      { name: "n", default: "10" },
    ]);
  });

  it("no toca los huecos de CodeMirror", () => {
    expect(variablesOf("SELECT ${*} FROM ${tabla} WHERE ${} AND ${1:x}")).toEqual([]);
  });

  it("acepta un valor por omisión vacío", () => {
    expect(variablesOf("WHERE ${filtro:}")).toEqual([{ name: "filtro", default: "" }]);
  });
});

describe("fillVariables", () => {
  it("rellena todas las apariciones con el mismo valor", () => {
    expect(fillVariables("${t:a} ${t:a}", { t: "x" })).toBe("x x");
  });

  it("usa el valor por omisión cuando no se escribió nada", () => {
    expect(fillVariables("LIMIT ${n:10}", {})).toBe("LIMIT 10");
  });

  it("deja intactos los huecos", () => {
    expect(fillVariables("FROM ${t:a} WHERE ${}", { t: "b" })).toBe("FROM b WHERE ${}");
  });

  it("escapa las llaves del valor para que no se vuelvan huecos", () => {
    expect(fillVariables("SELECT '${v:x}'", { v: "${y}" })).toBe("SELECT '$\\{y\\}'");
  });
});
