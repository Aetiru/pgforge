import { describe, expect, it } from "vitest";
import { allContainers, childPath, defaultOpen, flatten, parseJsonTree } from "./json-tree";

const doc = { a: 1, "b c": [true, null, { x: "y" }], d: {} };

describe("parseJsonTree", () => {
  it("solo toma objetos y arreglos", () => {
    expect(parseJsonTree('{"a":1}')).toEqual({ ok: true, value: { a: 1 } });
    expect(parseJsonTree("123").ok).toBe(false);
    expect(parseJsonTree("{roto").ok).toBe(false);
  });
});

describe("childPath", () => {
  it("escribe jsonpath de PostgreSQL", () => {
    expect(childPath("$", "a")).toBe("$.a");
    expect(childPath("$.a", 0)).toBe("$.a[0]");
    expect(childPath("$", "b c")).toBe('$."b c"');
    expect(childPath("$", 'a"b')).toBe('$."a\\"b"');
  });
});

describe("flatten", () => {
  it("muestra solo la raíz si está plegada", () => {
    const rows = flatten(doc, new Set());
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ path: "$", kind: "object", text: "{3}", open: false });
  });

  it("despliega con rutas correctas", () => {
    const rows = flatten(doc, new Set(["$", '$."b c"'])) as { path: string; kind: string; text: string }[];
    expect(rows.map((r) => r.path)).toEqual(["$", "$.a", '$."b c"', '$."b c"[0]', '$."b c"[1]', '$."b c"[2]', "$.d"]);
    expect(rows[1]).toMatchObject({ kind: "number", text: "1" });
    expect(rows[4]).toMatchObject({ kind: "null", text: "null" });
  });

  it("corta los contenedores enormes", () => {
    const rows = flatten(Array.from({ length: 10 }, (_, i) => i), new Set(["$"]), 3);
    expect(rows).toHaveLength(5);
    expect(rows[4]).toMatchObject({ more: 7 });
  });

  it("las cadenas se escriben entre comillas", () => {
    const rows = flatten({ s: 'a"b' }, new Set(["$"])) as { text: string }[];
    expect(rows[1].text).toBe('"a\\"b"');
  });
});

describe("defaultOpen / allContainers", () => {
  it("abre la raíz y el primer nivel", () => {
    expect([...defaultOpen(doc)].sort()).toEqual(['$', '$."b c"', "$.d"].sort());
  });

  it("desplegar todo incluye lo profundo", () => {
    expect(allContainers(doc).has('$."b c"[2]')).toBe(true);
  });
});
