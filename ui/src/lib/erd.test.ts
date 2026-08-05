import { describe, expect, it } from "vitest";

import { METRICS, layout, neighbors, pathOf, ranks } from "./erd";
import type { GraphColumn, GraphEdge, GraphTable, SchemaGraph } from "./ipc";

let nextOid = 1;

function table(name: string, columns: string[] = ["id"]): GraphTable {
  return {
    oid: nextOid++,
    name,
    kind: "table",
    columns: columns.map(
      (column, index): GraphColumn => ({
        position: index + 1,
        name: column,
        typeName: "text",
        notNull: false,
        primaryKey: index === 0,
        foreignKey: false,
      }),
    ),
  };
}

function edge(source: GraphTable, target: GraphTable, extra: Partial<GraphEdge> = {}): GraphEdge {
  return {
    name: `${source.name}_${target.name}_fkey`,
    source: source.oid,
    target: target.oid,
    sourceColumns: ["id"],
    targetColumns: ["id"],
    onUpdate: "noAction",
    onDelete: "noAction",
    ...extra,
  };
}

function graphOf(tables: GraphTable[], edges: GraphEdge[]): SchemaGraph {
  return { database: "postgres", schema: "public", tables, edges };
}

describe("ranks", () => {
  it("pone la referida a la izquierda y la que referencia a la derecha", () => {
    const paises = table("paises");
    const clientes = table("clientes");
    const ventas = table("ventas");
    const graph = graphOf(
      [clientes, paises, ventas],
      [edge(clientes, paises), edge(ventas, clientes)],
    );

    const rank = ranks(graph);
    expect(rank.get(paises.oid)).toBe(0);
    expect(rank.get(clientes.oid)).toBe(1);
    expect(rank.get(ventas.oid)).toBe(2);
  });

  it("no se cuelga con un ciclo de claves foráneas", () => {
    const a = table("a");
    const b = table("b");
    const graph = graphOf([a, b], [edge(a, b), edge(b, a)]);

    const rank = ranks(graph);
    expect(rank.size).toBe(2);
    // Cuál de las dos queda primero depende del recorrido; lo que importa es que las dos tengan
    // rango y que no sea el mismo, o quedarían encimadas.
    expect(rank.get(a.oid)).not.toBe(rank.get(b.oid));
  });

  it("una autorreferencia no cambia el rango", () => {
    const empleados = table("empleados", ["id", "jefe_id"]);
    const graph = graphOf(
      [empleados],
      [edge(empleados, empleados, { sourceColumns: ["jefe_id"] })],
    );

    expect(ranks(graph).get(empleados.oid)).toBe(0);
  });

  it("una referencia fuera del esquema no cuenta para el rango", () => {
    const pedidos = table("pedidos");
    const graph = graphOf(
      [pedidos],
      [{ ...edge(pedidos, pedidos), target: 9999, targetLabel: "otro.catalogo" }],
    );

    expect(ranks(graph).get(pedidos.oid)).toBe(0);
  });
});

describe("layout", () => {
  it("separa las capas en horizontal y apila dentro de cada una", () => {
    const clientes = table("clientes");
    const ventas = table("ventas");
    const pagos = table("pagos");
    const graph = graphOf([clientes, pagos, ventas], [edge(ventas, clientes), edge(pagos, clientes)]);

    const { boxes } = layout(graph);
    const at = (name: string) => boxes.find((box) => box.table.name === name)!;

    expect(at("ventas").x).toBeGreaterThan(at("clientes").x);
    expect(at("pagos").x).toBe(at("ventas").x);
    expect(at("pagos").y).not.toBe(at("ventas").y);
  });

  it("es determinista", () => {
    const clientes = table("clientes");
    const ventas = table("ventas");
    const graph = graphOf([clientes, ventas], [edge(ventas, clientes)]);

    expect(layout(graph)).toEqual(layout(graph));
  });

  it("respeta lo que el usuario arrastró", () => {
    const clientes = table("clientes");
    const graph = graphOf([clientes], []);

    const { boxes } = layout(graph, { [clientes.oid]: { x: 400, y: 250 } });
    expect(boxes[0].x).toBe(400);
    expect(boxes[0].y).toBe(250);
  });

  it("recorta las tablas con muchas columnas", () => {
    const nombres = Array.from({ length: 20 }, (_, index) => `columna_${index}`);
    const ancha = table("ancha", nombres);

    const { boxes } = layout(graphOf([ancha], []));
    expect(boxes[0].columns).toHaveLength(METRICS.maxColumns);
    expect(boxes[0].hidden).toBe(20 - METRICS.maxColumns);
  });

  it("engancha la flecha en la fila de la columna de la clave", () => {
    const clientes = table("clientes");
    const ventas = table("ventas", ["id", "cliente_id"]);
    const graph = graphOf(
      [clientes, ventas],
      [edge(ventas, clientes, { sourceColumns: ["cliente_id"] })],
    );

    const { boxes, links } = layout(graph);
    const ventasBox = boxes.find((box) => box.table.name === "ventas")!;
    const fila = ventasBox.y + METRICS.headerHeight + METRICS.rowHeight + METRICS.rowHeight / 2;
    expect(links[0].points[0].y).toBe(fila);
  });

  it("la referencia a otro esquema sale al aire con su etiqueta", () => {
    const pedidos = table("pedidos");
    const graph = graphOf(
      [pedidos],
      [{ ...edge(pedidos, pedidos), target: 9999, targetLabel: "otro.catalogo" }],
    );

    const { links } = layout(graph);
    expect(links[0].external).toBe(true);
    expect(links[0].points).toHaveLength(2);
    expect(links[0].edge.targetLabel).toBe("otro.catalogo");
  });

  it("la autorreferencia se dibuja como bucle al costado", () => {
    const empleados = table("empleados", ["id", "jefe_id"]);
    const graph = graphOf(
      [empleados],
      [edge(empleados, empleados, { sourceColumns: ["jefe_id"] })],
    );

    const { boxes, links } = layout(graph);
    expect(links[0].self).toBe(true);
    // El bucle sale y vuelve por el mismo lado.
    const derecha = boxes[0].x + boxes[0].width;
    expect(links[0].points[0].x).toBe(derecha);
    expect(links[0].points.at(-1)!.x).toBe(derecha);
  });
});

describe("pathOf", () => {
  it("arma el recorrido del path", () => {
    const clientes = table("clientes");
    const ventas = table("ventas");
    const { links } = layout(graphOf([clientes, ventas], [edge(ventas, clientes)]));

    expect(pathOf(links[0])).toMatch(/^M [\d.]+ [\d.]+( L [\d.]+ [\d.]+)+$/);
  });
});

describe("neighbors", () => {
  it("trae las tablas del otro extremo, en las dos direcciones", () => {
    const clientes = table("clientes");
    const ventas = table("ventas");
    const pagos = table("pagos");
    const graph = graphOf([clientes, pagos, ventas], [edge(ventas, clientes), edge(pagos, ventas)]);

    expect(neighbors(graph, ventas.oid)).toEqual(new Set([clientes.oid, pagos.oid]));
    expect(neighbors(graph, clientes.oid)).toEqual(new Set([ventas.oid]));
  });
});
