import type { PlanNode } from "./ipc";

/**
 * Qué tan caliente es un nodo del plan: de 0 (no pesa) a 4 (es el culpable).
 *
 * Cinco escalones y no un degradé continuo: lo que se busca con la vista es «cuáles son los dos o
 * tres que pesan», y entre dos tonos casi iguales no hay forma de decidirlo. Los umbrales son
 * proporciones del nodo más caro del plan, no valores absolutos: un plan de 2 ms y uno de 40 s se
 * leen igual de bien.
 */
export type HeatLevel = 0 | 1 | 2 | 3 | 4;

export interface PlanHeat {
  /** `ms` cuando el plan se ejecutó (`ANALYZE`); `cost` cuando es solo la estimación del planificador. */
  measure: "ms" | "cost";
  /** El peso propio del nodo más caro, contra el que se mide el resto. */
  worst: number;
  /** La suma del peso propio de todos los nodos, para decir qué parte del total es cada uno. */
  total: number;
}

/**
 * El costo del nodo sin el de sus hijos. En el plan el costo de un nodo ya incluye el de lo que
 * cuelga de él, así que sin restar, la raíz sería siempre la más cara. Puede dar negativo cuando el
 * planificador subestima un padre (un `Limit` sobre un hijo que no recorre entero): se corta en cero.
 */
export function selfCost(node: PlanNode): number {
  const children = node.children.reduce((sum, child) => sum + child.totalCost, 0);
  return Math.max(0, node.totalCost - children);
}

/** Con qué se mide un plan: el tiempo real si lo hay en la raíz, y si no el costo estimado. */
function measureOf(root: PlanNode): "ms" | "cost" {
  return root.selfMs !== null ? "ms" : "cost";
}

export function weightOf(node: PlanNode, measure: "ms" | "cost"): number {
  return measure === "ms" ? (node.selfMs ?? 0) : selfCost(node);
}

export function planHeat(root: PlanNode): PlanHeat {
  const measure = measureOf(root);
  let worst = 0;
  let total = 0;
  const walk = (node: PlanNode) => {
    const weight = weightOf(node, measure);
    worst = Math.max(worst, weight);
    total += weight;
    node.children.forEach(walk);
  };
  walk(root);
  return { measure, worst, total };
}

/** `share` es el peso del nodo sobre el del más caro, de 0 a 1. */
export function heatLevel(share: number): HeatLevel {
  if (share >= 0.6) return 4;
  if (share >= 0.3) return 3;
  if (share >= 0.15) return 2;
  if (share >= 0.05) return 1;
  return 0;
}
