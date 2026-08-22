import { describe, expect, it } from "vitest";
import { columnOptions, hoverInfo, qualifiedNameAt, relationAt, tablesInScope } from "./sql-complete";
import type { RelationColumn, SchemaRelation } from "./ipc";

/** Nombre pelado, sin tipo ni comentario: acá no hacen falta para probar el autocompletado. */
function cols(...names: string[]): RelationColumn[] {
  return names.map((name) => ({ name, typeName: "text" }));
}

let oid = 0;
function relation(schema: string, name: string, columns: string[]): SchemaRelation {
  oid += 1;
  return { oid, schema, name, columns: cols(...columns) };
}

const RELATIONS: SchemaRelation[] = [
  relation("public", "clientes", ["id", "nombre", "creado"]),
  relation("public", "pedidos", ["id", "cliente_id", "total"]),
  relation("ventas", "clientes", ["id", "razon_social"]),
];

/** Dónde está el cursor se marca con `|` para no contar caracteres a mano en cada caso. */
function at(marked: string) {
  const cursor = marked.indexOf("|");
  return { sql: marked.replace("|", ""), cursor };
}

describe("tablesInScope", () => {
  it("encuentra la tabla del FROM sin calificar", () => {
    const { sql, cursor } = at("SELECT | FROM clientes");
    expect(tablesInScope(sql, cursor)).toEqual([
      { schema: null, name: "clientes", alias: null },
    ]);
  });

  it("se queda con el alias cuando lo hay, con AS y sin AS", () => {
    const { sql, cursor } = at("SELECT | FROM public.clientes AS c JOIN pedidos p ON p.id = c.id");
    expect(tablesInScope(sql, cursor)).toEqual([
      { schema: "public", name: "clientes", alias: "c" },
      { schema: null, name: "pedidos", alias: "p" },
    ]);
  });

  it("parte la lista del FROM por las comas de primer nivel", () => {
    const { sql, cursor } = at("SELECT | FROM clientes c, pedidos");
    expect(tablesInScope(sql, cursor)).toEqual([
      { schema: null, name: "clientes", alias: "c" },
      { schema: null, name: "pedidos", alias: null },
    ]);
  });

  it("no toma como alias la palabra que cierra la lista", () => {
    const { sql, cursor } = at("SELECT | FROM clientes WHERE id = 1");
    expect(tablesInScope(sql, cursor)).toEqual([
      { schema: null, name: "clientes", alias: null },
    ]);
  });

  it("ignora las subconsultas, cuyas columnas no están en el catálogo", () => {
    const { sql, cursor } = at("SELECT | FROM (SELECT 1) t, pedidos");
    expect(tablesInScope(sql, cursor)).toEqual([
      { schema: null, name: "pedidos", alias: null },
    ]);
  });

  it("solo mira la sentencia donde está el cursor", () => {
    const { sql, cursor } = at("SELECT * FROM clientes;\nSELECT | FROM pedidos;");
    expect(tablesInScope(sql, cursor)).toEqual([
      { schema: null, name: "pedidos", alias: null },
    ]);
  });

  it("toma la tabla de un UPDATE y la de un INSERT INTO", () => {
    const update = at("UPDATE clientes SET | = 1");
    expect(tablesInScope(update.sql, update.cursor)).toEqual([
      { schema: null, name: "clientes", alias: null },
    ]);

    const insert = at("INSERT INTO pedidos (|)");
    expect(tablesInScope(insert.sql, insert.cursor)).toEqual([
      { schema: null, name: "pedidos", alias: null },
    ]);
  });

  it("saca los nombres de adentro de las comillas", () => {
    const { sql, cursor } = at('SELECT | FROM "Mi Esquema"."Mi Tabla" t');
    expect(tablesInScope(sql, cursor)).toEqual([
      { schema: "Mi Esquema", name: "Mi Tabla", alias: "t" },
    ]);
  });

  it("no se confunde con un FROM adentro de un comentario", () => {
    const { sql, cursor } = at("SELECT | -- FROM pedidos\nFROM clientes");
    expect(tablesInScope(sql, cursor)).toEqual([
      { schema: null, name: "clientes", alias: null },
    ]);
  });
});

describe("columnOptions", () => {
  it("ofrece las columnas de la tabla del FROM con el alias al lado", () => {
    const refs = [{ schema: null, name: "clientes", alias: "c" }];
    expect(columnOptions(RELATIONS, refs)).toEqual([
      { label: "id", detail: "c" },
      { label: "nombre", detail: "c" },
      { label: "creado", detail: "c" },
    ]);
  });

  it("desempata por esquema cuando el nombre está repetido", () => {
    const refs = [{ schema: "ventas", name: "clientes", alias: null }];
    expect(columnOptions(RELATIONS, refs).map((option) => option.label)).toEqual([
      "id",
      "razon_social",
    ]);
  });

  it("repite la columna que está en dos tablas, una vez por cada una", () => {
    const refs = [
      { schema: null, name: "clientes", alias: "c" },
      { schema: null, name: "pedidos", alias: "p" },
    ];
    const id = columnOptions(RELATIONS, refs).filter((option) => option.label === "id");
    expect(id).toEqual([
      { label: "id", detail: "c" },
      { label: "id", detail: "p" },
    ]);
  });

  it("no ofrece nada de una tabla que el catálogo no conoce", () => {
    const refs = [{ schema: null, name: "inventada", alias: null }];
    expect(columnOptions(RELATIONS, refs)).toEqual([]);
  });
});

describe("qualifiedNameAt", () => {
  it("encuentra el identificador aunque el clic caiga en el medio", () => {
    const { sql, cursor } = at("SELECT * FROM cli|entes");
    expect(qualifiedNameAt(sql, cursor)).toEqual({ schema: null, name: "clientes", from: 14, to: 22 });
  });

  it("junta el esquema cuando el punto está pegado a la izquierda", () => {
    const { sql, cursor } = at("SELECT * FROM ventas.cli|entes");
    expect(qualifiedNameAt(sql, cursor)).toEqual({
      schema: "ventas",
      name: "clientes",
      from: 21,
      to: 29,
    });
  });

  it("un punto sin nada antes no arma un esquema", () => {
    const { sql, cursor } = at(".cli|entes");
    expect(qualifiedNameAt(sql, cursor)).toEqual({ schema: null, name: "clientes", from: 1, to: 9 });
  });

  it("nada bajo el cursor da null", () => {
    const { sql, cursor } = at("SELECT * FROM | clientes");
    expect(qualifiedNameAt(sql, cursor)).toBeNull();
  });
});

describe("relationAt", () => {
  const refs = [{ schema: null, name: "clientes", alias: "c" }];

  it("con esquema, busca directo por esquema y nombre", () => {
    const found = relationAt(RELATIONS, [], { schema: "ventas", name: "clientes" });
    expect(found?.schema).toBe("ventas");
  });

  it("sin esquema, resuelve por el alias del FROM", () => {
    const found = relationAt(RELATIONS, refs, { schema: null, name: "c" });
    expect(found?.schema).toBe("public");
    expect(found?.name).toBe("clientes");
  });

  it("sin esquema y sin alias, resuelve por el nombre de la tabla en el FROM", () => {
    const found = relationAt(RELATIONS, refs, { schema: null, name: "clientes" });
    expect(found?.schema).toBe("public");
  });

  it("un nombre repetido en dos esquemas y sin nada en el FROM no elige ninguno", () => {
    expect(relationAt(RELATIONS, [], { schema: null, name: "clientes" })).toBeNull();
  });

  it("un nombre que no está en ningún lado da null", () => {
    expect(relationAt(RELATIONS, [], { schema: null, name: "inventada" })).toBeNull();
  });
});

describe("hoverInfo", () => {
  const comentadas: SchemaRelation[] = [
    {
      oid: 1,
      schema: "public",
      name: "clientes",
      comment: "Uno por persona o empresa.",
      columns: [
        { name: "id", typeName: "integer" },
        { name: "creado", typeName: "timestamp with time zone", comment: "En UTC, no local." },
      ],
    },
    { oid: 2, schema: "public", name: "sin_comentario", columns: cols("id") },
  ];

  it("con alias, resuelve la columna de esa tabla", () => {
    const refs = [{ schema: null, name: "clientes", alias: "c" }];
    expect(hoverInfo(comentadas, refs, { schema: "c", name: "creado" })).toEqual({
      kind: "column",
      table: "c",
      column: { name: "creado", typeName: "timestamp with time zone", comment: "En UTC, no local." },
    });
  });

  it("sin alias, dentro del FROM, resuelve la columna igual", () => {
    const refs = [{ schema: null, name: "clientes", alias: null }];
    expect(hoverInfo(comentadas, refs, { schema: null, name: "id" })).toEqual({
      kind: "column",
      table: "clientes",
      column: { name: "id", typeName: "integer" },
    });
  });

  it("esquema.tabla sin alias que lo tape muestra el comentario de la tabla", () => {
    expect(hoverInfo(comentadas, [], { schema: "public", name: "clientes" })).toEqual({
      kind: "table",
      relation: comentadas[0],
    });
  });

  it("una tabla sin COMMENT ON no tiene nada que mostrar", () => {
    expect(hoverInfo(comentadas, [], { schema: "public", name: "sin_comentario" })).toBeNull();
  });

  it("nada que resuelva da null", () => {
    expect(hoverInfo(comentadas, [], { schema: null, name: "inventada" })).toBeNull();
  });
});
