<script lang="ts">
  import { untrack } from "svelte";
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

<div class="fixed inset-0 z-10 grid place-items-center bg-black/40 p-4">
  <div
    class="card flex max-h-[85vh] w-full max-w-2xl flex-col shadow-xl"
    role="dialog"
    aria-modal="true"
    aria-label={existing ? "Editar vista" : "Nueva vista"}
  >
    <h2 class="divider-b px-5 py-3 text-base font-medium">
      {#if existing}
        Editar {existing.name}
      {:else if materialized}
        Nueva vista materializada en {schema}
      {:else}
        Nueva vista en {schema}
      {/if}
    </h2>

    <div class="min-h-0 flex-1 overflow-auto px-5 py-4 text-sm">
      <div class="grid grid-cols-2 gap-3">
        <label class="flex flex-col gap-1">
          <span class="text-xs muted">Nombre</span>
          <input class="field" bind:value={name} disabled={existing !== null} />
        </label>

        <label class="flex flex-col gap-1">
          <span class="text-xs muted">Columnas (opcional, separadas por coma)</span>
          <input class="field" bind:value={columnsText} placeholder="Postgres las infiere solo" />
        </label>
      </div>

      <label class="mt-3 flex flex-col gap-1">
        <span class="text-xs muted">Consulta</span>
        {#if loadingQuery}
          <p class="rounded border border-zinc-200 px-2 py-4 text-center text-sm muted dark:border-zinc-800">
            Cargando…
          </p>
        {:else}
          <textarea
            class="field font-mono text-xs"
            rows="10"
            bind:value={query}
            placeholder="SELECT ..."
          ></textarea>
        {/if}
      </label>

      {#if materialized}
        <label class="check mt-3 text-xs">
          <input type="checkbox" bind:checked={withData} />
          Poblar de inmediato (WITH DATA)
        </label>
      {/if}

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
      <button class="btn btn-ghost text-xs" onclick={showPreview} disabled={saving || loadingQuery}>
        Ver SQL
      </button>
      <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
      <button class="btn btn-primary" onclick={submit} disabled={saving || loadingQuery}>
        {existing ? "Guardar" : "Crear"}
      </button>
    </div>
  </div>
</div>
