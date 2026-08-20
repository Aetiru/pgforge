<script lang="ts">
  import Icon, { type IconName } from "./Icon.svelte";
  import Sql from "./Sql.svelte";
  import type { Advice, AdviceKind, IndexTarget } from "./ipc";

  /**
   * Lo que el plan deja ver, dicho en voz alta.
   *
   * Va arriba del árbol y no abajo porque es la respuesta: quien pide un plan quiere saber qué
   * anda mal, y el árbol es la prueba. Lo que muestra sale entero de `sql::advice` —acá no se
   * decide nada—, y la sentencia se copia: crear un índice cambia el costo de cada escritura de esa
   * tabla y esa decisión no la toma un cartel.
   */
  let {
    advice,
    analyzed,
    oncreateIndex,
  }: {
    advice: Advice[];
    analyzed: boolean;
    /** Abre el diálogo de índices con la tabla y las columnas de la sugerencia ya puestas. */
    oncreateIndex: (target: IndexTarget) => void;
  } = $props();

  const ICON: Record<AdviceKind, IconName> = {
    missingIndex: "index",
    indexFilter: "index",
    staleStats: "chart",
    workMem: "gauge",
  };

  let copied = $state<string | null>(null);

  async function copy(sql: string) {
    await navigator.clipboard.writeText(sql).catch(() => {});
    copied = sql;
    setTimeout(() => (copied = copied === sql ? null : copied), 1500);
  }
</script>

{#if advice.length > 0}
  <div class="flex flex-col gap-2">
    {#each advice as item, index (index)}
      <div class="card p-0">
        <div class="flex items-start gap-2 px-3 py-2">
          <Icon
            name={ICON[item.kind]}
            size={14}
            class="mt-0.5 shrink-0 {item.severity === 'warn'
              ? 'text-amber-600 dark:text-amber-400'
              : 'muted'}"
          />
          <div class="min-w-0 flex-1">
            <p class="text-sm font-medium select-text">{item.title}</p>
            <p class="mt-0.5 text-xs select-text muted">{item.detail}</p>
          </div>
        </div>

        {#if item.sql}
          <div class="divider-t flex items-start gap-2 px-3 py-2">
            <div class="min-w-0 flex-1 overflow-auto">
              <Sql code={item.sql} />
            </div>
            <div class="flex shrink-0 items-center gap-1.5">
              <button class="btn btn-sm" onclick={() => copy(item.sql ?? "")}>
                <Icon name={copied === item.sql ? "check" : "copy"} size={11} />
                {copied === item.sql ? "Copiado" : "Copiar"}
              </button>
              <!-- Copiar y pegar el mismo texto en otra ventana era el paso que sobraba: el diálogo
                   se abre con esta tabla y estas columnas, y de ahí sigue el camino de siempre
                   —vista previa, confirmación de producción, tarea en segundo plano—. -->
              {#if item.index}
                <button class="btn btn-primary btn-sm" onclick={() => oncreateIndex(item.index!)}>
                  Crear el índice…
                </button>
              {/if}
            </div>
          </div>
        {/if}
      </div>
    {/each}
  </div>
{:else if !analyzed}
  <!-- Sin medir no hay nada honesto que sugerir: cuántas filas descarta un filtro es justo lo que
       no se sabe hasta ejecutar. -->
  <p class="text-xs muted">
    «Explicar y medir» ejecuta la consulta y compara lo estimado con lo real; con eso se pueden
    señalar los recorridos completos que sobran y los índices que faltan.
  </p>
{/if}
