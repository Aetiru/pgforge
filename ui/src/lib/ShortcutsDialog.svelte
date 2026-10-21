<script lang="ts">
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { SHORTCUTS, comboFromEvent, formatCombo } from "./shortcuts";
  import { MAC, shortcutStore } from "./shortcuts.svelte";

  /**
   * La chuleta de atajos y, a la vez, donde se reasignan. Una sola pantalla: la lista que se lee
   * es la misma que se edita, así no hay una chuleta que quede desactualizada respecto de lo que
   * de verdad hace cada tecla.
   */
  let { onclose }: { onclose: () => void } = $props();

  /** El atajo que espera la próxima combinación, o `null`. */
  let capturing = $state<string | null>(null);
  let error = $state<string | null>(null);

  const groups = $derived.by(() => {
    const out: { name: string; items: typeof SHORTCUTS }[] = [];
    for (const def of SHORTCUTS) {
      let group = out.find((item) => item.name === def.group);
      if (!group) out.push((group = { name: def.group, items: [] }));
      group.items.push(def);
    }
    return out;
  });

  const labelOf = (id: string) => SHORTCUTS.find((def) => def.id === id)?.label ?? id;

  // En fase de captura y con `stopImmediatePropagation`: mientras se espera una combinación, ni el
  // atajo que se está por pisar ni el `Escape` del diálogo deben enterarse de la tecla.
  $effect(() => {
    if (capturing === null) return;
    const id = capturing;

    const onKey = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      if (event.key === "Escape") {
        capturing = null;
        error = null;
        return;
      }
      const combo = comboFromEvent(event);
      if (combo === null) return; // solo un modificador: se espera la tecla
      const problem = shortcutStore.set(id, combo);
      if (problem) {
        error = problem;
        return;
      }
      error = null;
      capturing = null;
    };

    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });
</script>

<Modal
  title="Atajos de teclado"
  subtitle="Los que tienen «Cambiar» se pueden reasignar; el resto los atiende cada pantalla"
  size="lg"
  {onclose}
>
  {#if error}
    <Alert tone="bad" box class="mb-3" onclose={() => (error = null)}>{error}</Alert>
  {/if}

  <div class="max-h-[60vh] space-y-4 overflow-y-auto pr-1">
    {#each groups as group (group.name)}
      <section>
        <h3 class="label mb-1">{group.name}</h3>
        <table class="list-table">
          <tbody>
            {#each group.items as def (def.id)}
              {@const clash = shortcutStore.conflicts.get(def.id)}
              <tr>
                <td>
                  {def.label}
                  {#if clash}
                    <span
                      class="ml-1 text-xs text-amber-700 dark:text-amber-400"
                      title="La misma combinación la usa: {clash.map(labelOf).join(', ')}"
                    >
                      ⚠ se pisa con {clash.map(labelOf).join(', ')}
                    </span>
                  {/if}
                </td>
                <td class="w-44 text-right">
                  {#if capturing === def.id}
                    <span class="text-xs muted">Apretá la combinación… (Esc cancela)</span>
                  {:else}
                    <span class="kbd">{formatCombo(shortcutStore.combo(def.id), MAC)}</span>
                  {/if}
                </td>
                <td class="w-52">
                  {#if !def.fixed}
                    <div class="flex justify-end gap-1">
                      <button
                        class="btn btn-sm"
                        aria-pressed={capturing === def.id}
                        onclick={() => {
                          error = null;
                          capturing = capturing === def.id ? null : def.id;
                        }}
                      >
                        Cambiar
                      </button>
                      <button
                        class="btn btn-sm"
                        disabled={shortcutStore.combo(def.id) === null}
                        title="Deja el atajo sin asignar"
                        onclick={() => shortcutStore.set(def.id, null)}
                      >
                        Quitar
                      </button>
                      {#if shortcutStore.isCustom(def.id)}
                        <button class="btn btn-sm" onclick={() => shortcutStore.reset(def.id)}>
                          Restablecer
                        </button>
                      {/if}
                    </div>
                  {:else}
                    <span class="block text-right text-xs muted">fijo</span>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </section>
    {/each}
  </div>

  {#snippet footer()}
    <button class="btn btn-ghost mr-auto" onclick={() => shortcutStore.resetAll()}>
      Restablecer todos
    </button>
    <button class="btn btn-primary" onclick={onclose}>Listo</button>
  {/snippet}
</Modal>
