<script lang="ts">
  import { untrack } from "svelte";
  import { confirmMutation } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    describeError,
    sequenceApply,
    sequenceInfo,
    sequencePreview,
    type SequenceChange,
    type SequenceInfo,
    type SequenceOptions,
  } from "./ipc";

  let {
    profileId,
    database,
    schema,
    existing,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    schema: string;
    /** `null` da de alta; si no, edita la secuencia que llega acá. */
    existing: { oid: number; name: string } | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  const TYPES = ["smallint", "integer", "bigint"];

  let name = $state(untrack(() => existing?.name ?? ""));
  let dataType = $state("bigint");
  let increment = $state("1");
  let start = $state("1");
  let minValue = $state("");
  let maxValue = $state("");
  let cache = $state("1");
  let cycle = $state(false);
  /** Vacío no reinicia; con un número, mueve la secuencia ahora. */
  let restart = $state("");

  let info = $state<SequenceInfo | null>(null);
  let loading = $state(untrack(() => existing !== null));
  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  $effect(() => {
    if (!existing) return;
    loading = true;
    sequenceInfo(profileId, existing.oid, database)
      .then((result) => {
        info = result;
        dataType = result.dataType;
        increment = String(result.increment);
        start = String(result.start);
        minValue = String(result.minValue);
        maxValue = String(result.maxValue);
        cache = String(result.cache);
        cycle = result.cycle;
      })
      .catch((e) => (error = describeError(e)))
      .finally(() => (loading = false));
  });

  /** Un campo numérico vacío es «no lo toques»; con texto, tiene que ser un entero. */
  function num(text: string): number | null {
    const trimmed = text.trim();
    if (trimmed === "") return null;
    const value = Number(trimmed);
    return Number.isInteger(value) ? value : null;
  }

  function options(): SequenceOptions {
    return {
      dataType,
      increment: num(increment),
      minValue: num(minValue),
      maxValue: num(maxValue),
      start: num(start),
      cache: num(cache),
      cycle,
    };
  }

  function changes(): SequenceChange[] {
    const target = existing ? existing.name : name.trim();

    if (!existing) {
      return [
        { kind: "createSequence", schema, name: target, ifNotExists: false, options: options() },
      ];
    }

    const list: SequenceChange[] = [
      { kind: "alterSequence", schema, name: target, options: options() },
    ];
    // El reinicio va aparte del resto: `START WITH` dice a dónde vuelve la secuencia el día que la
    // reinicien, y esto la mueve ahora. Confundirlos es el error clásico con secuencias.
    if (restart.trim() !== "") {
      list.push({ kind: "restartSequence", schema, name: target, value: num(restart) });
    }
    return list;
  }

  function validate(): string | null {
    if (!existing && !name.trim()) return "Poné un nombre para la secuencia.";
    for (const [label, text] of [
      ["El incremento", increment],
      ["El mínimo", minValue],
      ["El máximo", maxValue],
      ["El valor inicial", start],
      ["El caché", cache],
      ["El reinicio", restart],
    ] as const) {
      if (text.trim() !== "" && num(text) === null) return `${label} tiene que ser un entero.`;
    }
    if (num(increment) === 0) return "El incremento no puede ser cero.";
    const min = num(minValue);
    const max = num(maxValue);
    if (min !== null && max !== null && min > max) return "El mínimo es mayor que el máximo.";
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
      const statements = await sequencePreview(changes());
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

    if (!(await confirmMutation(profileId, "Se va a modificar una secuencia."))) return;

    saving = true;
    try {
      await sequenceApply(profileId, changes(), database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={existing ? `Editar ${existing.name}` : "Nueva secuencia"}
  subtitle={schema}
  size="lg"
  busy={saving}
  {onclose}
>
  {#if loading}
    <p class="flex items-center justify-center gap-2 py-8 text-sm muted">
      <span class="spinner"></span>
      Leyendo la definición…
    </p>
  {:else}
    <div class="grid grid-cols-2 gap-3">
      <label class="flex flex-col gap-1">
        <span class="label">Nombre</span>
        <input class="field" data-autofocus bind:value={name} disabled={existing !== null} />
      </label>

      <label class="flex flex-col gap-1">
        <span class="label">Tipo</span>
        <select class="field" bind:value={dataType}>
          {#each TYPES as type (type)}
            <option value={type}>{type}</option>
          {/each}
        </select>
      </label>
    </div>

    <div class="mt-3 grid grid-cols-3 gap-3">
      <label class="flex flex-col gap-1">
        <span class="label">Incremento</span>
        <input class="field" bind:value={increment} />
      </label>

      <label class="flex flex-col gap-1">
        <span class="label">Valor inicial</span>
        <input class="field" bind:value={start} />
      </label>

      <label class="flex flex-col gap-1">
        <span class="label">Caché</span>
        <input class="field" bind:value={cache} />
      </label>

      <label class="flex flex-col gap-1">
        <span class="label">Mínimo</span>
        <input class="field" bind:value={minValue} placeholder="lo elige el servidor" />
      </label>

      <label class="flex flex-col gap-1">
        <span class="label">Máximo</span>
        <input class="field" bind:value={maxValue} placeholder="lo elige el servidor" />
      </label>

      <label class="check self-end pb-2">
        <input type="checkbox" bind:checked={cycle} />
        Vuelve a empezar al llegar al tope
      </label>
    </div>

    {#if existing}
      <div class="divider-t mt-4 pt-3">
        <label class="flex flex-col gap-1">
          <span class="label">Reiniciar ahora</span>
          <input class="field" bind:value={restart} placeholder="dejalo vacío para no moverla" />
        </label>
        <p class="mt-1 text-xs muted">
          El valor inicial dice a dónde vuelve la secuencia cuando alguien la reinicie; esto la mueve
          ahora.
          {#if info}
            Valor actual: {info.lastValue ?? "sin usar todavía"}.
          {/if}
        </p>
        {#if info?.ownedBy}
          <p class="mt-2 text-xs muted">
            Pertenece a {info.ownedBy.schema}.{info.ownedBy.table}.{info.ownedBy.column}: al borrar
            esa columna se borra la secuencia con ella.
          </p>
        {/if}
      </div>
    {/if}
  {/if}

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#if preview}
    <SqlPreview sql={preview} />
  {/if}

  {#snippet footer()}
    <button class="btn btn-ghost btn-sm" onclick={showPreview} disabled={saving || loading}>
      Ver SQL
    </button>
    <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
    <button class="btn btn-primary" onclick={submit} disabled={saving || loading}>
      {#if saving}<span class="spinner"></span>{/if}
      {existing ? "Guardar" : "Crear"}
    </button>
  {/snippet}
</Modal>
