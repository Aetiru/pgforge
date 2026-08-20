import { describe, expect, it } from "vitest";
import { planText } from "./plan-text";
import type { Plan, PlanNode } from "./ipc";

function node(partial: Partial<PlanNode>): PlanNode {
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
    ...partial,
  };
}

function plan(root: PlanNode, partial: Partial<Plan> = {}): Plan {
  return {
    root,
    planningMs: null,
    executionMs: null,
    analyzed: false,
    advice: [],
    json: "",
    ...partial,
  };
}

describe("planText", () => {
  it("escribe el nodo con su costo y sobre qué relación corre", () => {
    const text = planText(
      plan(node({ relation: "public.trabajos", startupCost: 0, totalCost: 4521, planRows: 120 })),
    );

    expect(text).toContain("Seq Scan on public.trabajos  (cost=0.00..4521.00 rows=120)");
  });

  it("agrega lo medido cuando el plan se ejecutó", () => {
    const text = planText(
      plan(
        node({
          relation: "trabajos",
          totalCost: 4521,
          planRows: 120,
          actualRows: 118,
          loops: 1,
          totalMs: 38.4,
        }),
        { analyzed: true, executionMs: 38.5 },
      ),
    );

    expect(text.split("\n")).toEqual([
      "Seq Scan on trabajos  (cost=0.00..4521.00 rows=120) (actual time=38.400 rows=118 loops=1)",
      "Execution Time: 38.500 ms",
    ]);
  });

  it("cuelga cada hijo con la flecha e indenta lo que lo describe", () => {
    const hijo = node({
      nodeType: "Index Scan",
      relation: "pedidos",
      index: "pedidos_pkey",
      condition: "(id = 7)",
      totalCost: 8.3,
      planRows: 1,
    });
    const text = planText(
      plan(
        node({ nodeType: "Nested Loop", relation: null, totalCost: 20, planRows: 1, children: [hijo] }),
        { analyzed: true },
      ),
    );

    expect(text.split("\n")).toEqual([
      "Nested Loop  (cost=0.00..20.00 rows=1)",
      "  ->  Index Scan using pedidos_pkey on pedidos  (cost=0.00..8.30 rows=1)",
      "        Cond: (id = 7)",
    ]);
  });

  it("distingue la condición del índice del filtro que quedó encima", () => {
    const text = planText(
      plan(
        node({
          nodeType: "Index Scan",
          relation: "trabajos",
          index: "trabajos_fecha_idx",
          condition: "(fecha > '2024-01-01'::date)",
          filter: "(estado = 'activo'::text)",
          rowsRemoved: 80000,
          actualRows: 200,
          loops: 1,
        }),
        { analyzed: true },
      ),
    );

    expect(text).toContain("  Cond: (fecha > '2024-01-01'::date)");
    expect(text).toContain("  Filter: (estado = 'activo'::text)");
    expect(text).toContain("  Rows Removed by Filter: 80000");
  });

  it("dice cómo ordenó y si el orden se fue a disco", () => {
    const text = planText(
      plan(node({ nodeType: "Sort", sortMethod: "external merge", sortSpaceKb: 43008, sortOnDisk: true })),
    );

    expect(text).toContain("Sort Method: external merge  Disk: 43008kB");
  });

  it("avisa cuando lo que se copia es un plan estimado", () => {
    expect(planText(plan(node({})))).toContain("(plan estimado, sin ejecutar)");
  });
});
