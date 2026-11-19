import { describe, expect, it } from "vitest";
import {
  autoVariablesOf,
  clockContext,
  fillAutomatic,
  fillVariables,
  previewBody,
  variablesOf,
} from "./snippet-vars";

const NOW = new Date(2026, 2, 5, 9, 7, 3);

describe("variablesOf", () => {
  it("lee nombre y valor por omisión, sin repetir", () => {
    const body = "SELECT * FROM ${tabla:clientes} LIMIT ${n:10}; -- ${tabla:otra}";
    expect(variablesOf(body)).toEqual([
      { name: "tabla", default: "clientes", kind: "text" },
      { name: "n", default: "10", kind: "text" },
    ]);
  });

  it("no toca los huecos de CodeMirror", () => {
    expect(variablesOf("SELECT ${*} FROM ${tabla} WHERE ${} AND ${1:x}")).toEqual([]);
  });

  it("acepta un valor por omisión vacío", () => {
    expect(variablesOf("WHERE ${filtro:}")).toEqual([{ name: "filtro", default: "", kind: "text" }]);
  });

  it("lee las listas: la primera opción es el valor por omisión", () => {
    expect(variablesOf("WHERE estado = '${estado|activo|baja|pendiente}'")).toEqual([
      {
        name: "estado",
        default: "activo",
        kind: "list",
        options: ["activo", "baja", "pendiente"],
      },
    ]);
  });

  it("lee las tipadas, con y sin valor", () => {
    const vars = variablesOf("${d@fecha} ${t@tabla} ${c@columna:id} ${n@numero:5} ${m@numero}", NOW);
    expect(vars).toEqual([
      { name: "d", default: "2026-03-05", kind: "date" },
      { name: "t", default: "", kind: "table" },
      { name: "c", default: "id", kind: "column" },
      { name: "n", default: "5", kind: "number" },
      { name: "m", default: "0", kind: "number" },
    ]);
  });

  it("si una variable se repite con otro tipo vale la primera declaración", () => {
    expect(variablesOf("${x@tabla} ${x|a|b} ${x:z}")).toEqual([
      { name: "x", default: "", kind: "table" },
    ]);
  });

  it("un tipo desconocido no es variable: queda como hueco de CodeMirror", () => {
    expect(variablesOf("${x@color}")).toEqual([]);
  });

  it("los automáticos no son variables, ni siquiera con el mismo nombre pelado", () => {
    expect(variablesOf("${hoy} ${base} ${seleccion}")).toEqual([]);
  });

  it("un automático con dos puntos sí es una variable común", () => {
    expect(variablesOf("${base:ventas}")).toEqual([{ name: "base", default: "ventas", kind: "text" }]);
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

  it("rellena las de lista y las tipadas", () => {
    const body = "${e|activo|baja} ${d@fecha} ${t@tabla}";
    expect(fillVariables(body, { e: "baja", t: "clientes" }, NOW)).toBe("baja 2026-03-05 clientes");
    expect(fillVariables(body, {}, NOW)).toBe("activo 2026-03-05 ");
  });

  it("una repetida con otro tipo usa el valor por omisión de la primera", () => {
    expect(fillVariables("${x:a} ${x@numero}", {})).toBe("a a");
  });

  it("no toca los automáticos", () => {
    expect(fillVariables("${hoy} ${v:1}", {})).toBe("${hoy} 1");
  });
});

describe("autoVariablesOf y fillAutomatic", () => {
  it("lista los automáticos usados, sin repetir", () => {
    expect(autoVariablesOf("${hoy} ${base} ${hoy} ${tabla} ${x:1}")).toEqual(["hoy", "base"]);
  });

  it("llena lo que el contexto trae", () => {
    expect(fillAutomatic("-- ${usuario} en ${base}", { usuario: "ana", base: "ventas" })).toBe(
      "-- ana en ventas",
    );
  });

  it("un valor ausente queda como hueco, no se borra", () => {
    expect(fillAutomatic("SELECT ${seleccion}", {})).toBe("SELECT ${seleccion}");
  });

  it("escapa las llaves de la selección: puede traer ${ adentro", () => {
    const sql = "SELECT '${x:1}'";
    expect(fillAutomatic("(${seleccion})", { seleccion: sql })).toBe("(SELECT '$\\{x:1\\}')");
  });

  it("lo insertado no lo vuelve a leer fillVariables", () => {
    const body = fillAutomatic("${seleccion} ${v:2}", { seleccion: "${v:9}" });
    expect(fillVariables(body, {})).toBe("$\\{v:9\\} 2");
  });

  it("el reloj se arma en hora local", () => {
    expect(clockContext(NOW)).toEqual({ hoy: "2026-03-05", ahora: "2026-03-05 09:07:03" });
  });
});

describe("previewBody", () => {
  it("expande sin preguntar: valores por omisión y nombres de los huecos", () => {
    const body = "SELECT ${*}\nFROM ${tabla@tabla}\nWHERE ${} AND e = '${e|a|b}' LIMIT ${n@numero:5}";
    expect(previewBody(body, {}, NOW)).toBe("SELECT *\nFROM tabla\nWHERE  AND e = 'a' LIMIT 5");
  });

  it("los huecos de CodeMirror valen su texto", () => {
    expect(previewBody("${1:uno} ${2} ${nombre}", {}, NOW)).toBe("uno  nombre");
  });

  it("las fechas valen hoy y los automáticos su ejemplo", () => {
    expect(previewBody("${d@fecha} ${hoy} ${usuario}", { usuario: "ana" }, NOW)).toBe(
      "2026-03-05 2026-03-05 ana",
    );
  });

  it("lo que se inserta no se lee de nuevo", () => {
    expect(previewBody("${seleccion}", { seleccion: "${x}" }, NOW)).toBe("${x}");
  });

  it("las llaves escapadas del cuerpo salen como llaves", () => {
    expect(previewBody("a \\{b\\} ${v:c}", {}, NOW)).toBe("a {b} c");
  });

  it("una tabla con valor escrito muestra ese valor", () => {
    expect(previewBody("${t@tabla:clientes}", {}, NOW)).toBe("clientes");
  });
});
