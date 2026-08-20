import { describe, expect, it } from "vitest";
import { stickyIndex, type StickyRow } from "./tree-sticky";

const node = (level: number): StickyRow => ({ level, isSection: false, isGroup: false });
const folder = (level: number): StickyRow => ({ level, isSection: true, isGroup: false });
const group = (): StickyRow => ({ level: 0, isSection: true, isGroup: true });

describe("stickyIndex", () => {
  // servidor / base / carpeta «Esquemas» / esquema / carpeta «Tablas» / tabla, tabla
  const tree = [node(0), node(1), folder(2), node(3), folder(4), node(5), node(5)];

  it("ancla la carpeta más cercana, no la raíz", () => {
    expect(stickyIndex(tree, 6, true)).toBe(4);
  });

  it("el propio rótulo se ancla cuando es el que se está yendo por arriba", () => {
    expect(stickyIndex(tree, 4, true)).toBe(4);
  });

  it("no se ancla a sí mismo mientras la fila está entera en su lugar", () => {
    // Anclarla dibujaría una copia encima del original y se comería sus clics: la carpeta de arriba
    // de todo no se podía abrir ni renombrar.
    expect(stickyIndex(tree, 4, false)).toBeNull();
    expect(stickyIndex([group(), node(0)], 0, false)).toBeNull();
  });

  it("desde un nodo intermedio sube hasta la carpeta que lo contiene", () => {
    expect(stickyIndex(tree, 3, false)).toBe(2);
  });

  it("lo que no cuelga de ninguna carpeta no ancla nada", () => {
    expect(stickyIndex(tree, 1, false)).toBeNull();
    expect(stickyIndex(tree, 0, false)).toBeNull();
  });

  it("una carpeta de conexiones ancla a sus servidores, que están a su mismo nivel", () => {
    const roots = [group(), node(0), node(0)];
    expect(stickyIndex(roots, 2, false)).toBe(0);
  });

  it("un servidor suelto, sin carpeta arriba, no ancla nada", () => {
    expect(stickyIndex([node(0), node(0)], 1, false)).toBeNull();
  });

  it("un índice fuera de la lista no rompe", () => {
    expect(stickyIndex(tree, 99, true)).toBeNull();
    expect(stickyIndex([], 0, true)).toBeNull();
  });
});
