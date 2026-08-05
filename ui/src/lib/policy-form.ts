/**
 * Lógica pura del formulario de políticas de seguridad por fila.
 *
 * Fuera del componente por lo mismo que `role-form`: qué `PolicyChange` sale del formulario es lo
 * que se va a ejecutar. Acá lo silencioso es el par de expresiones — `USING` y `WITH CHECK` no valen
 * para todos los comandos, y mandar la que no corresponde crea una política que no filtra lo que el
 * usuario cree.
 */

import type { PolicyChange, PolicyCommand, PolicyInfo, PolicyKind } from "./ipc";

export interface PolicyForm {
  name: string;
  command: PolicyCommand;
  kind: PolicyKind;
  /** Como se escribe: nombres separados por coma. Vacío significa PUBLIC. */
  roles: string;
  using: string;
  check: string;
}

/** La copia editable inicial. El diálogo la toma una sola vez, con `untrack`. */
export function policyForm(existing: PolicyInfo | null): PolicyForm {
  return {
    name: existing?.name ?? "",
    command: existing?.command ?? "all",
    kind: existing?.kind ?? "permissive",
    roles: existing?.roles.join(", ") ?? "",
    using: existing?.using ?? "",
    check: existing?.check ?? "",
  };
}

/** `USING` dice qué filas se ven, y un `INSERT` no ve ninguna. */
export function acceptsUsing(command: PolicyCommand): boolean {
  return command !== "insert";
}

/** `WITH CHECK` dice qué filas se pueden escribir, y `SELECT`/`DELETE` no escriben. */
export function acceptsCheck(command: PolicyCommand): boolean {
  return command !== "select" && command !== "delete";
}

export function roleList(form: PolicyForm): string[] {
  return form.roles
    .split(",")
    .map((role) => role.trim())
    .filter((role) => role.length > 0);
}

/**
 * Los cambios pendientes. Con `existing` son dos: no hay «editar» una política, se borra y se crea
 * de nuevo.
 */
export function policyChanges(
  form: PolicyForm,
  target: { schema: string; table: string },
  existing: PolicyInfo | null,
): PolicyChange[] {
  const create: PolicyChange = {
    kind: "createPolicy",
    schema: target.schema,
    table: target.table,
    name: form.name.trim(),
    definition: {
      command: form.command,
      kind: form.kind,
      roles: roleList(form),
      using: acceptsUsing(form.command) ? form.using.trim() || null : null,
      check: acceptsCheck(form.command) ? form.check.trim() || null : null,
    },
  };
  if (!existing) return [create];
  return [
    { kind: "dropPolicy", schema: target.schema, table: target.table, name: existing.name },
    create,
  ];
}

export function validatePolicy(form: PolicyForm): string | null {
  if (!form.name.trim()) return "Poné un nombre para la política.";
  return null;
}
