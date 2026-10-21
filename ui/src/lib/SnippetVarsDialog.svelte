<script lang="ts">
  import { untrack } from "svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import type { SnippetVariable } from "./snippet-vars";

  let { title, description, variables, onsubmit, onclose }: {
    title: string;
    description: string;
    variables: SnippetVariable[];
    onsubmit: (values: Record<string, string>) => void;
    onclose: () => void;
  } = $props();

  // Copia editable tomada una sola vez, como en el resto de los formularios.
  let values = $state<Record<string, string>>(
    untrack(() => Object.fromEntries(variables.map((v) => [v.name, v.default]))),
  );

  function submit() {
    onsubmit($state.snapshot(values));
    onclose();
  }
</script>

<Modal {title} subtitle={description || undefined} size="sm" {onclose}>
  <form
    class="space-y-3"
    onsubmit={(event) => {
      event.preventDefault();
      submit();
    }}
  >
    {#each variables as variable, index (variable.name)}
      <label class="flex flex-col gap-1">
        <span class="label">{variable.name}</span>
        <input
          class="field font-mono"
          data-autofocus={index === 0 ? true : undefined}
          bind:value={values[variable.name]}
          spellcheck="false"
        />
      </label>
    {/each}
    <!-- Sin un botón de envío dentro del formulario, Enter no lo manda. -->
    <button type="submit" class="hidden" tabindex="-1" aria-hidden="true"></button>
  </form>

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>Cancelar</button>
    <button class="btn btn-primary" onclick={submit}>
      <Icon name="check" size={12} />
      Insertar
    </button>
  {/snippet}
</Modal>
