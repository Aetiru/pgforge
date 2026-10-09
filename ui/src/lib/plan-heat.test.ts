import { describe, expect, it } from "vitest";

import { heatLevel, planHeat, selfCost } from "./plan-heat";
import type { PlanNode } from "./ipc";

function node(over: Partial<PlanNode>): PlanNode {
  return {
    nodeType: "Seq Scan",
    relation: null,
    schema: null,
    index: null,
    condition: null,
    filter: null,
    startupCost: 0,
    totalCost: 0,
    planRows: 0,
    actualRows: null,
    loops: null,
    totalMs: null,
    selfMs: null,
    rowsRemoved: null,
    misestimated: false,
    sharedHitBlocks: null,
    sharedReadBlocks: null,
    sortMethod: null,
    sortSpaceKb: null,
    sortOnDisk: false,
    children: [],
    ...over,
  };
}

describe("selfCost", () => {
  it("resta el costo de los hijos", () => {
    const plan = node({ totalCost: 100, children: [node({ totalCost: 70 }), node({ totalCost: 10 })] });
    expect(selfCost(plan)).toBe(20);
  });

  it("no baja de cero cuando el planificador subestima al padre", () => {
    const plan = node({ totalCost: 10, children: [node({ totalCost: 50 })] });
    expect(selfCost(plan)).toBe(0);
  });
});

describe("planHeat", () => {
  it("mide en milisegundos si el plan se ejecutó", () => {
    const plan = node({ selfMs: 1, children: [node({ selfMs: 9 })] });
    expect(planHeat(plan)).toEqual({ measure: "ms", worst: 9, total: 10 });
  });

  it("cae al costo propio cuando es solo la estimación", () => {
    const plan = node({ totalCost: 100, children: [node({ totalCost: 90 })] });
    expect(planHeat(plan)).toEqual({ measure: "cost", worst: 90, total: 100 });
  });
});

describe("heatLevel", () => {
  it("separa lo que pesa de lo que no", () => {
    expect([0, 0.04, 0.05, 0.15, 0.3, 0.6, 1].map(heatLevel)).toEqual([0, 0, 1, 2, 3, 4, 4]);
  });
});
