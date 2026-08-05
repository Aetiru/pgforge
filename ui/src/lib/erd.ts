/**
 * Posiciones del diagrama ERD.
 *
 * El núcleo entrega el grafo y no lo posiciona, así que el layout es cosa de la interfaz: depende
 * del ancho del texto en pantalla y de lo que el usuario arrastre. Vive fuera del componente
 * porque es lo único con lógica de verdad —rangos, ciclos, recorrido de las aristas— y así se
 * prueba sin montar nada.
 *
 * El layout es en capas y **determinista**: el mismo esquema tiene que salir siempre igual, o cada
 * refresco movería el diagrama debajo del cursor.
 */

import type { GraphColumn, GraphEdge, GraphTable, SchemaGraph } from "./ipc";

/** Métricas del dibujo. Van acá y no en el CSS porque el layout necesita medir antes de dibujar. */
export const METRICS = {
  /** Ancho de un carácter del monoespaciado a 12px. Alcanza para estimar el ancho de la caja. */
  charWidth: 7,
  padding: 10,
  headerHeight: 26,
  rowHeight: 18,
  minWidth: 150,
  /** Separación entre capas y entre cajas de la misma capa. */
  gapX: 110,
  gapY: 36,
  /** Cuántas columnas entran en la caja. Una tabla de sesenta empujaría todo el diagrama. */
  maxColumns: 12,
  /** Cuánto sobresale la flecha de una referencia que sale del esquema. */
  externalStub: 70,
};

export interface Point {
  x: number;
  y: number;
}

export interface ErdBox {
  oid: number;
  table: GraphTable;
  x: number;
  y: number;
  width: number;
  height: number;
  /** Las columnas que entran en la caja. */
  columns: GraphColumn[];
  /** Cuántas quedaron afuera; 0 si entraron todas. */
  hidden: number;
}

export interface ErdLink {
  edge: GraphEdge;
  /** Recorrido de la flecha, del origen al destino. */
  points: Point[];
  /** La tabla se referencia a sí misma: se dibuja como bucle sobre el costado. */
  self: boolean;
  /** La referida está fuera del esquema: la flecha muere en el aire, con la etiqueta al final. */
  external: boolean;
}

export interface ErdLayout {
  boxes: ErdBox[];
  links: ErdLink[];
  width: number;
  height: number;
}

/** Posiciones que el usuario movió a mano, por OID. Pisan lo que calcula el layout. */
export type Positions = Record<number, Point>;

export function layout(graph: SchemaGraph, moved: Positions = {}): ErdLayout {
  const boxes = place(graph, moved);
  const byOid = new Map(boxes.map((box) => [box.oid, box]));
  const links = graph.edges
    .map((edge) => link(edge, byOid))
    .filter((item): item is ErdLink => item !== null);

  const width = boxes.reduce((max, box) => Math.max(max, box.x + box.width), 0);
  const height = boxes.reduce((max, box) => Math.max(max, box.y + box.height), 0);

  return {
    boxes,
    links,
    width: width + METRICS.gapX,
    height: height + METRICS.gapY,
  };
}

/**
 * Rango de cada tabla: cuántos saltos de clave foránea hay desde ella hasta una que no referencia
 * a nadie. Las referidas quedan a la izquierda y las que referencian a la derecha.
 *
 * Un ciclo de claves foráneas es legal en PostgreSQL, así que la arista de retroceso se ignora en
 * vez de esperar un DAG: no se puede colgar la interfaz por un modelo que el servidor aceptó.
 */
export function ranks(graph: SchemaGraph): Map<number, number> {
  const own = new Set(graph.tables.map((table) => table.oid));
  const targets = new Map<number, number[]>();
  for (const edge of graph.edges) {
    if (edge.source === edge.target || !own.has(edge.target)) continue;
    const list = targets.get(edge.source) ?? [];
    list.push(edge.target);
    targets.set(edge.source, list);
  }

  const rank = new Map<number, number>();
  const visiting = new Set<number>();

  function visit(oid: number): number {
    const known = rank.get(oid);
    if (known !== undefined) return known;
    if (visiting.has(oid)) return 0;

    visiting.add(oid);
    let value = 0;
    for (const target of targets.get(oid) ?? []) {
      value = Math.max(value, visit(target) + 1);
    }
    visiting.delete(oid);

    rank.set(oid, value);
    return value;
  }

  for (const table of graph.tables) visit(table.oid);
  return rank;
}

function place(graph: SchemaGraph, moved: Positions): ErdBox[] {
  const rank = ranks(graph);
  const degree = new Map<number, number>();
  for (const edge of graph.edges) {
    degree.set(edge.source, (degree.get(edge.source) ?? 0) + 1);
    degree.set(edge.target, (degree.get(edge.target) ?? 0) + 1);
  }

  const sized = graph.tables.map((table) => size(table));

  // Dentro de una capa van primero las tablas más conectadas, y el desempate es alfabético para
  // que el orden no dependa de cómo devolvió las filas el servidor.
  const layers = new Map<number, ErdBox[]>();
  for (const box of sized) {
    const layer = rank.get(box.oid) ?? 0;
    const list = layers.get(layer) ?? [];
    list.push(box);
    layers.set(layer, list);
  }
  for (const list of layers.values()) {
    list.sort((a, b) => {
      const byDegree = (degree.get(b.oid) ?? 0) - (degree.get(a.oid) ?? 0);
      return byDegree !== 0 ? byDegree : a.table.name.localeCompare(b.table.name);
    });
  }

  let x = METRICS.gapX / 2;
  for (const layer of [...layers.keys()].sort((a, b) => a - b)) {
    const list = layers.get(layer) ?? [];
    let y = METRICS.gapY;
    const widest = list.reduce((max, box) => Math.max(max, box.width), 0);

    for (const box of list) {
      box.x = x;
      box.y = y;
      y += box.height + METRICS.gapY;
    }
    x += widest + METRICS.gapX;
  }

  // Lo que el usuario arrastró manda sobre lo calculado.
  for (const box of sized) {
    const position = moved[box.oid];
    if (position) {
      box.x = position.x;
      box.y = position.y;
    }
  }

  return sized;
}

function size(table: GraphTable): ErdBox {
  const columns = table.columns.slice(0, METRICS.maxColumns);
  const hidden = table.columns.length - columns.length;

  const longest = columns.reduce(
    (max, column) => Math.max(max, `${column.name}  ${column.typeName}`.length),
    table.name.length + 2,
  );
  const width = Math.max(METRICS.minWidth, longest * METRICS.charWidth + METRICS.padding * 2);
  const rows = columns.length + (hidden > 0 ? 1 : 0);

  return {
    oid: table.oid,
    table,
    x: 0,
    y: 0,
    width,
    height: METRICS.headerHeight + rows * METRICS.rowHeight + METRICS.padding,
    columns,
    hidden,
  };
}

/** Alto al que se engancha la flecha: la fila de la columna si se ve, o el medio de la caja. */
function anchorY(box: ErdBox, column: string | undefined): number {
  const index = box.columns.findIndex((item) => item.name === column);
  if (index < 0) return box.y + box.height / 2;
  return box.y + METRICS.headerHeight + index * METRICS.rowHeight + METRICS.rowHeight / 2;
}

function link(edge: GraphEdge, boxes: Map<number, ErdBox>): ErdLink | null {
  const source = boxes.get(edge.source);
  // Sin caja de origen no hay nada que dibujar; no debería pasar, las aristas salen del esquema.
  if (!source) return null;

  const from = anchorY(source, edge.sourceColumns[0]);
  const target = boxes.get(edge.target);

  if (!target) {
    // Referencia a otro esquema: la flecha sale del costado y termina ahí, con su etiqueta.
    return {
      edge,
      self: false,
      external: true,
      points: [
        { x: source.x + source.width, y: from },
        { x: source.x + source.width + METRICS.externalStub, y: from },
      ],
    };
  }

  if (target.oid === source.oid) {
    const to = anchorY(source, edge.targetColumns[0]);
    const out = source.x + source.width;
    const loop = out + METRICS.gapX / 3;
    return {
      edge,
      self: true,
      external: false,
      points: [
        { x: out, y: from },
        { x: loop, y: from },
        { x: loop, y: to },
        { x: out, y: to },
      ],
    };
  }

  const to = anchorY(target, edge.targetColumns[0]);
  // La flecha sale por el lado que mira a la otra caja, con un codo en el medio del hueco.
  const rightwards = target.x + target.width / 2 >= source.x + source.width / 2;
  const start = { x: rightwards ? source.x + source.width : source.x, y: from };
  const end = { x: rightwards ? target.x : target.x + target.width, y: to };
  const middle = (start.x + end.x) / 2;

  return {
    edge,
    self: false,
    external: false,
    points: [start, { x: middle, y: from }, { x: middle, y: to }, end],
  };
}

/** El recorrido como atributo `d` de un `<path>`. */
export function pathOf(link: ErdLink): string {
  return link.points
    .map((point, index) => `${index === 0 ? "M" : "L"} ${point.x} ${point.y}`)
    .join(" ");
}

/** Las tablas del otro extremo de las aristas que tocan a `oid`, para resaltar la selección. */
export function neighbors(graph: SchemaGraph, oid: number): Set<number> {
  const found = new Set<number>();
  for (const edge of graph.edges) {
    if (edge.source === oid) found.add(edge.target);
    if (edge.target === oid) found.add(edge.source);
  }
  return found;
}
