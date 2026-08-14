import { describe, expect, it } from "vitest";

import { visibleRows, type Row } from "./explorer.svelte";
import type { NodeKind } from "./ipc";

/**
 * `visibleRows` es lo único del explorador que se puede probar sin servidor ni ventana: recibe el
 * árbol y devuelve la lista de filas a dibujar. Lo que se verifica acá es cómo se combinan sus dos
 * criterios —la búsqueda y el filtro de conectados—, que es donde una fila puede desaparecer sin
 * que se note por qué.
 */

function server(name: string, connected: boolean, children: Row[] = []): Row {
  return {
    kind: "server",
    key: name,
    profileId: name,
    node: null,
    level: 0,
    label: name,
    hasChildren: children.length > 0,
    expanded: children.length > 0,
    loading: false,
    children: children.length > 0 ? children : null,
    connected,
  };
}

function node(label: string, parent: string): Row {
  return {
    kind: "node",
    key: `${parent}:${label}`,
    profileId: parent,
    node: null,
    level: 1,
    label,
    hasChildren: false,
    expanded: false,
    loading: false,
    children: null,
    connected: true,
  };
}

function group(name: string, servers: Row[], expanded = true): Row {
  return {
    kind: "group",
    key: `g:${name}`,
    profileId: "",
    group: name,
    node: null,
    level: 0,
    label: name,
    hasChildren: servers.length > 0,
    expanded,
    loading: false,
    children: servers,
    connected: false,
  };
}

const labels = (rows: Row[]) => rows.map((row) => row.label);

describe("visibleRows", () => {
  it("aplana solo lo expandido", () => {
    const abierto = server("abierto", true, [node("ventas", "abierto")]);
    const cerrado = server("cerrado", true, [node("compras", "cerrado")]);
    cerrado.expanded = false;

    expect(labels(visibleRows([abierto, cerrado]))).toEqual(["abierto", "ventas", "cerrado"]);
  });

  it("sin filtro muestra los servidores sin conectar", () => {
    const rows = visibleRows([server("prod", false), server("local", true)]);
    expect(labels(rows)).toEqual(["prod", "local"]);
  });

  it("con el filtro puesto deja fuera a los servidores sin conectar", () => {
    const rows = visibleRows([server("prod", false), server("local", true)], "", true);
    expect(labels(rows)).toEqual(["local"]);
  });

  it("una carpeta se va con el último de sus servidores conectados", () => {
    const vacia = group("apagados", [server("uno", false), server("dos", false)]);
    const viva = group("en uso", [server("tres", false), server("cuatro", true)]);

    expect(labels(visibleRows([vacia, viva], "", true))).toEqual(["en uso", "cuatro"]);
  });

  it("el filtro no toca lo que cuelga de un servidor conectado", () => {
    const abierto = server("local", true, [node("ventas", "local")]);
    expect(labels(visibleRows([abierto], "", true))).toEqual(["local", "ventas"]);
  });

  it("buscar trae la coincidencia y su ancestro aunque el nodo esté cerrado", () => {
    const abierto = server("local", true, [node("ventas", "local"), node("compras", "local")]);
    abierto.expanded = false;

    expect(labels(visibleRows([abierto], "vent"))).toEqual(["local", "ventas"]);
  });

  it("buscar con el filtro puesto no alcanza a los servidores escondidos", () => {
    const apagado = server("respaldo", false, [node("ventas", "respaldo")]);
    const encendido = server("local", true, [node("ventas", "local")]);

    expect(labels(visibleRows([apagado, encendido], "ventas", true))).toEqual(["local", "ventas"]);
    expect(labels(visibleRows([apagado, encendido], "ventas"))).toEqual([
      "respaldo",
      "ventas",
      "local",
      "ventas",
    ]);
  });
});

describe("búsqueda con prefijo de tipo", () => {
  /** Un nodo del catálogo con su clase, que es lo que mira el prefijo. */
  function object(label: string, kind: NodeKind, parent: string): Row {
    const row = node(label, parent);
    row.node = {
      id: label,
      label,
      kind,
      hasChildren: false,
      database: "app",
    };
    return row;
  }

  const tree = [
    server("local", true, [
      object("facturas", "table", "local"),
      object("facturas_activas", "view", "local"),
      object("facturas_id_seq", "sequence", "local"),
    ]),
  ];

  it("acota a la familia sin dejar de mostrar el camino hasta ella", () => {
    expect(visibleRows(tree, "t:factura").map((row) => row.label)).toEqual(["local", "facturas"]);
  });

  it("el prefijo solo lista la familia entera de lo que ya está cargado", () => {
    expect(visibleRows(tree, "v:").map((row) => row.label)).toEqual([
      "local",
      "facturas_activas",
    ]);
  });

  it("sin prefijo sigue buscando por nombre en todo", () => {
    expect(visibleRows(tree, "factura")).toHaveLength(4);
  });

  it("el servidor no entra por su nombre cuando se acotó a un tipo", () => {
    expect(visibleRows(tree, "t:local")).toEqual([]);
  });
});
