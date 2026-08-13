import { describe, expect, it } from "vitest";

import { guideAt, guideSpans } from "./tree-guides";

/**
 * Los niveles se escriben como una lista porque así es como los ve la función: el árbol ya viene
 * aplanado por `visibleRows`. Cada caso arma la forma que rompe una versión ingenua del cálculo.
 */

describe("guideSpans", () => {
  it("sin nada elegido no dibuja ninguna guía", () => {
    expect(guideSpans([0, 1, 2], -1)).toEqual([]);
  });

  it("una raíz elegida no tiene ancestros", () => {
    expect(guideSpans([0, 1, 1], 0)).toEqual([]);
  });

  it("devuelve una guía por cada ancestro, del más cercano al más lejano", () => {
    //  0 servidor
    //  1   base
    //  2     esquema
    //  3       tabla  ← elegida
    expect(guideSpans([0, 1, 2, 3], 3)).toEqual([
      { level: 2, from: 3, to: 3 },
      { level: 1, from: 2, to: 3 },
      { level: 0, from: 1, to: 3 },
    ]);
  });

  it("la guía llega hasta el último hijo del ancestro, no hasta la fila elegida", () => {
    //  0 servidor
    //  1   base
    //  2     ventas  ← elegida
    //  2     compras
    //  1   otra base
    const spans = guideSpans([0, 1, 2, 2, 1], 2);
    expect(spans).toContainEqual({ level: 1, from: 2, to: 3 });
    expect(spans).toContainEqual({ level: 0, from: 1, to: 4 });
  });

  it("no salta a un tío: el ancestro es el primer nivel de arriba hacia atrás", () => {
    //  0 servidor A
    //  1   base
    //  0 servidor B  ← el bloque de A quedó atrás
    //  1   base      ← elegida
    expect(guideSpans([0, 1, 0, 1], 3)).toEqual([{ level: 0, from: 3, to: 3 }]);
  });
});

describe("guideAt", () => {
  const spans = guideSpans([0, 1, 2, 2], 2);

  it("marca las filas de adentro del bloque", () => {
    expect(guideAt(spans, 2, 1)).toBe(true);
    expect(guideAt(spans, 3, 1)).toBe(true);
  });

  it("deja fuera al propio ancestro y a los niveles que no tienen guía", () => {
    expect(guideAt(spans, 1, 1)).toBe(false);
    expect(guideAt(spans, 2, 5)).toBe(false);
  });
});
