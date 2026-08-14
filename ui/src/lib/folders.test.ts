import { beforeEach, describe, expect, it } from "vitest";

import { folders, LOOSE_GROUP, normalizeGroup, renamedPath } from "./folders.svelte";

/**
 * Los tests corren en Node y no en un navegador —son de lógica pura, sin DOM—, así que el
 * almacenamiento se reemplaza por un Map. Lo que se prueba no es `localStorage` sino lo que se
 * rompe en silencio: una carpeta que se renombra y pierde que estaba cerrada, o una vacía que
 * sobrevive a haberse llenado.
 */
const store = new Map<string, string>();
globalThis.localStorage = {
  getItem: (key: string) => store.get(key) ?? null,
  setItem: (key: string, value: string) => void store.set(key, value),
  removeItem: (key: string) => void store.delete(key),
  clear: () => store.clear(),
  key: (index: number) => [...store.keys()][index] ?? null,
  get length() {
    return store.size;
  },
} as Storage;

beforeEach(() => {
  localStorage.clear();
  folders.keepEmpty([]);
  for (const name of ["ventas", "compras", "nueva", LOOSE_GROUP]) folders.setOpen(name, true);
});

describe("abrir y cerrar", () => {
  it("una carpeta que nadie tocó está abierta", () => {
    expect(folders.isOpen("ventas")).toBe(true);
  });

  it("recuerda la que se cerró, sin tocar a las demás", () => {
    folders.setOpen("ventas", false);
    expect(folders.isOpen("ventas")).toBe(false);
    expect(folders.isOpen("compras")).toBe(true);
  });

  it("la sección de los sueltos se recuerda igual que una carpeta", () => {
    folders.setOpen(LOOSE_GROUP, false);
    expect(folders.isOpen(LOOSE_GROUP)).toBe(false);
  });

  it("cerrar dos veces no la anota dos veces", () => {
    folders.setOpen("ventas", false);
    folders.setOpen("ventas", false);
    folders.setOpen("ventas", true);
    expect(folders.isOpen("ventas")).toBe(true);
  });
});

describe("carpetas vacías", () => {
  it("las recuerda y las olvida", () => {
    folders.remember("nueva");
    expect(folders.empty).toEqual(["nueva"]);

    folders.forget("nueva");
    expect(folders.empty).toEqual([]);
  });

  it("no repite la misma", () => {
    folders.remember("nueva");
    folders.remember("nueva");
    expect(folders.empty).toEqual(["nueva"]);
  });

  it("`keepEmpty` deja exactamente las que se le pasan", () => {
    folders.remember("nueva");
    folders.remember("otra");
    folders.keepEmpty(["otra"]);
    expect(folders.empty).toEqual(["otra"]);
  });
});

describe("renombrar", () => {
  it("se lleva puesto que estaba cerrada", () => {
    folders.setOpen("ventas", false);
    folders.rename("ventas", "ventas 2026");

    expect(folders.isOpen("ventas")).toBe(true);
    expect(folders.isOpen("ventas 2026")).toBe(false);
  });

  it("se lleva puesto que estaba vacía", () => {
    folders.remember("nueva");
    folders.rename("nueva", "otra");
    expect(folders.empty).toEqual(["otra"]);
  });

  it("deshacer una carpeta no deja nada suyo atrás", () => {
    folders.setOpen("ventas", false);
    folders.remember("ventas");
    folders.rename("ventas");

    expect(folders.isOpen("ventas")).toBe(true);
    expect(folders.empty).toEqual([]);
  });
});

describe("carpetas anidadas", () => {
  it("normaliza tramo por tramo, como el almacén de perfiles", () => {
    expect(normalizeGroup("  Clientes / ACME ")).toBe("Clientes/ACME");
    expect(normalizeGroup("Clientes//ACME")).toBe("Clientes/ACME");
    expect(normalizeGroup("   /  ")).toBe("");
  });

  it("renombrar arrastra a las de adentro", () => {
    expect(renamedPath("Clientes/ACME", "Clientes", "Cuentas")).toBe("Cuentas/ACME");
    expect(renamedPath("Clientes", "Clientes", "Cuentas")).toBe("Cuentas");
  });

  it("deshacer sube un escalón a las de adentro y suelta a la propia", () => {
    expect(renamedPath("Clientes/ACME", "Clientes")).toBe("ACME");
    expect(renamedPath("Clientes", "Clientes")).toBeNull();
  });

  it("una carpeta que solo comparte el prefijo no se toca", () => {
    expect(renamedPath("Clientescopia", "Clientes", "Cuentas")).toBe("Clientescopia");
  });

  it("lo recordado de una carpeta viaja con las de adentro", () => {
    folders.setOpen("Clientes/ACME", false);
    folders.rename("Clientes", "Cuentas");

    expect(folders.isOpen("Cuentas/ACME")).toBe(false);
    expect(folders.isOpen("Clientes/ACME")).toBe(true);
  });
});
