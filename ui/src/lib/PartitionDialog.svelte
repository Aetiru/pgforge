<script lang="ts">
  import { confirmMutation } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    describeError,
    partitionApply,
    partitionPreview,
    type PartitionBound,
    type PartitionChange,
  } from "./ipc";

  let {
    profileId,
    database,
    schema,
    parent,
    strategy,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    schema: string;
    parent: string;
    /** La estrategia tal como la escribe el servidor: `RANGE (dia)`, `LIST (region)`, … */
    strategy: string;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  /** Crear una tabla nueva o enganchar una que ya existe: la cláusula del límite es la misma. */
  let attach = $state(false);
  let name = $state("");
  let from = $state("");
  let to = $state("");
  let values = $state("");
  let modulus = $state("4");
  let remainder = $state("0");
  let isDefault = $state(false);

  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  /** Qué límite pide la tabla madre. Sale de la estrategia, no se elige. */
  const kind = $derived(
    strategy.startsWith("LIST") ? "list" : strategy.startsWith("HASH") ? "hash" : "range",
  );

  function list(text: string): string[] {
    return text
      .split(",")
      .map((value) => value.trim())
      .filter((value) => value !== "");
  }

  function bound(): PartitionBound {
    if (isDefault) return { kind: "default" };
    if (kind === "list") return { kind: "list", values: list(values) };
    if (kind === "hash") {
      return { kind: "hash", modulus: Number(modulus), remainder: Number(remainder) };
    }
    return { kind: "range", from: list(from), to: list(to) };
  }

  function changes(): PartitionChange[] {
    const target = name.trim();
    return attach
      ? [
          {
            kind: "attachPartition",
            parentSchema: schema,
            parent,
            schema,
            name: target,
            bound: bound(),
          },
        ]
      : [
          {
            kind: "createPartition",
            parentSchema: schema,
            parent,
            schema,
            name: target,
            bound: bound(),
            partitionBy: null,
          },
        ];
  }

  function validate(): string | null {
    if (!name.trim()) return "Poné un nombre para la partición.";
    if (isDefault) return null;

    if (kind === "range") {
      if (list(from).length === 0) return "Poné el extremo inicial del rango.";
      if (list(to).length === 0) return "Poné el extremo final del rango.";
    } else if (kind === "list") {
      if (list(values).length === 0) return "Poné al menos un valor.";
    } else {
      const m = Number(modulus);
      const r = Number(remainder);
      if (!Number.isInteger(m) || m < 1) return "El módulo tiene que ser un entero de al menos 1.";
      if (!Number.isInteger(r) || r < 0 || r >= m) {
        return "El resto tiene que ser un entero menor que el módulo.";
      }
    }
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
      const statements = await partitionPreview(profileId, changes());
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

    if (!(await confirmMutation(profileId, "Se va a modificar una tabla particionada."))) return;

    saving = true;
    try {
      await partitionApply(profileId, changes(), database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal title="Nueva partición" subtitle="{schema}.{parent} · {strategy}" size="lg" busy={saving} {onclose}>
  <div class="seg" role="tablist">
    <button role="tab" aria-selected={!attach} onclick={() => (attach = false)}>
      Crear tabla nueva
    </button>
    <button role="tab" aria-selected={attach} onclick={() => (attach = true)}>
      Enganchar una existente
    </button>
  </div>

  <label class="mt-3 flex flex-col gap-1">
    <span class="label">Nombre de la partición</span>
    <input class="field" data-autofocus bind:value={name} />
  </label>

  <label class="check mt-3">
    <input type="checkbox" bind:checked={isDefault} />
    Partición por omisión: se lleva todo lo que no entra en ninguna otra
  </label>

  {#if !isDefault}
    {#if kind === "range"}
      <div class="mt-3 grid grid-cols-2 gap-3">
        <label class="flex flex-col gap-1">
          <span class="label">Desde (incluido)</span>
          <input class="field" bind:value={from} placeholder="'2024-01-01'" />
        </label>
        <label class="flex flex-col gap-1">
          <span class="label">Hasta (excluido)</span>
          <input class="field" bind:value={to} placeholder="'2025-01-01'" />
        </label>
      </div>
      <p class="mt-1 text-xs muted">
        Con varias columnas de partición, separá los extremos con coma. Se admiten
        <code>MINVALUE</code> y <code>MAXVALUE</code>.
      </p>
    {:else if kind === "list"}
      <label class="mt-3 flex flex-col gap-1">
        <span class="label">Valores (separados por coma)</span>
        <input class="field" bind:value={values} placeholder="'sur', 'patagonia'" />
      </label>
    {:else}
      <div class="mt-3 grid grid-cols-2 gap-3">
        <label class="flex flex-col gap-1">
          <span class="label">Módulo</span>
          <input class="field" bind:value={modulus} />
        </label>
        <label class="flex flex-col gap-1">
          <span class="label">Resto</span>
          <input class="field" bind:value={remainder} />
        </label>
      </div>
    {/if}
  {/if}

  {#if attach}
    <Alert tone="warn" box class="mt-3">
      El servidor revisa que ninguna fila de la tabla se salga del límite antes de engancharla, y la
      bloquea mientras lo hace.
    </Alert>
  {/if}

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#if preview}
    <SqlPreview sql={preview} />
  {/if}

  {#snippet footer()}
    <button class="btn btn-ghost btn-sm" onclick={showPreview} disabled={saving}>Ver SQL</button>
    <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
    <button class="btn btn-primary" onclick={submit} disabled={saving}>
      {#if saving}<span class="spinner"></span>{/if}
      {attach ? "Enganchar" : "Crear"}
    </button>
  {/snippet}
</Modal>
