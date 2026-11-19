<script lang="ts">
  import { untrack } from "svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import type { SnippetVariable } from "./snippet-vars";
  import { validateVariable } from "./snippet-vars-input";

  let {
    title,
    description,
    variables,
    onsubmit,
    onclose,
    submitLabel = "Insertar",
    tables = [],
    columns = [],
  }: {
    submitLabel?: string;
    title: string;
    description: string;
    variables: SnippetVariable[];
    onsubmit: (values: Record<string, string>) => void;
    onclose: () => void;
    /** Para las variables `@tabla`: nombres calificados del catálogo. Se puede escribir otro. */
    tables?: string[];
    /** Para las `@columna`: las de las tablas del `FROM` o, sin ellas, las de todo el catálogo. */
    columns?: string[];
  } = $props();

  // Copia editable tomada una sola vez, como en el resto de los formularios.
  let values = $state<Record<string, string>>(
    untrack(() => Object.fromEntries(variables.map((v) => [v.name, v.default]))),
  );
  let errors = $state<Record<string, string>>({});

  function submit() {
    const found: Record<string, string> = {};
    const out: Record<string, string> = {};
    for (const variable of variables) {
      const result = validateVariable(variable, values[variable.name] ?? "");
      if (result.error) found[variable.name] = result.error;
      // Sin `value` la variable queda fuera y rige su valor por omisión: un campo de fecha o de
      // número vaciado a propósito no tiene que insertar una cadena vacía.
      else if (result.value !== null) out[variable.name] = result.value;
    }
    errors = found;
    if (Object.keys(found).length > 0) return;
    onsubmit(out);
    onclose();
  }

  const listId = (index: number) => `snippet-var-list-${index}`;
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
      {@const error = errors[variable.name]}
      <label class="flex flex-col gap-1">
        <span class="label">
          {variable.name}
          {#if variable.kind !== "text"}
            <span class="font-normal muted">
              · {variable.kind === "list"
                ? "elige una opción"
                : variable.kind === "date"
                  ? "fecha"
                  : variable.kind === "number"
                    ? "número"
                    : variable.kind === "table"
                      ? "tabla del catálogo"
                      : "columna del catálogo"}
            </span>
          {/if}
        </span>

        {#if variable.kind === "list"}
          <select
            class="field font-mono"
            data-autofocus={index === 0 ? true : undefined}
            bind:value={values[variable.name]}
            aria-invalid={error ? true : undefined}
          >
            {#each variable.options ?? [] as option (option)}
              <option value={option}>{option}</option>
            {/each}
          </select>
        {:else if variable.kind === "date"}
          <input
            class="field font-mono"
            type="date"
            data-autofocus={index === 0 ? true : undefined}
            bind:value={values[variable.name]}
            aria-invalid={error ? true : undefined}
          />
        {:else if variable.kind === "number"}
          <input
            class="field font-mono"
            inputmode="decimal"
            data-autofocus={index === 0 ? true : undefined}
            bind:value={values[variable.name]}
            spellcheck="false"
            aria-invalid={error ? true : undefined}
          />
        {:else if variable.kind === "table" || variable.kind === "column"}
          <!-- Un `datalist` y no un `select`: busca mientras se escribe y deja escribir un nombre
               que el catálogo todavía no trajo (una tabla recién creada, una columna calculada). -->
          <input
            class="field font-mono"
            list={listId(index)}
            data-autofocus={index === 0 ? true : undefined}
            bind:value={values[variable.name]}
            spellcheck="false"
            autocomplete="off"
            aria-invalid={error ? true : undefined}
          />
          <datalist id={listId(index)}>
            {#each variable.kind === "table" ? tables : columns as option (option)}
              <option value={option}></option>
            {/each}
          </datalist>
        {:else}
          <input
            class="field font-mono"
            data-autofocus={index === 0 ? true : undefined}
            bind:value={values[variable.name]}
            spellcheck="false"
          />
        {/if}

        {#if error}
          <span class="text-xs text-rose-600 dark:text-rose-400" role="alert">{error}</span>
        {/if}
      </label>
    {/each}
    <!-- Sin un botón de envío dentro del formulario, Enter no lo manda. -->
    <button type="submit" class="hidden" tabindex="-1" aria-hidden="true"></button>
  </form>

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>Cancelar</button>
    <button class="btn btn-primary" onclick={submit}>
      <Icon name="check" size={12} />
      {submitLabel}
    </button>
  {/snippet}
</Modal>
