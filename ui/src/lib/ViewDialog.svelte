<script lang="ts">
  import { untrack } from "svelte";
  import { confirmMutation } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlEditor from "./SqlEditor.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import { describeError, viewApply, viewPreview, viewQuery, type ViewChange } from "./ipc";

  let {
    profileId,
    database,
    schema,
    materialized,
    existing,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    schema: string;
    materialized: boolean;
    /** `null` da de alta; si no, edita la vista que llega acá (se resuelve la consulta sola). */
    existing: { oid: number; name: string } | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  let name = $state(untrack(() => existing?.name ?? ""));
  let columnsText = $state("");
  let query = $state("");
  let withData = $state(true);

  let loadingQuery = $state(untrack(() => existing !== null));
  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  $effect(() => {
    if (!existing) return;
    loadingQuery = true;
    viewQuery(profileId, existing.oid, database)
      .then((result) => (query = result))
      .catch((e) => (error = describeError(e)))
      .finally(() => (loadingQuery = false));
  });

  function columns(): string[] {
    return columnsText
      .split(",")
      .map((c) => c.trim())
      .filter((c) => c.length > 0);
  }

  function changes(): ViewChange[] {
    if (materialized) {
      const create: ViewChange = {
        kind: "createMaterializedView",
        schema,
        name: existing ? existing.name : name.trim(),
        columns: columns(),
        query,
        withData,
      };
      if (!existing) return [create];
      // Una vista materializada no admite reemplazo en el lugar: se borra y se crea de nuevo, en
      // la misma transacción.
      return [
        { kind: "dropMaterializedView", schema, name: existing.name, cascade: false },
        create,
      ];
    }

    return [
      {
        kind: "createView",
        schema,
        name: existing ? existing.name : name.trim(),
        columns: columns(),
        query,
        replace: existing !== null,
      },
    ];
  }

  function validate(): string | null {
    if (!existing && !name.trim()) return "Poné un nombre para la vista.";
    if (!query.trim()) return "Poné la consulta de la vista.";
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
      const statements = await viewPreview(changes());
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

    if (!(await confirmMutation(profileId, "Se va a modificar una vista."))) return;

    saving = true;
    try {
      await viewApply(profileId, changes(), database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={existing
    ? `Editar ${existing.name}`
    : materialized
      ? "Nueva vista materializada"
      : "Nueva vista"}
  subtitle={schema}
  size="lg"
  busy={saving}
  {onclose}
>
  <div class="grid grid-cols-2 gap-3">
    <label class="flex flex-col gap-1">
      <span class="label">Nombre</span>
      <input class="field" data-autofocus bind:value={name} disabled={existing !== null} />
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Columnas (opcional, separadas por coma)</span>
      <input class="field" bind:value={columnsText} placeholder="Postgres las infiere solo" />
    </label>
  </div>

  <div class="mt-3 flex flex-col gap-1">
    <span class="label">Consulta</span>
    {#if loadingQuery}
      <p
        class="flex items-center justify-center gap-2 rounded-md border border-zinc-200 px-2 py-8
               text-sm muted dark:border-zinc-800"
      >
        <span class="spinner"></span>
        Leyendo la definición…
      </p>
    {:else}
      <!-- El mismo editor que la pestaña de consultas: resaltado y numeración, no un textarea. -->
      <div class="h-64 overflow-hidden rounded-md border border-zinc-200 dark:border-zinc-800">
        <SqlEditor bind:value={query} />
      </div>
    {/if}
  </div>

  {#if materialized}
    <label class="check mt-3">
      <input type="checkbox" bind:checked={withData} />
      Poblar de inmediato (WITH DATA)
    </label>
  {/if}

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#if preview}
    <SqlPreview sql={preview} />
  {/if}

  {#snippet footer()}
    <button class="btn btn-ghost btn-sm" onclick={showPreview} disabled={saving || loadingQuery}>
      Ver SQL
    </button>
    <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
    <button class="btn btn-primary" onclick={submit} disabled={saving || loadingQuery}>
      {#if saving}<span class="spinner"></span>{/if}
      {existing ? "Guardar" : "Crear"}
    </button>
  {/snippet}
</Modal>
