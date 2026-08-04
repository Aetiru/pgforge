<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import { explorer } from "./explorer.svelte";
  import { describeError } from "./ipc";

  let {
    name,
    onclose,
  }: {
    /** La carpeta que se está editando. */
    name: string;
    onclose: () => void;
  } = $props();

  // Copia editable tomada una sola vez, como en el resto de los formularios.
  let value = $state(untrack(() => name));
  let error = $state<string | null>(null);
  let busy = $state(false);

  const servers = $derived(explorer.servers.filter((row) => row.group === name));

  function validate(): string | null {
    const trimmed = value.trim();
    if (!trimmed) return "Poné un nombre, o deshacé la carpeta si no la querés más.";
    if (trimmed !== name && explorer.groups.includes(trimmed)) {
      return `Ya existe una carpeta «${trimmed}».`;
    }
    return null;
  }

  async function run(to?: string) {
    error = null;
    busy = true;
    try {
      await explorer.renameGroup(name, to);
      onclose();
    } catch (e) {
      error = describeError(e);
    } finally {
      busy = false;
    }
  }

  async function submit() {
    error = validate();
    if (error) return;
    if (value.trim() === name) {
      onclose();
      return;
    }
    await run(value.trim());
  }
</script>

<Modal
  title="Carpeta «{name}»"
  subtitle="{servers.length} {servers.length === 1 ? 'servidor' : 'servidores'}"
  size="sm"
  {busy}
  {onclose}
>
  <label class="flex flex-col gap-1">
    <span class="label">Nombre</span>
    <input
      class="field"
      data-autofocus
      bind:value
      onkeydown={(event) => {
        if (event.key === "Enter") submit();
      }}
    />
  </label>

  <p class="mt-2 text-xs muted">
    La carpeta agrupa conexiones guardadas: no toca nada en los servidores. Se puede arrastrar un
    servidor del árbol para meterlo o sacarlo.
  </p>

  {#if servers.length > 0}
    <ul class="mt-3 flex flex-col gap-1">
      {#each servers as server (server.profileId)}
        <li class="flex items-center gap-1.5 text-sm">
          <span class="dot {server.connected ? 'dot-on' : 'dot-off'}"></span>
          <Icon name="server" size={12} class="muted" />
          <span class="truncate">{server.label}</span>
          <span class="truncate text-xs muted">{server.detail}</span>
        </li>
      {/each}
    </ul>
  {/if}

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#snippet footer()}
    <!-- Deshacer la carpeta no borra nada: los servidores quedan sueltos en el árbol. -->
    <button class="btn btn-danger-ghost" disabled={busy} onclick={() => run(undefined)}>
      <Icon name="trash" size={12} />
      Deshacer la carpeta
    </button>
    <button class="btn ml-auto" disabled={busy} onclick={onclose}>Cancelar</button>
    <button class="btn btn-primary" disabled={busy} onclick={submit}>
      {#if busy}<span class="spinner"></span>{/if}
      Guardar
    </button>
  {/snippet}
</Modal>
