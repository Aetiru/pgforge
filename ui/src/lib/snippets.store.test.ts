import { beforeEach, describe, expect, it, vi } from "vitest";

// El almacén usa `$state`, que solo compila dentro de un `.svelte.ts`: este test lo importa igual
// porque Vitest pasa por el mismo plugin. Los comandos se simulan en el borde del IPC.
const calls: string[] = [];
let list: unknown[] = [];

vi.mock("./ipc", () => ({
  describeError: (e: unknown) => String(e),
  snippetsList: async () => list,
  snippetUsage: async () => ({ a: { uses: 2, lastUsedAt: 10 } }),
  snippetUsed: async (id: string) => {
    calls.push(`used:${id}`);
  },
  snippetSave: async (s: unknown) => {
    calls.push("save");
    list = [...list.filter((x) => (x as { id: string }).id !== (s as { id: string }).id), s];
    return list;
  },
  snippetDelete: async (id: string) => {
    calls.push(`delete:${id}`);
    list = list.filter((x) => (x as { id: string }).id !== id);
    return list;
  },
  snippetsReset: async () => list,
  snippetsExport: async (_p: string, ids: string[] | null) => ids?.length ?? list.length,
  snippetsImport: async () => ({ snippets: [{ id: "n", abbreviation: "n", body: "x", description: "", category: "", scope: { profileIds: [], databases: [] }, shortcut: null }], added: 1, replaced: 0, skipped: [] }),
}));

const make = (id: string, extra: Record<string, unknown> = {}) => ({
  id,
  abbreviation: id,
  body: "SELECT 1",
  description: "",
  category: "",
  scope: { profileIds: [], databases: [] },
  shortcut: null,
  ...extra,
});

describe("Snippets (almacén)", () => {
  beforeEach(() => {
    calls.length = 0;
    list = [make("a", { shortcut: "Mod-Alt-1" }), make("b", { scope: { profileIds: ["p1"], databases: [] } })];
    vi.resetModules();
  });

  async function fresh() {
    const { snippets } = await import("./snippets.svelte");
    await snippets.load();
    return snippets;
  }

  it("carga la lista y los usos", async () => {
    const snippets = await fresh();
    expect(snippets.items).toHaveLength(2);
    expect(snippets.usage.a.uses).toBe(2);
  });

  it("anotar un uso actualiza el contador en el acto y avisa al núcleo", async () => {
    const snippets = await fresh();
    snippets.noteUsed("a");
    expect(snippets.usage.a.uses).toBe(3);
    snippets.noteUsed("nueva");
    expect(snippets.usage.nueva.uses).toBe(1);
    expect(calls).toEqual(["used:a", "used:nueva"]);
  });

  it("borrar con deshacer devuelve lo borrado y se puede reponer con save", async () => {
    const snippets = await fresh();
    const removed = await snippets.removeWithUndo("a");
    expect(removed?.id).toBe("a");
    expect(snippets.items.map((i) => i.id)).toEqual(["b"]);
    expect(snippets.usage.a).toBeUndefined();
    await snippets.save(removed!);
    expect(snippets.items.map((i) => i.id).sort()).toEqual(["a", "b"]);
  });

  it("borrar una que no existe devuelve null y no llama al núcleo", async () => {
    const snippets = await fresh();
    expect(await snippets.removeWithUndo("zzz")).toBeNull();
    expect(calls).toEqual([]);
  });

  it("importar toma la lista que devuelve el núcleo", async () => {
    const snippets = await fresh();
    const report = await snippets.importFrom("/x.json", "skip");
    expect(report?.added).toBe(1);
    expect(snippets.items.map((i) => i.id)).toEqual(["n"]);
  });

  it("exportar devuelve cuántas escribió", async () => {
    const snippets = await fresh();
    expect(await snippets.exportTo("/x.json", null)).toBe(2);
    expect(await snippets.exportTo("/x.json", ["a"])).toBe(1);
  });

  it("filtra por servidor y base, y busca por atajo normalizado", async () => {
    const snippets = await fresh();
    expect(snippets.visibleFor("p9", null).map((i) => i.id)).toEqual(["a"]);
    expect(snippets.visibleFor("p1", null).map((i) => i.id)).toEqual(["a", "b"]);
    expect(snippets.byShortcut("Alt-Ctrl-1")?.id).toBe("a");
    expect(snippets.byShortcut("Mod-Alt-2")).toBeUndefined();
    expect(snippets.byShortcut("a")).toBeUndefined();
  });
});
