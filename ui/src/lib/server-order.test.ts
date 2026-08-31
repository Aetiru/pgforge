import { describe, expect, it } from "vitest";
import { compareServers, reorderDrop } from "./server-order";
import type { ConnectionProfile } from "./ipc";

function profile(id: string, name: string, extra: Partial<ConnectionProfile> = {}): ConnectionProfile {
  return {
    id,
    name,
    host: "localhost",
    port: 5432,
    database: "postgres",
    user: "postgres",
    sslMode: "prefer",
    connectTimeoutSecs: 10,
    savePassword: false,
    readOnly: false,
    autocommit: true,
    ...extra,
  };
}

describe("compareServers", () => {
  it("con order en los dos, gana el número más chico", () => {
    const a = profile("a", "zeta", { order: 1 });
    const b = profile("b", "alfa", { order: 0 });
    expect(compareServers(a, b)).toBeGreaterThan(0);
  });

  it("los que tienen order van antes que los que no, sin importar el nombre", () => {
    const withOrder = profile("a", "zeta", { order: 5 });
    const withoutOrder = profile("b", "alfa");
    expect(compareServers(withOrder, withoutOrder)).toBeLessThan(0);
  });

  it("empatados en order (los dos ausentes), decide el nombre", () => {
    const a = profile("a", "zeta");
    const b = profile("b", "alfa");
    expect(compareServers(a, b)).toBeGreaterThan(0);
  });
});

describe("reorderDrop", () => {
  it("inserta en medio de una carpeta", () => {
    const profiles = [
      profile("a", "a", { group: "Clientes", order: 0 }),
      profile("b", "b", { group: "Clientes", order: 1 }),
      profile("c", "c", { group: "Clientes", order: 2 }),
    ];
    const patches = reorderDrop(profiles, "c", "Clientes", "b");
    expect(patches).toEqual([
      { id: "a", group: "Clientes", order: 0 },
      { id: "c", group: "Clientes", order: 1 },
      { id: "b", group: "Clientes", order: 2 },
    ]);
  });

  it("mueve a otra carpeta insertando en una posición dada", () => {
    const profiles = [
      profile("a", "a", { group: "Origen", order: 0 }),
      profile("b", "b", { group: "Destino", order: 0 }),
      profile("c", "c", { group: "Destino", order: 1 }),
    ];
    const patches = reorderDrop(profiles, "a", "Destino", "c");
    expect(patches).toEqual([
      { id: "b", group: "Destino", order: 0 },
      { id: "a", group: "Destino", order: 1 },
      { id: "c", group: "Destino", order: 2 },
    ]);
  });

  it("soltar al final (beforeId nulo) manda el arrastrado al final de la lista", () => {
    const profiles = [profile("a", "a", { order: 0 }), profile("b", "b", { order: 1 })];
    const patches = reorderDrop(profiles, "a", null, null);
    expect(patches).toEqual([
      { id: "b", group: null, order: 0 },
      { id: "a", group: null, order: 1 },
    ]);
  });

  it("soltar sobre sí mismo es un no-op", () => {
    const profiles = [profile("a", "a", { group: "Clientes", order: 0 })];
    expect(reorderDrop(profiles, "a", "Clientes", "a")).toEqual([]);
  });

  it("no toca el order de perfiles de otras carpetas", () => {
    const profiles = [
      profile("a", "a", { group: "Clientes", order: 0 }),
      profile("b", "b", { group: "Clientes", order: 1 }),
      profile("x", "x", { group: "Otros", order: 7 }),
    ];
    const patches = reorderDrop(profiles, "b", "Clientes", "a");
    expect(patches.some((patch) => patch.id === "x")).toBe(false);
  });
});
