import { describe, expect, it } from "vitest";
import { folderLabel } from "./folder-label";

describe("folderLabel", () => {
  it("rotula todas las carpetas que manda el núcleo, con los nombres de serde", () => {
    // Estos dos eran «fdws» y «fservers» en la interfaz y el núcleo manda el nombre entero en
    // camelCase: la fila quedaba sin rótulo y nada fallaba al compilar.
    expect(folderLabel("foreignDataWrappers")).toBe("Wrappers foráneos");
    expect(folderLabel("foreignServers")).toBe("Servidores foráneos");
    expect(folderLabel("tables")).toBe("Tablas");
  });

  it("devuelve undefined ante una carpeta desconocida en vez de inventar un rótulo", () => {
    expect(folderLabel("algoNuevo" as never)).toBeUndefined();
  });
});
