import { describe, expect, it } from "vitest";
import { dropQuestion, headerActions, type ActionContext } from "./detail-actions";
import { flagsOf } from "./detail-node";
import type { NodeKind, TreeNode } from "./ipc";

function node(kind: NodeKind, extra: Partial<TreeNode> = {}): TreeNode {
  return {
    id: "n",
    label: "clientes",
    kind,
    hasChildren: false,
    database: "app",
    ...extra,
  };
}

function context(current: TreeNode | null, extra: Partial<ActionContext> = {}): ActionContext {
  return {
    flags: flagsOf(current),
    node: current,
    isServer: false,
    isGroup: false,
    connected: false,
    label: current?.label ?? "",
    hasShape: false,
    hasDdl: false,
    dataTarget: null,
    queryTarget: null,
    hasCommentTarget: false,
    ...extra,
  };
}

const kinds = (context: ActionContext) => headerActions(context).map((action) => action.kind);

describe("headerActions", () => {
  it("lo que borra va último: se llega después de todo lo demás", () => {
    const table = node("table", { schema: "public", oid: 42 });
    const list = kinds(
      context(table, { hasShape: true, hasDdl: true, hasCommentTarget: true, dataTarget: 42 }),
    );
    expect(list.at(-1)).toBe("dropTable");
    expect(list.indexOf("export")).toBeLessThan(list.indexOf("dropTable"));
  });

  it("una carpeta ofrece crear lo que contiene, y nada más", () => {
    expect(kinds(context(node({ folder: "views" }, { schema: "public" })))).toEqual([
      "newView",
    ]);
    expect(kinds(context(node({ folder: "types" }, { schema: "public" })))).toEqual([
      "newEnum",
      "newComposite",
      "newDomain",
    ]);
  });

  it("una carpeta de esquema sin esquema no ofrece crear: no habría dónde", () => {
    expect(kinds(context(node({ folder: "tables" })))).toEqual([]);
  });

  it("el servidor conectado ofrece desconectar y el apagado, conectar", () => {
    const connected = kinds(context(null, { isServer: true, connected: true, label: "Local" }));
    expect(connected).toEqual(["disconnect", "editServer", "deleteServer"]);
    const off = kinds(context(null, { isServer: true, connected: false, label: "Local" }));
    expect(off).toEqual(["connect", "editServer", "deleteServer"]);
  });

  it("editar una función necesita su DDL, que es de donde sale el cuerpo", () => {
    const routine = node("function", { schema: "app", oid: 7 });
    expect(kinds(context(routine))).not.toContain("editFunction");
    expect(kinds(context(routine, { hasDdl: true }))).toContain("editFunction");
  });

  it("borrar la tabla necesita su forma, que es de donde sale el nombre real", () => {
    const table = node("table", { schema: "public", oid: 42 });
    expect(kinds(context(table))).not.toContain("dropTable");
    expect(kinds(context(table, { hasShape: true }))).toContain("dropTable");
  });

  it("lo que modifica el servidor va marcado para que lo apague el modo de solo lectura", () => {
    const table = node("table", { schema: "public", oid: 42 });
    const actions = headerActions(context(table, { hasShape: true, dataTarget: 42 }));
    const guarded = (kind: string) => actions.find((action) => action.kind === kind)?.guarded;
    expect(guarded("import")).toBe(true);
    expect(guarded("dropTable")).toBe(true);
    // Exportar y mirar no cambian nada del servidor.
    expect(guarded("export")).toBeUndefined();
  });

  it("una carpeta de conexiones solo se renombra", () => {
    expect(kinds(context(null, { isGroup: true, label: "Producción" }))).toEqual(["renameGroup"]);
  });

  it("la partición se ofrece solo en una tabla particionada ya leída", () => {
    expect(kinds(context(node("partitionedTable", { schema: "public", oid: 9 })))).toContain(
      "newPartition",
    );
    expect(kinds(context(node("table", { schema: "public", oid: 9 })))).not.toContain(
      "newPartition",
    );
  });
});

describe("dropQuestion", () => {
  it("avisa lo que se pierde, no solo lo que se borra", () => {
    expect(dropQuestion({ kind: "table", label: "clientes" })).toContain("Se pierden sus datos");
    expect(dropQuestion({ kind: "materializedView", label: "resumen" })).toContain(
      "Se pierden los datos guardados",
    );
  });

  it("una función se nombra con su firma", () => {
    expect(
      dropQuestion({
        kind: "function",
        schema: "app",
        name: "recalcular",
        args: "integer",
        procedure: true,
      }),
    ).toBe("¿Eliminar el procedimiento recalcular(integer)?");
  });

  it("la política avisa del caso que deja la tabla sin nada visible", () => {
    expect(dropQuestion({ kind: "policy", label: "solo_mias" })).toContain(
      "la tabla queda sin nada visible",
    );
  });
});
