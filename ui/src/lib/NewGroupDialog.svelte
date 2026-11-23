<script lang="ts">
  import { t } from "./i18n.svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { explorer, groupStartsWith } from "./explorer.svelte";
  import { normalizeGroup } from "./folders.svelte";

  let { onclose }: { onclose: () => void } = $props();

  let value = $state("");
  let error = $state<string | null>(null);

  const name = $derived(normalizeGroup(value));

  /**
   * La carpeta que se va a crear no cuelga del `rootGroup` de esta ventana: se crea igual —no
   * bloquea, puede ser a propósito, por ejemplo para moverla a mano después—, pero sin este aviso
   * `scopedPending` (`explorer.svelte.ts`) la descarta al dibujar el árbol de esta ventana y el
   * diálogo se cierra sin que se vea nada, como si la creación hubiera fallado. Mismo criterio que
   * `ConnectionDialog` e `ImportServersDialog`.
   */
  const outOfWorkspaceScope = $derived.by(() => {
    const root = explorer.workspace?.rootGroup;
    return !!root && !!name && !groupStartsWith(name, root);
  });

  function validate(): string | null {
    if (!name) return t("Poné un nombre para la carpeta.");
    if (explorer.groups.includes(name) || explorer.pendingGroups.includes(name)) {
      return t("Ya existe una carpeta \"{name}\".", { name });
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

<Modal title={t("Nueva carpeta")} size="sm" {onclose}>
  <label class="flex flex-col gap-1">
    <span class="label">{t("Nombre")}</span>
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
    {t(
      "La carpeta agrupa conexiones guardadas: no toca nada en los servidores. Arrastrá servidores del árbol para meterlos. Una barra anida —\"Clientes/ACME\" se dibuja adentro de \"Clientes\"—. Una carpeta que quede vacía no se guarda: desaparece al cerrar la aplicación.",
    )}
  </p>

  {#if outOfWorkspaceScope}
    <Alert tone="warn" class="mt-2">
      {t("Esto no va a verse en esta ventana: queda fuera de \"{root}\".", {
        root: explorer.workspace?.rootGroup ?? "",
      })}
    </Alert>
  {/if}

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>{t("Cancelar")}</button>
    <button class="btn btn-primary" onclick={submit}>{t("Crear")}</button>
  {/snippet}
</Modal>
