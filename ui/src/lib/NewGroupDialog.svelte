<script lang="ts">
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { explorer } from "./explorer.svelte";
  import { normalizeGroup } from "./folders.svelte";

  let { onclose }: { onclose: () => void } = $props();

  let value = $state("");
  let error = $state<string | null>(null);

  const name = $derived(normalizeGroup(value));

  function validate(): string | null {
    if (!name) return "Poné un nombre para la carpeta.";
    if (explorer.groups.includes(name) || explorer.pendingGroups.includes(name)) {
      return `Ya existe una carpeta «${name}».`;
    }
    return null;
  }

  function submit() {
    error = validate();
    if (error) return;
    explorer.newGroup(name);
    onclose();
  }
</script>

<Modal title="Nueva carpeta" size="sm" {onclose}>
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
    La carpeta agrupa conexiones guardadas: no toca nada en los servidores. Arrastrá servidores del
    árbol para meterlos. Una barra anida —«Clientes/ACME» se dibuja adentro de «Clientes»—. Una
    carpeta que quede vacía no se guarda: desaparece al cerrar la aplicación.
  </p>

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>Cancelar</button>
    <button class="btn btn-primary" onclick={submit}>Crear</button>
  {/snippet}
</Modal>
