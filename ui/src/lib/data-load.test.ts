import { describe, expect, it, vi } from "vitest";

vi.mock("./ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./ipc")>()),
  dataOpen: async () => ({ key: { columns: ["id"] }, readOnly: null, columns: [] }),
  dataPage: async () => ({ columns: ["id"], rows: [["1"], ["2"]], next: null }),
}));

describe("DataTab.load", () => {
  it("trae la primera tanda sin esperar a que el scroll la pida", async () => {
    const { DataTab } = await import("./data.svelte");
    const tab = new DataTab("perfil", "base", "tabla", 1);
    await tab.load();
    expect(tab.rows).toHaveLength(2);
    expect(tab.complete).toBe(true);
    expect(tab.loading).toBe(false);
  });
});
