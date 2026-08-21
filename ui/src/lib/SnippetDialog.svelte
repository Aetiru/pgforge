<script lang="ts">
  import Alert from "./Alert.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import Empty from "./Empty.svelte";
  import { snippets } from "./snippets.svelte";
  import { preview } from "./sql-snippet";
  import type { Snippet } from "./ipc";

  /**
   * Las abreviaturas del editor.
   *
   * Se abre desde la barra de la pestaña de consulta y no desde una pantalla de preferencias
   * general: uno se acuerda de que quiere una abreviatura mientras escribe la consulta que la
   * pediría, igual que el umbral de los avisos se configura en la vista de procesos.
   *
   * Guardar valida del lado de Rust —la abreviatura única, sin espacios, con cuerpo— porque es el
   * que tiene la lista entera; acá solo se muestra lo que contestó. Duplicar esa validación en la
   * pantalla dejaría dos ideas de qué es válido.
   */
  let { onclose }: { onclose: () => void } = $props();

  /** La que se está editando, o `null` mientras se mira la lista. */
  let editing = $state<Snippet | null>(null);
  let busy = $state(false);
  let confirmingReset = $state(false);

  function nueva(): Snippet {
    // El identificador vacío hace que Rust le ponga uno: es una alta, no una reescritura.
    return { id: "", abbreviation: "", body: "", description: "" };
  }

  async function submit() {
    if (!editing) return;
    busy = true;
    const ok = await snippets.save(editing);
    busy = false;
    if (ok) editing = null;
  }

  async function remove(item: Snippet) {
    busy = true;
    await snippets.remove(item.id);
    busy = false;
  }

  async function reset() {
    confirmingReset = false;
    busy = true;
    await snippets.reset();
    busy = false;
  }
</script>

<Modal
  title="Abreviaturas"
  subtitle="Escribí la abreviatura en el editor y apretá Tab para expandirla."
  size="lg"
  {busy}
  {onclose}
>
  {#if snippets.error}
    <Alert tone="bad" box class="mb-3" onclose={() => (snippets.error = null)}>
      {snippets.error}
    </Alert>
  {/if}

  {#if editing}
    <div class="space-y-3">
      <div class="grid grid-cols-[160px_1fr] gap-3">
        <label class="field">
          <span class="label">Abreviatura</span>
          <input
            class="w-full"
            data-autofocus
            bind:value={editing.abbreviation}
            placeholder="sf"
            spellcheck="false"
          />
        </label>
        <label class="field">
          <span class="label">Para qué es</span>
          <input class="w-full" bind:value={editing.description} placeholder="Consulta base" />
        </label>
      </div>

      <label class="field">
        <span class="label">Texto</span>
        <textarea
          class="w-full select-text font-mono"
          rows="7"
          bind:value={editing.body}
          spellcheck="false"
          placeholder={"SELECT ${*}\nFROM ${tabla}\nWHERE ${}"}
        ></textarea>
      </label>

      <p class="text-xs muted">
        Lo que va entre <code>{"${}"}</code> es un hueco: al expandir queda seleccionado y con Tab se
        salta al siguiente. El nombre de adentro —<code>{"${tabla}"}</code>— es el texto con el que
        aparece. Un <code>$1</code> sin llaves es un parámetro de PostgreSQL y se inserta tal cual.
      </p>
    </div>
  {:else if snippets.items.length === 0}
    <Empty icon="sql" title="No hay abreviaturas" hint="Creá una, o volvé a las de fábrica." />
  {:else}
    <table class="list-table">
      <thead>
        <tr>
          <th class="w-28">Abreviatura</th>
          <th>Se expande a</th>
          <th class="w-20"></th>
        </tr>
      </thead>
      <tbody>
        {#each snippets.items as item (item.id)}
          <tr>
            <td class="font-mono">{item.abbreviation}</td>
            <td class="truncate">
              <span>{item.description || preview(item.body)}</span>
              {#if item.description}
                <span class="ml-2 text-xs muted">{preview(item.body)}</span>
              {/if}
            </td>
            <td>
              <div class="row-actions">
                <button
                  class="btn btn-icon btn-sm"
                  title="Editar «{item.abbreviation}»"
                  onclick={() => (editing = { ...item })}
                >
                  <Icon name="edit" />
                </button>
                <button
                  class="btn btn-icon btn-sm"
                  title="Borrar «{item.abbreviation}»"
                  onclick={() => remove(item)}
                >
                  <Icon name="trash" />
                </button>
              </div>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}

  {#snippet footer()}
    {#if editing}
      <button class="btn btn-ghost" onclick={() => (editing = null)} disabled={busy}>
        Cancelar
      </button>
      <button class="btn btn-primary" onclick={submit} disabled={busy}>Guardar</button>
    {:else if confirmingReset}
      <span class="mr-auto text-xs muted">Se descartan las tuyas y vuelven las de fábrica.</span>
      <button class="btn btn-ghost" onclick={() => (confirmingReset = false)}>Cancelar</button>
      <button class="btn btn-danger" onclick={reset} disabled={busy}>Restablecer</button>
    {:else}
      <button class="btn btn-ghost mr-auto" onclick={() => (confirmingReset = true)}>
        Restablecer
      </button>
      <button class="btn" onclick={() => (editing = nueva())}>Nueva</button>
      <button class="btn btn-primary" onclick={onclose}>Listo</button>
    {/if}
  {/snippet}
</Modal>
