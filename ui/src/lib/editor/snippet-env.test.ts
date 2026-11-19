import { describe, expect, it } from "vitest";
import type { SchemaRelation, Snippet } from "../ipc";
import {
  autoContext,
  columnChoices,
  qualifiedName,
  readWithTimeout,
  shortcutEntries,
  tableChoices,
  withSelection,
} from "./snippet-env";

function relation(schema: string, name: string, columns: string[], oid = 1): SchemaRelation {
  return { oid, schema, name, columns: columns.map((c) => ({ name: c, typeName: "text" })) } as SchemaRelation;
}

function snippet(abbreviation: string, shortcut: string | null): Snippet {
  return {
    id: abbreviation,
    abbreviation,
    body: "x",
    description: "",
    category: "",
    scope: { profileIds: [], databases: [] },
    shortcut,
  };
}

describe("autoContext", () => {
  const now = new Date(2026, 2, 5, 9, 4, 7);

  it("llena el reloj y lo que se conoce de la pestaña", () => {
    const ctx = autoContext({ profileId: "p", database: "ventas", server: "Producción", user: "ana" }, now);
    expect(ctx).toMatchObject({ hoy: "2026-03-05", ahora: "2026-03-05 09:04:07", usuario: "ana", base: "ventas", servidor: "Producción" });
  });

  it("lo que no se sabe no entra, para que quede como hueco y no como texto vacío", () => {
    const ctx = autoContext({ profileId: null, database: null, server: null, user: "" }, now);
    expect(Object.keys(ctx).sort()).toEqual(["ahora", "hoy"]);
  });
});

describe("withSelection", () => {
  it("una selección vacía no cuenta", () => {
    expect(withSelection({ hoy: "x" }, "")).toEqual({ hoy: "x" });
    expect(withSelection({ hoy: "x" }, undefined)).toEqual({ hoy: "x" });
    expect(withSelection({ hoy: "x" }, "SELECT 1")).toEqual({ hoy: "x", seleccion: "SELECT 1" });
  });
});

describe("readWithTimeout", () => {
  it("devuelve lo leído", async () => {
    expect(await readWithTimeout(async () => "hola", 50)).toBe("hola");
  });

  it("un permiso que nunca contesta vale null y no cuelga la expansión", async () => {
    expect(await readWithTimeout(() => new Promise<string>(() => {}), 20)).toBeNull();
  });

  it("un rechazo o una excepción valen null", async () => {
    expect(await readWithTimeout(() => Promise.reject(new Error("denegado")), 50)).toBeNull();
    expect(
      await readWithTimeout(() => {
        throw new Error("sin api");
      }, 50),
    ).toBeNull();
  });
});

describe("tableChoices y qualifiedName", () => {
  it("califica, cita lo que lo pide y deja afuera las CTE", () => {
    const list = [relation("public", "pedidos", ["id"]), relation("Ventas", "Clientes", ["id"]), relation("", "cte", ["a"], -1)];
    expect(qualifiedName(list[1])).toBe('"Ventas"."Clientes"');
    expect(tableChoices(list)).toEqual(['"Ventas"."Clientes"', "public.pedidos"]);
  });
});

describe("columnChoices", () => {
  const catalogo = [relation("public", "pedidos", ["id", "total"]), relation("public", "clientes", ["id", "nombre"])];

  it("prefiere las columnas de las tablas del FROM de la sentencia", () => {
    const sql = "SELECT  FROM clientes";
    expect(columnChoices(catalogo, sql, 7)).toEqual(["id", "nombre"]);
  });

  it("sin tablas a la vista ofrece las de todo el catálogo, sin repetir", () => {
    expect(columnChoices(catalogo, "", 0)).toEqual(["id", "nombre", "total"]);
  });
});

describe("shortcutEntries", () => {
  it("solo registra las que tienen atajo", () => {
    expect(shortcutEntries([snippet("a", null), snippet("b", "Mod-Alt-1")], []).map((e) => e.snippet.abbreviation)).toEqual(["b"]);
  });

  it("los atajos incorporados ganan: el de la abreviatura calla", () => {
    const entries = shortcutEntries([snippet("a", "Mod-Enter"), snippet("b", "Mod-Alt-2")], ["Mod-Enter"]);
    expect(entries.map((e) => e.snippet.abbreviation)).toEqual(["b"]);
  });

  it("Ctrl y Mod son la misma pulsación al comparar", () => {
    expect(shortcutEntries([snippet("a", "Ctrl-Alt-1")], ["Mod-Alt-1"])).toEqual([]);
  });

  it("entre dos iguales vale la primera", () => {
    const entries = shortcutEntries([snippet("a", "Mod-Alt-3"), snippet("b", "Mod-Alt-3")], []);
    expect(entries.map((e) => e.snippet.abbreviation)).toEqual(["a"]);
  });

  it("una combinación mal escrita no se registra", () => {
    expect(shortcutEntries([snippet("a", "cualquier cosa")], [])).toEqual([]);
  });
});
