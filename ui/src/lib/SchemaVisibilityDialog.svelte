<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import Skeleton from "./Skeleton.svelte";
  import { describeError, schemaNames } from "./ipc";
  import { hiddenSchemas } from "./tree-view.svelte";

  /**
   * Elegir cuáles esquemas de una base se ven en el árbol. Lo tildado se ve; lo demás se pliega.
   * Lo que se guarda es lo **oculto**, no lo visible: un esquema que se cree después aparece solo.
   */
  let {
    profileId,
    database,
    onapply,
    onclose,
  }: {
    profileId: string;
    database: string;
    /** Se llama después de guardar, para que el árbol vuelva a leer la carpeta. */
    onapply: () => void;
    onclose: () => void;
  } = $props();

  let names = $state<string[] | null>(null);
  let error = $state<string | null>(null);
  let filter = $state("");
  // Lo ya oculto arranca destildado.
  let visible = $state<Record<string, boolean>>({});

  $effect(() => {
    const hidden = untrack(() => hiddenSchemas.get(profileId, database));
    schemaNames(profileId, database)
      .then((list) => {
        names = list;
        visible = Object.fromEntries(list.map((name) => [name, !hidden.includes(name)]));
      })
      .catch((problem) => (error = describeError(problem)));
  });

  const shown = $derived(
    (names ?? []).filter((name) => name.toLowerCase().includes(filter.trim().toLowerCase())),
  );
  const hiddenCount = $derived((names ?? []).filter((name) => !visible[name]).length);

  function setAll(on: boolean) {
    for (const name of shown) visible[name] = on;
  }

  function save() {
    hiddenSchemas.set(
      profileId,
      database,
      (names ?? []).filter((name) => !visible[name]),
    );
    onapply();
    onclose();
  }
</script>

<Modal title="Esquemas visibles" subtitle={database} size="md" {onclose}>
  {#if error}
    <Alert tone="bad" box>{error}</Alert>
  {:else if !names}
    <Skeleton rows={6} />
  {:else}
    <div class="flex items-center gap-2">
      <input class="field min-w-0 flex-1" placeholder="Filtrar…" bind:value={filter} spellcheck="false" />
      <button class="btn btn-sm" onclick={() => setAll(true)}>Ver todos</button>
      <button class="btn btn-sm" onclick={() => setAll(false)}>Ocultar todos</button>
    </div>
    <div class="mt-3 max-h-72 overflow-auto rounded-md border border-zinc-200 p-1 dark:border-zinc-700">
      {#each shown as name (name)}
        <label class="check rounded px-2 py-1 hover:bg-zinc-100 dark:hover:bg-zinc-700/60">
          <input type="checkbox" bind:checked={visible[name]} />
          <span class="font-mono text-xs">{name}</span>
        </label>
      {:else}
        <p class="p-2 text-xs muted">Ningún esquema coincide.</p>
      {/each}
    </div>
    <p class="mt-2 text-xs muted">
      {hiddenCount === 0 ? "Se ven todos." : `${hiddenCount} oculto${hiddenCount === 1 ? "" : "s"}.`}
      Ocultar solo cambia el árbol de esta computadora; no toca nada en la base.
    </p>
  {/if}

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>Cancelar</button>
    <button class="btn btn-primary" disabled={!names} onclick={save}>Guardar</button>
  {/snippet}
</Modal>
