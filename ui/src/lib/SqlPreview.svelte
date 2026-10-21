<script lang="ts">
  import Alert from "./Alert.svelte";
  import Icon from "./Icon.svelte";
  import { describeError, rehearsalRun, type RehearsalReport } from "./ipc";
  import RehearsalResult from "./RehearsalResult.svelte";
  import Sql from "./Sql.svelte";

  /**
   * El SQL que un diálogo va a ejecutar.
   *
   * Todos los formularios de DDL ofrecen verlo antes de aplicar: es la única forma de que el
   * usuario aprenda qué hace la herramienta en lugar de tener que confiar en ella.
   *
   * Con `profileId` ofrece además «Ensayar»: correr ese mismo SQL adentro de una transacción que se
   * revierte, para ver filas afectadas, errores y locks antes de aplicarlo de verdad.
   */
  let {
    sql,
    profileId = undefined,
    database = undefined,
  }: { sql: string; profileId?: string; database?: string } = $props();

  let report = $state<RehearsalReport | null>(null);
  let failure = $state<string | null>(null);
  let running = $state(false);

  // Un resultado de otro texto es de otra cosa: se descarta apenas cambia el SQL.
  $effect(() => {
    void sql;
    report = null;
    failure = null;
  });

  // Las vistas previas vacías dicen esto en lugar de dejar el recuadro en blanco: no hay nada que ensayar.
  const rehearsable = $derived(profileId !== undefined && sql.trim() !== "" && sql !== "Nada que aplicar.");

  async function rehearse() {
    if (!profileId) return;
    running = true;
    report = null;
    failure = null;
    try {
      report = await rehearsalRun(profileId, database, sql);
    } catch (error) {
      failure = describeError(error);
    } finally {
      running = false;
    }
  }
</script>

<div
  class="mt-3 max-h-48 overflow-auto rounded-md border border-zinc-200 bg-zinc-50 p-2
         dark:border-zinc-700 dark:bg-zinc-700/50"
>
  <Sql code={sql} />
</div>

{#if rehearsable}
  <div class="mt-2">
    <button
      class="btn btn-sm"
      disabled={running}
      title="Corre este SQL adentro de una transacción que se revierte: no deja nada aplicado"
      onclick={rehearse}
    >
      {#if running}<span class="spinner"></span>{:else}<Icon name="flask" size={14} />{/if}
      Ensayar
    </button>
  </div>
{/if}

{#if failure}
  <Alert class="mt-2" box tone="bad">{failure}</Alert>
{/if}

{#if report}
  <div class="mt-2 max-h-72 overflow-auto rounded-md border border-zinc-200 p-2 dark:border-zinc-700">
    <RehearsalResult {report} />
  </div>
{/if}
