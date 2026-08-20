/**
 * El plan, como texto para pegar en otro lado.
 *
 * Es una reconstrucción del árbol que ya está en pantalla y **no** la salida literal de
 * `EXPLAIN (FORMAT TEXT)`: el plan se pidió una sola vez y en JSON, y volver a pedirlo en texto
 * significaría, con `ANALYZE`, ejecutar la consulta otra vez. Para el uso de todos los días —pegarlo
 * en un ticket, mandarlo por chat— alcanza y sobra; el que necesite el original exacto tiene el JSON
 * al lado, que sí viene tal cual lo mandó el servidor.
 *
 * Se sigue la forma de `psql` porque es la que todo el mundo sabe leer: cada hijo cuelga con `->` y
 * lo que describe al nodo va indentado debajo.
 */

import type { Plan, PlanNode } from "./ipc";

/** Dos espacios por nivel, como `psql`. */
const STEP = 2;

function decimal(value: number, digits = 2): string {
  return value.toFixed(digits);
}

/** El encabezado del nodo: qué hace, sobre qué y a qué costo. */
function header(node: PlanNode): string {
  let title = node.nodeType;
  if (node.index && node.relation) title += ` using ${node.index} on ${node.relation}`;
  else if (node.relation) title += ` on ${node.relation}`;
  else if (node.index) title += ` using ${node.index}`;

  let line = `${title}  (cost=${decimal(node.startupCost)}..${decimal(node.totalCost)} rows=${Math.round(node.planRows)})`;

  if (node.actualRows !== null) {
    const time = node.totalMs !== null ? `time=${decimal(node.totalMs, 3)} ` : "";
    line += ` (actual ${time}rows=${Math.round(node.actualRows)} loops=${Math.round(node.loops ?? 1)})`;
  }
  return line;
}

/** Lo que va debajo del nodo: condiciones, filas descartadas, bloques, cómo ordenó. */
function details(node: PlanNode): string[] {
  const out: string[] = [];

  // La condición del índice y el filtro son cosas distintas y las dos importan: la primera dice qué
  // buscó, la segunda qué tuvo que descartar después.
  if (node.condition && node.condition !== node.filter) out.push(`Cond: ${node.condition}`);
  if (node.filter) out.push(`Filter: ${node.filter}`);
  if (node.rowsRemoved !== null && node.rowsRemoved > 0) {
    out.push(`Rows Removed by Filter: ${Math.round(node.rowsRemoved)}`);
  }
  if (node.sortMethod) {
    const space =
      node.sortSpaceKb !== null
        ? `  ${node.sortOnDisk ? "Disk" : "Memory"}: ${Math.round(node.sortSpaceKb)}kB`
        : "";
    out.push(`Sort Method: ${node.sortMethod}${space}`);
  }
  if (node.sharedHitBlocks !== null || node.sharedReadBlocks !== null) {
    const parts: string[] = [];
    if (node.sharedHitBlocks) parts.push(`hit=${Math.round(node.sharedHitBlocks)}`);
    if (node.sharedReadBlocks) parts.push(`read=${Math.round(node.sharedReadBlocks)}`);
    if (parts.length > 0) out.push(`Buffers: shared ${parts.join(" ")}`);
  }

  return out;
}

function lines(node: PlanNode, depth: number, out: string[]): void {
  // La raíz arranca pegada al margen; los hijos cuelgan con la flecha, que es lo que hace legible un
  // plan de cuarenta nodos sin dibujar nada.
  const prefix = depth === 0 ? "" : `${" ".repeat((depth - 1) * STEP + STEP)}->  `;
  out.push(prefix + header(node));

  const indent = " ".repeat(depth === 0 ? STEP : (depth - 1) * STEP + STEP + 6);
  for (const detail of details(node)) out.push(indent + detail);

  for (const child of node.children) lines(child, depth + 1, out);
}

/** El plan entero como texto. */
export function planText(plan: Plan): string {
  const out: string[] = [];
  lines(plan.root, 0, out);

  if (plan.planningMs !== null) out.push(`Planning Time: ${decimal(plan.planningMs, 3)} ms`);
  if (plan.executionMs !== null) out.push(`Execution Time: ${decimal(plan.executionMs, 3)} ms`);
  if (!plan.analyzed) out.push("(plan estimado, sin ejecutar)");

  return out.join("\n");
}
