<script lang="ts">
  import { untrack } from "svelte";
  import { confirmMutation } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { tasks } from "./tasks.svelte";
  import { describeError, maintenancePlan, type Operation, type Target } from "./ipc";

  let {
    profileId,
    target,
    database,
    onclose,
  }: {
    profileId: string;
    target: Target;
    /** Base sobre la que se ejecuta. Para una tabla o un índice es dónde vive el objeto; para la
     *  base entera es esa misma base. Sin ella, el comando cae en la base por defecto del servidor
     *  y la operación correría contra la base equivocada. */
    database: string | null;
    onclose: () => void;
  } = $props();

  type Kind = "vacuum" | "analyze" | "reindex";

  // Un índice solo admite REINDEX; VACUUM y ANALYZE no aplican y el núcleo los rechaza. El objetivo
  // no cambia mientras el diálogo está abierto (se recrea en cada apertura), así que se lee una vez.
  const isIndex = untrack(() => target.kind === "index");
  const kinds: Kind[] = isIndex ? ["reindex"] : ["vacuum", "analyze", "reindex"];

  let kind = $state<Kind>(isIndex ? "reindex" : "vacuum");
  let full = $state(false);
  let freeze = $state(false);
  let analyze = $state(false);
  let concurrently = $state(true);

  let sql = $state("");
  let warning = $state<string | null>(null);
  let planError = $state<string | null>(null);

  const operation = $derived.by<Operation>(() => {
    switch (kind) {
      case "vacuum":
        return { kind: "vacuum", full, freeze, analyze };
      case "analyze":
        return { kind: "analyze" };
      case "reindex":
        return { kind: "reindex", concurrently };
    }
  });

  const targetLabel = $derived(
    target.kind === "table"
      ? `${target.schema}.${target.name}`
      : target.kind === "index"
        ? `índice ${target.schema}.${target.name}`
        : `base ${target.name}`,
  );

  const ALL_KINDS: { value: Kind; label: string; hint: string }[] = [
    { value: "vacuum", label: "VACUUM", hint: "recupera el espacio de las filas muertas" },
    { value: "analyze", label: "ANALYZE", hint: "actualiza las estadísticas del planificador" },
    { value: "reindex", label: "REINDEX", hint: "reconstruye los índices" },
  ];
  const KINDS = ALL_KINDS.filter((item) => kinds.includes(item.value));

  // La sentencia exacta se pide al núcleo en vez de armarla acá: así lo que se muestra es
  // literalmente lo que se va a ejecutar, no una reconstrucción parecida.
  $effect(() => {
    const current = operation;
    let cancelled = false;
    maintenancePlan(profileId, current, target)
      .then((plan) => {
        if (cancelled) return;
        sql = plan.sql;
        warning = plan.warning;
        planError = null;
      })
      .catch((error) => {
        if (cancelled) return;
        planError = describeError(error);
        sql = "";
      });
    return () => {
      cancelled = true;
    };
  });

  /**
   * Larga la tarea y cierra: seguirla es cosa de la vista de procesos.
   *
   * Antes el diálogo se quedaba abierto mostrando el avance, y con él la aplicación entera: un
   * `VACUUM FULL` de media hora era media hora sin poder mirar otra tabla.
   */
  async function run() {
    if (!(await confirmMutation(profileId, "Se va a correr una tarea de mantenimiento."))) return;

    try {
      await tasks.maintenance({
        profileId,
        database: database ?? "",
        target: targetLabel,
        operation,
        on: target,
      });
    } catch (error) {
      planError = describeError(error);
      return;
    }
    onclose();
  }
</script>

<Modal title="Mantenimiento" subtitle={targetLabel} size="lg" {onclose}>
  <div class="seg" role="tablist">
    {#each KINDS as item (item.value)}
      <button
        class="seg-item"
        role="tab"
        aria-selected={kind === item.value}
        title={item.hint}
        onclick={() => (kind = item.value)}
      >
        {item.label}
      </button>
    {/each}
  </div>
  <p class="mt-1 text-xs muted">{KINDS.find((item) => item.value === kind)?.hint}</p>

  {#if kind === "vacuum"}
    <div class="mt-3 flex flex-wrap gap-4">
      <label class="check">
        <input type="checkbox" bind:checked={full} /> FULL
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={freeze} /> FREEZE
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={analyze} /> ANALYZE
      </label>
    </div>
  {:else if kind === "reindex"}
    <label class="check mt-3">
      <input type="checkbox" bind:checked={concurrently} />
      CONCURRENTLY (no bloquea las escrituras)
    </label>
  {/if}

  {#if warning}
    <Alert tone="warn" box class="mt-3">{warning}</Alert>
  {/if}

  {#if planError}
    <Alert tone="bad" box class="mt-3">{planError}</Alert>
  {:else}
    <pre
      class="mt-3 overflow-x-auto rounded-md border border-zinc-200 bg-zinc-50 px-3 py-2 font-mono
             text-xs select-text dark:border-zinc-800 dark:bg-zinc-800/50">{sql}</pre>
  {/if}

  <p class="mt-2 text-xs muted">
    Corre en segundo plano: se sigue y se cancela desde la vista de procesos.
  </p>

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>Cerrar</button>
    <button class="btn btn-primary" onclick={run} disabled={!sql}>Ejecutar</button>
  {/snippet}
</Modal>
