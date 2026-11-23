<script lang="ts">
  import { t } from "./i18n.svelte";
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import Skeleton from "./Skeleton.svelte";
  import { dataOpen, dataPage, describeError, type DataPage, type TableLink } from "./ipc";
  import { linkLabel } from "./fk-nav";

  /**
   * Elegir el valor de una clave foránea de una lista, en vez de saber el número de memoria.
   *
   * Muestra las filas de la tabla de destino con una búsqueda que resuelve el servidor (la misma de
   * la pestaña de datos), y al elegir una devuelve los valores de **las columnas referenciadas** por
   * nombre: quien lo abrió los pone en las columnas propias, en el mismo orden de la clave.
   */
  let {
    profileId,
    database,
    link,
    current,
    onpick,
    onclose,
  }: {
    profileId: string;
    database: string;
    link: TableLink;
    /** Lo que vale hoy la clave, para decir qué se está cambiando. */
    current: (string | null)[];
    onpick: (values: Record<string, string | null>) => void;
    onclose: () => void;
  } = $props();

  const LIMIT = 50;

  let search = $state("");
  let page = $state<DataPage | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  // La forma se pide una vez; cada búsqueda nueva reemplaza a la anterior sin esperarla.
  let shapePromise = untrack(() => dataOpen(profileId, link.oid, database));
  let ticket = 0;

  async function load(term: string) {
    const mine = ++ticket;
    loading = true;
    error = null;
    try {
      const shape = await shapePromise;
      const result = await dataPage(profileId, shape, null, LIMIT, database, {
        order: null,
        filter: null,
        search: term.trim() || null,
      });
      if (mine !== ticket) return;
      page = result;
    } catch (problem) {
      if (mine === ticket) error = describeError(problem);
    } finally {
      if (mine === ticket) loading = false;
    }
  }

  // Una búsqueda por pausa al teclear, no una por letra.
  $effect(() => {
    const term = search;
    clearTimeout(timer);
    timer = setTimeout(() => void load(term), page === null ? 0 : 300);
    return () => clearTimeout(timer);
  });

  /** Las columnas que se muestran: las de la clave primero, después hasta cuatro más. */
  const shown = $derived.by(() => {
    if (!page) return [] as number[];
    const wanted = link.otherColumns.map((name) => page!.columns.indexOf(name));
    const rest = page.columns.map((_, index) => index).filter((index) => !wanted.includes(index));
    return [...wanted, ...rest.slice(0, 4)].filter((index) => index >= 0);
  });

  function pick(row: (string | null)[]) {
    const values: Record<string, string | null> = {};
    for (const name of link.otherColumns) {
      values[name] = row[page!.columns.indexOf(name)] ?? null;
    }
    onpick(values);
    onclose();
  }
</script>

<Modal
  title={t("Elegir de {label}", { label: linkLabel(link) })}
  subtitle={current.some((value) => value !== null)
    ? t("Ahora: {value}", { value: current.map((value) => value ?? "NULL").join(", ") })
    : t("Ahora: NULL")}
  size="xl"
  {onclose}
>
  <!-- svelte-ignore a11y_autofocus -->
  <input
    class="field w-full"
    placeholder={t("Buscar en {label}…", { label: linkLabel(link) })}
    autofocus
    bind:value={search}
    spellcheck="false"
  />

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {:else if page === null}
    <div class="mt-3"><Skeleton rows={6} /></div>
  {:else}
    <div class="mt-3 max-h-96 overflow-auto rounded-md border border-zinc-200 dark:border-zinc-700 {loading ? 'opacity-60' : ''}">
      <table class="list-table">
        <thead>
          <tr>
            {#each shown as index (index)}
              <th>{page.columns[index]}</th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each page.rows as row, position (position)}
            <tr class="cursor-pointer hover:bg-blue-50 dark:hover:bg-blue-950/30" onclick={() => pick(row)}>
              {#each shown as index (index)}
                <td class="font-mono text-xs {row[index] === null ? 'cell-null' : ''}">
                  {row[index] ?? "NULL"}
                </td>
              {/each}
            </tr>
          {:else}
            <tr><td class="muted" colspan={shown.length || 1}>{t("Ninguna fila coincide.")}</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
    <p class="mt-2 text-xs muted">
      {page.next !== null
        ? t("Se muestran las primeras {n}: afiná la búsqueda para ver otras.", { n: LIMIT })
        : page.rows.length === 1
          ? t("1 fila.")
          : t("{n} filas.", { n: page.rows.length })}
    </p>
  {/if}

  {#snippet footer()}
    <button class="btn" onclick={() => onpick(Object.fromEntries(link.otherColumns.map((name) => [name, null])))}>
      {t("Poner NULL")}
    </button>
    <button class="btn ml-auto" onclick={onclose}>{t("Cancelar")}</button>
  {/snippet}
</Modal>
