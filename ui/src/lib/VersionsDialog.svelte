<script lang="ts">
  import { onMount } from "svelte";
  import Alert from "./Alert.svelte";
  import Confirm from "./Confirm.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import Sql from "./Sql.svelte";
  import { count } from "./format";
  import { describeError, versionGet, versionList, type VersionMeta } from "./ipc";
  import { versions, type VersionableTab } from "./editor/versions.svelte";
  import { relativeTime } from "./editor/versions";

  let {
    tab,
    onrestore,
    onclose,
  }: {
    tab: VersionableTab;
    /** Recibe el texto elegido; quien llama lo pone en el editor como UN cambio (se puede deshacer). */
    onrestore: (text: string) => void;
    onclose: () => void;
  } = $props();

  let list = $state<VersionMeta[] | null>(null);
  let selected = $state<VersionMeta | null>(null);
  let text = $state<string | null>(null);
  let error = $state<string | null>(null);
  let confirmClear = $state(false);
  let copied = $state(false);

  const now = Math.floor(Date.now() / 1000);
  const key = $derived(versions.key(tab));

  async function load() {
    try {
      // Lo que hay escrito ahora también cuenta: si nunca se ejecutó ni pasaron 90 s, todavía no es
      // una copia, y abrir esta lista justo después de escribir mostraría una pestaña «sin historia».
      versions.snapshot(tab);
      list = await versionList(key);
      if (list.length > 0) await pick(list[0]);
    } catch (e) {
      error = describeError(e);
      list = [];
    }
  }

  async function pick(version: VersionMeta) {
    selected = version;
    text = null;
    try {
      const body = await versionGet(version.id);
      if (selected?.id === version.id) text = body;
    } catch (e) {
      error = describeError(e);
    }
  }

  onMount(() => {
    // Un guardado en vuelo (el del `snapshot`) tiene que llegar antes de leer la lista.
    setTimeout(load, 120);
  });

  function restore() {
    if (text === null) return;
    // Primero se guarda lo que está escrito: restaurar tira ese texto, y «deshacer» no alcanza si se
    // cerró la pestaña.
    versions.snapshot(tab);
    onrestore(text);
    onclose();
  }

  async function copy() {
    if (text === null) return;
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      error = "No se pudo copiar: abrí la versión y copiala a mano.";
    }
  }

  async function clearAll() {
    confirmClear = false;
    try {
      await versions.clear(tab);
      list = [];
      selected = null;
      text = null;
    } catch (e) {
      error = describeError(e);
    }
  }

  const stamp = (at: number) => new Date(at * 1000).toLocaleString("es");
</script>

<Modal
  title="Versiones anteriores de este texto"
  subtitle="Se guarda una copia al ejecutar, a los 90 segundos sin teclear y al cerrar la pestaña."
  size="2xl"
  {onclose}
>
  {#if error}<Alert tone="bad" box class="mb-2">{error}</Alert>{/if}

  {#if list === null}
    <p class="text-xs muted">Leyendo las copias…</p>
  {:else if list.length === 0}
    <p class="py-8 text-center text-sm muted">
      Todavía no hay copias de esta pestaña. Se van guardando solas mientras trabajás.
    </p>
  {:else}
    <div class="grid h-[26rem] gap-3 md:grid-cols-[17rem_minmax(0,1fr)]">
      <ul class="min-h-0 overflow-auto rounded-md border border-zinc-200 dark:border-zinc-700" role="listbox" aria-label="Copias guardadas">
        {#each list as version (version.id)}
          <li role="option" aria-selected={selected?.id === version.id}>
            <button
              class="flex w-full flex-col gap-0.5 border-b border-zinc-100 px-2.5 py-1.5 text-left text-xs last:border-b-0 hover:bg-zinc-50 dark:border-zinc-800 dark:hover:bg-zinc-800
                     {selected?.id === version.id ? 'bg-blue-50 dark:bg-blue-950/40' : ''}"
              title={stamp(version.at)}
              onclick={() => pick(version)}
            >
              <span class="flex items-baseline justify-between gap-2">
                <span class="font-semibold">{relativeTime(version.at, now)}</span>
                <span class="muted">{count(version.size)} car.</span>
              </span>
              <span class="truncate font-mono muted">{version.preview}</span>
            </button>
          </li>
        {/each}
      </ul>

      <div class="flex min-h-0 min-w-0 flex-col gap-2">
        <div class="flex items-center gap-2 text-xs muted">
          {#if selected}{stamp(selected.at)}{/if}
        </div>
        <div class="min-h-0 flex-1 overflow-auto rounded-md border border-zinc-200 p-2 select-text dark:border-zinc-700">
          {#if text !== null}<Sql code={text} />{:else}<span class="text-xs muted">Leyendo…</span>{/if}
        </div>
      </div>
    </div>
  {/if}

  {#snippet footer()}
    <button
      class="btn btn-danger-ghost"
      disabled={!list || list.length === 0}
      title="Borra todas las copias de esta pestaña"
      onclick={() => (confirmClear = true)}
    >
      <Icon name="trash" size={12} /> Borrar las copias
    </button>
    <button class="btn ml-auto" disabled={text === null} onclick={copy}>
      <Icon name="copy" size={12} /> {copied ? "Copiado" : "Copiar"}
    </button>
    <button
      class="btn btn-primary"
      disabled={text === null}
      title="Reemplaza el texto del editor por esta copia; Ctrl+Z lo deshace"
      onclick={restore}
    >
      <Icon name="undo" size={12} /> Restaurar
    </button>
  {/snippet}
</Modal>

{#if confirmClear}
  <Confirm
    title="Borrar las copias"
    message="Se borran todas las copias guardadas de esta pestaña. El texto que hay en el editor no se toca."
    confirmLabel="Borrar las copias"
    onconfirm={clearAll}
    onclose={() => (confirmClear = false)}
  />
{/if}
