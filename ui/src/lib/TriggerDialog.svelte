<script lang="ts">
  import { untrack } from "svelte";
  import {
    describeError,
    triggerApply,
    triggerPreview,
    type Timing,
    type TriggerChange,
    type TriggerEvent,
    type TriggerInfo,
    type TriggerLevel,
  } from "./ipc";

  let {
    profileId,
    database,
    schema,
    table,
    existing,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    schema: string;
    table: string;
    /** `null` da de alta; si no, reemplaza el trigger que llega acá (borra y crea de nuevo). */
    existing: TriggerInfo | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  const TIMING_OPTIONS: { value: Timing; label: string }[] = [
    { value: "before", label: "BEFORE" },
    { value: "after", label: "AFTER" },
    { value: "insteadOf", label: "INSTEAD OF" },
  ];

  const EVENT_OPTIONS: { value: TriggerEvent; label: string }[] = [
    { value: "insert", label: "INSERT" },
    { value: "update", label: "UPDATE" },
    { value: "delete", label: "DELETE" },
    { value: "truncate", label: "TRUNCATE" },
  ];

  const LEVEL_OPTIONS: { value: TriggerLevel; label: string }[] = [
    { value: "row", label: "ROW" },
    { value: "statement", label: "STATEMENT" },
  ];

  // Copia editable, tomada una sola vez: el diálogo se crea de nuevo cada vez que se abre.
  let name = $state(untrack(() => existing?.name ?? ""));
  let timing = $state<Timing>(untrack(() => existing?.timing ?? "before"));
  let events = $state<TriggerEvent[]>(untrack(() => existing?.events ?? ["insert"]));
  let level = $state<TriggerLevel>(untrack(() => existing?.level ?? "row"));
  let whenText = $state(untrack(() => existing?.when ?? ""));
  let functionSchema = $state(untrack(() => existing?.functionSchema ?? schema));
  let functionName = $state(untrack(() => existing?.functionName ?? ""));

  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  function toggleEvent(event: TriggerEvent) {
    events = events.includes(event) ? events.filter((e) => e !== event) : [...events, event];
  }

  function changes(): TriggerChange[] {
    const definition = {
      timing,
      events,
      level,
      when: whenText.trim() || null,
      functionSchema: functionSchema.trim(),
      functionName: functionName.trim(),
    };
    const create: TriggerChange = { kind: "createTrigger", schema, table, name: name.trim(), definition };
    if (!existing) return [create];
    return [{ kind: "dropTrigger", schema, table, name: existing.name, cascade: false }, create];
  }

  function validate(): string | null {
    if (!name.trim()) return "Poné un nombre para el trigger.";
    if (events.length === 0) return "Elegí al menos un evento.";
    if (!functionSchema.trim() || !functionName.trim()) return "Poné la función que va a ejecutar.";
    return null;
  }

  async function showPreview() {
    error = null;
    const problem = validate();
    if (problem) {
      error = problem;
      return;
    }
    try {
      const statements = await triggerPreview(changes());
      preview = statements.map((statement) => statement.sql).join(";\n\n");
    } catch (e) {
      error = describeError(e);
    }
  }

  async function submit() {
    error = null;
    const problem = validate();
    if (problem) {
      error = problem;
      return;
    }

    saving = true;
    try {
      await triggerApply(profileId, changes(), database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<div class="fixed inset-0 z-10 grid place-items-center bg-black/40 p-4">
  <div
    class="card flex max-h-[85vh] w-full max-w-lg flex-col shadow-xl"
    role="dialog"
    aria-modal="true"
    aria-label={existing ? "Editar trigger" : "Nuevo trigger"}
  >
    <h2 class="divider-b px-5 py-3 text-base font-medium">
      {existing ? `Editar ${existing.name}` : `Nuevo trigger en ${table}`}
    </h2>

    <div class="min-h-0 flex-1 overflow-auto px-5 py-4 text-sm">
      <div class="grid grid-cols-2 gap-3">
        <label class="flex flex-col gap-1">
          <span class="text-xs muted">Nombre</span>
          <input class="field" bind:value={name} />
        </label>

        <label class="flex flex-col gap-1">
          <span class="text-xs muted">Momento</span>
          <select class="field" bind:value={timing}>
            {#each TIMING_OPTIONS as option (option.value)}
              <option value={option.value}>{option.label}</option>
            {/each}
          </select>
        </label>
      </div>

      <div class="mt-3">
        <span class="text-xs muted">Eventos</span>
        <div class="mt-1 flex flex-wrap gap-3">
          {#each EVENT_OPTIONS as option (option.value)}
            <label class="check text-xs">
              <input
                type="checkbox"
                checked={events.includes(option.value)}
                onchange={() => toggleEvent(option.value)}
              />
              {option.label}
            </label>
          {/each}
        </div>
      </div>

      <div class="mt-3 grid grid-cols-2 gap-3">
        <label class="flex flex-col gap-1">
          <span class="text-xs muted">Nivel</span>
          <select class="field" bind:value={level}>
            {#each LEVEL_OPTIONS as option (option.value)}
              <option value={option.value}>{option.label}</option>
            {/each}
          </select>
        </label>
      </div>

      <label class="mt-3 flex flex-col gap-1">
        <span class="text-xs muted">WHEN (opcional)</span>
        <input class="field" bind:value={whenText} placeholder="condición SQL, p. ej. NEW.activo" />
      </label>

      <div class="mt-3 grid grid-cols-2 gap-3">
        <label class="flex flex-col gap-1">
          <span class="text-xs muted">Esquema de la función</span>
          <input class="field" bind:value={functionSchema} />
        </label>
        <label class="flex flex-col gap-1">
          <span class="text-xs muted">Función a ejecutar</span>
          <input class="field" bind:value={functionName} />
        </label>
      </div>

      {#if error}
        <p class="mt-3 text-sm text-rose-600 dark:text-rose-400">{error}</p>
      {/if}

      {#if preview}
        <pre
          class="mt-3 max-h-40 overflow-auto rounded bg-zinc-100 p-2 font-mono text-xs
                 whitespace-pre-wrap select-text dark:bg-zinc-800">{preview}</pre>
      {/if}
    </div>

    <div class="divider-t flex items-center gap-2 px-5 py-3">
      <button class="btn btn-ghost text-xs" onclick={showPreview} disabled={saving}>
        Ver SQL
      </button>
      <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
      <button class="btn btn-primary" onclick={submit} disabled={saving}>
        {existing ? "Guardar" : "Crear trigger"}
      </button>
    </div>
  </div>
</div>
