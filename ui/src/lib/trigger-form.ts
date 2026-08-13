/**
 * Lógica pura del formulario de disparadores.
 *
 * Lo que hay que tener a la vista es que **editar es borrar y crear**: PostgreSQL no tiene un
 * `ALTER TRIGGER` que cambie el momento, los eventos ni la función, así que el cambio son dos
 * sentencias en la misma transacción y el `DROP` tiene que nombrar al trigger **como se llamaba**,
 * no como quedó. Eso es exactamente lo que se rompe callado si se arma en el componente.
 */

import type { Timing, TriggerChange, TriggerEvent, TriggerInfo, TriggerLevel } from "./ipc";

export const TIMING_OPTIONS: { value: Timing; label: string }[] = [
  { value: "before", label: "BEFORE" },
  { value: "after", label: "AFTER" },
  { value: "insteadOf", label: "INSTEAD OF" },
];

export const EVENT_OPTIONS: { value: TriggerEvent; label: string }[] = [
  { value: "insert", label: "INSERT" },
  { value: "update", label: "UPDATE" },
  { value: "delete", label: "DELETE" },
  { value: "truncate", label: "TRUNCATE" },
];

export const LEVEL_OPTIONS: { value: TriggerLevel; label: string }[] = [
  { value: "row", label: "ROW" },
  { value: "statement", label: "STATEMENT" },
];

export interface TriggerForm {
  name: string;
  timing: Timing;
  events: TriggerEvent[];
  level: TriggerLevel;
  /** Condición `WHEN`, cruda: la valida el servidor al ejecutar. */
  when: string;
  functionSchema: string;
  functionName: string;
}

/** La copia editable inicial. El esquema de la tabla es el default para el de la función. */
export function triggerForm(existing: TriggerInfo | null, schema: string): TriggerForm {
  return {
    name: existing?.name ?? "",
    timing: existing?.timing ?? "before",
    events: existing?.events ?? ["insert"],
    level: existing?.level ?? "row",
    when: existing?.when ?? "",
    functionSchema: existing?.functionSchema ?? schema,
    functionName: existing?.functionName ?? "",
  };
}

export function validateTrigger(form: TriggerForm): string | null {
  if (!form.name.trim()) return "Poné un nombre para el trigger.";
  if (form.events.length === 0) return "Elegí al menos un evento.";
  if (!form.functionSchema.trim() || !form.functionName.trim()) {
    return "Poné la función que va a ejecutar.";
  }
  return null;
}

export function toggleEvent(events: TriggerEvent[], event: TriggerEvent): TriggerEvent[] {
  return events.includes(event) ? events.filter((item) => item !== event) : [...events, event];
}

export function triggerChanges(
  form: TriggerForm,
  target: { schema: string; table: string },
  existing: TriggerInfo | null,
): TriggerChange[] {
  const { schema, table } = target;
  const create: TriggerChange = {
    kind: "createTrigger",
    schema,
    table,
    name: form.name.trim(),
    definition: {
      timing: form.timing,
      events: form.events,
      level: form.level,
      when: form.when.trim() || null,
      functionSchema: form.functionSchema.trim(),
      functionName: form.functionName.trim(),
    },
  };
  if (!existing) return [create];

  // El borrado nombra al trigger como estaba: si se le cambió el nombre, el nuevo todavía no existe.
  return [{ kind: "dropTrigger", schema, table, name: existing.name, cascade: false }, create];
}
