<script lang="ts">
  import Empty from "./Empty.svelte";
  import Icon, { type IconName } from "./Icon.svelte";
  import Sql from "./Sql.svelte";
  import { notify, MIN_SECONDS_CHOICES } from "./notify.svelte";
  import { tasks, type TaskRun } from "./tasks.svelte";
  import { elapsedText, taskKindLabel, type TaskKind } from "./task-format";

  /**
   * Lo que está corriendo en segundo plano.
   *
   * Es la contracara de haber sacado los procesos largos de sus diálogos: si el `VACUUM` ya no vive
   * en una ventana modal, tiene que haber un lugar donde se lo vea. Acá no hay estado propio —todo
   * sale de `tasks`, que a su vez es espejo de lo que anota Rust— salvo el reloj: el proceso no
   * manda un evento por segundo solo para mover el número de al lado.
   *
   * El aviso al terminar se configura acá y no en una pantalla de preferencias aparte: es donde uno
   * está justo cuando se pregunta por qué no le avisaron, o por qué le avisan tanto.
   */

  const KIND_ICON: Record<TaskKind, IconName> = {
    maintenance: "gauge",
    index: "index",
    backup: "save",
    restore: "undo",
    export: "download",
    import: "upload",
  };

  let now = $state(Date.now());
  /** Cuál está desplegado. Uno solo: la salida de `pg_restore` son cientos de líneas. */
  let open = $state<string | null>(null);

  function secondsLabel(seconds: number): string {
    return seconds === 0 ? "de todo" : `de más de ${seconds} s`;
  }

  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  // Estar acá es estar viendo lo que termina: el aviso de la barra se apaga al entrar y también
  // con lo que vaya terminando mientras se mira. La escritura no se repite porque después queda en
  // cero, que es la condición de salida.
  $effect(() => {
    if (tasks.unseen > 0) tasks.seen();
  });

  function toggle(run: TaskRun) {
    open = open === run.taskId ? null : run.taskId;
  }
</script>

<div class="flex h-full flex-col">
  <div class="toolbar">
    <span class="text-xs font-medium">
      {tasks.running.length}
      {tasks.running.length === 1 ? "proceso en curso" : "procesos en curso"}
    </span>
    <label class="check ml-auto" title="Un aviso del sistema cuando termina algo que tardó">
      <input
        type="checkbox"
        checked={notify.enabled}
        onchange={(event) => notify.setEnabled(event.currentTarget.checked)}
      />
      Avisar al terminar
    </label>

    <select
      class="field py-0.5 text-xs"
      disabled={!notify.enabled}
      aria-label="A partir de cuánto avisar"
      title="Por debajo de esto no se avisa: el resultado aparece mientras todavía se está mirando"
      value={notify.minSeconds}
      onchange={(event) => notify.setMinSeconds(Number(event.currentTarget.value))}
    >
      {#each MIN_SECONDS_CHOICES as seconds (seconds)}
        <option value={seconds}>{secondsLabel(seconds)}</option>
      {/each}
    </select>

    <button
      class="btn btn-sm"
      disabled={tasks.finished.length === 0}
      onclick={() => tasks.clearFinished()}
    >
      Limpiar los terminados
    </button>
  </div>

  {#if tasks.all.length === 0}
    <Empty
      icon="clock"
      title="No hay procesos"
      hint="Acá aparecen el mantenimiento, la creación de índices, los backups y las copias de datos mientras corren. Se los puede dejar corriendo y seguir usando la aplicación."
    />
  {:else}
    <div class="min-h-0 flex-1 overflow-auto p-3">
      <div class="flex flex-col gap-2">
        {#each tasks.list as run (run.taskId)}
          <div class="card p-0">
            <div class="flex items-center gap-3 px-3 py-2">
              {#if run.status === "running"}
                <span class="spinner"></span>
              {:else}
                <Icon
                  name={run.status === "done" ? "check" : "warn"}
                  size={14}
                  class={run.status === "done"
                    ? "text-emerald-600 dark:text-emerald-400"
                    : "text-rose-600 dark:text-rose-400"}
                />
              {/if}

              <Icon name={KIND_ICON[run.kind]} size={13} class="muted" />

              <div class="min-w-0 flex-1">
                <p class="truncate text-sm font-medium">
                  {taskKindLabel(run.kind)}
                  <span class="font-normal muted">· {run.target}</span>
                </p>
                <p class="truncate text-xs muted">
                  {run.server}{run.database ? ` / ${run.database}` : ""}
                  {#if run.progress}· {run.progress}{/if}
                </p>
              </div>

              <span class="shrink-0 text-xs muted" title="Empezó a las {new Date(run.startedAt).toLocaleTimeString('es')}">
                {elapsedText(run.startedAt, run.finishedAt, now)}
              </span>

              {#if run.status === "running"}
                <button
                  class="btn btn-danger btn-sm shrink-0"
                  disabled={run.canceling}
                  title="Cancelar el proceso"
                  onclick={() => run.cancel()}
                >
                  {run.canceling ? "Cancelando…" : "Cancelar"}
                </button>
              {:else}
                <button
                  class="btn btn-ghost btn-icon shrink-0"
                  aria-label="Sacarlo de la lista"
                  title="Sacarlo de la lista"
                  onclick={() => tasks.remove(run)}
                >
                  <Icon name="close" size={11} />
                </button>
              {/if}

              <button
                class="btn btn-ghost btn-icon shrink-0"
                aria-label="Ver el detalle"
                aria-expanded={open === run.taskId}
                title="Ver el SQL y lo que fue informando"
                onclick={() => toggle(run)}
              >
                <Icon
                  name="chevron"
                  size={12}
                  class="transition-transform {open === run.taskId ? 'rotate-180' : ''}"
                />
              </button>
            </div>

            {#if run.outcome}
              <p
                class="divider-t px-3 py-1.5 text-xs select-text {run.status === 'failed'
                  ? 'text-rose-600 dark:text-rose-400'
                  : 'text-emerald-600 dark:text-emerald-400'}"
              >
                {run.outcome}
              </p>
            {/if}

            {#if open === run.taskId}
              <div class="divider-t px-3 py-2">
                {#if run.command}
                  <!-- El mantenimiento y el índice mandan SQL; el backup, una línea de comando. Los
                       dos se leen mejor con el resaltado que en un `<pre>` gris. -->
                  <Sql code={run.command} />
                {/if}
                {#if run.log.length > 0}
                  <div
                    class="mt-2 max-h-56 overflow-auto rounded-md border border-zinc-200 px-2 py-1.5
                           font-mono text-xs select-text dark:border-zinc-800"
                  >
                    {#each run.log as line, index (index)}
                      <div class="whitespace-pre-wrap">{line}</div>
                    {/each}
                  </div>
                {:else if !run.command}
                  <p class="text-xs muted">Todavía no informó nada.</p>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>
