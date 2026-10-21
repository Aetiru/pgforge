import { describe, expect, it } from "vitest";
import { applyBlocker, descriptionText, revertBlocker, versionLabel } from "./migration-view";
import type { MigrationItem } from "./ipc";

function item(version: number, state: MigrationItem["state"], hasDown = false): MigrationItem {
  return {
    version,
    description: "x",
    state,
    checksum: "abc",
    appliedChecksum: null,
    appliedAt: null,
    hasDown,
    error: null,
    outOfOrder: false,
  };
}

describe("migration-view", () => {
  it("rellena el número a tres cifras y deja crecer los más grandes", () => {
    expect(versionLabel(12)).toBe("V012");
    expect(versionLabel(1234)).toBe("V1234");
    expect(descriptionText("crear_clientes")).toBe("crear clientes");
  });

  it("solo deja aplicar la primera pendiente", () => {
    const items = [item(1, "applied"), item(2, "pending"), item(3, "pending")];
    expect(applyBlocker(items, items[1])).toBeNull();
    expect(applyBlocker(items, items[2])).toContain("V002");
    expect(applyBlocker(items, items[0])).toBe("Ya está aplicada");
  });

  it("deja reintentar la que falló", () => {
    const items = [item(1, "applied"), item(2, "failed")];
    expect(applyBlocker(items, items[1])).toBeNull();
  });

  it("una modificada frena todo lo que sigue", () => {
    const items = [item(1, "changed"), item(2, "pending")];
    expect(applyBlocker(items, items[1])).toContain("V001");
  });

  it("deshacer pide reversa y va desde la última", () => {
    const items = [item(1, "applied", true), item(2, "applied", true), item(3, "applied", false)];
    expect(revertBlocker(items, items[2])).toContain("reversa");
    expect(revertBlocker(items, items[1])).toContain("V003");
    const two = items.slice(0, 2);
    expect(revertBlocker(two, two[1])).toBeNull();
    expect(revertBlocker(two, two[0])).toContain("V002");
    expect(revertBlocker([item(1, "pending", true)], item(1, "pending", true))).toBe("No está aplicada");
  });
});
