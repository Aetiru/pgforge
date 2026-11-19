<script lang="ts">
  import { untrack } from "svelte";
  import Empty from "./Empty.svelte";
  import Modal from "./Modal.svelte";
  import Sql from "./Sql.svelte";
  import { applyLiterals, findLiterals, type LiteralKind } from "./snippet-literals";

  /**
   * Convierte los valores fijos de una consulta en variables antes de guardarla como abreviatura:
   * `'2026-01-01'` pasa a `${desde:2026-01-01}` y 100 a `${limite:100}`. El nombre sale del contexto
   * (la columna con la que se compara) y se puede cambiar; lo que se escribió queda como valor por
   * omisión.
   */
  let {
    body,
    onapply,
    onskip,
  }: {
    body: string;
    /** El cuerpo con los elegidos convertidos. */
    onapply: (body: string) => void;
    /** Seguir con el texto como está. */
    onskip: () => void;
  } = $props();

  // El diálogo se monta por cada cuerpo: los literales se leen una vez y las casillas son suyas.
  const found = untrack(() => findLiterals(body));

  const KIND: Record<LiteralKind, string> = {
    string: "texto",
    number: "número",
    date: "fecha",
    timestamp: "fecha y hora",
  };

  // Todos marcados salvo lo que casi nunca se quiere variable: los números sueltos son lo más
  // ruidoso de una consulta, y sacarlos con un clic es más fácil que marcar uno por uno.
  let rows = $state(
    found.map((literal) => ({
      literal,
      checked: literal.kind !== "number",
      name: literal.suggestedName,
    })),
  );

  const result = $derived(
    applyLiterals(
      body,
      rows
        .filter((row) => row.checked)
        .map((row) => ({
          from: row.literal.from,
          to: row.literal.to,
          name: row.name,
          default: row.literal.text,
        })),
    ),
  );

  const chosen = $derived(rows.filter((row) => row.checked).length);
</script>

<Modal
  title="Convertir valores en variables"
  subtitle="Al expandir la abreviatura se preguntará cada uno, con lo escrito como valor por omisión."
  size="lg"
  onclose={onskip}
>
  {#if rows.length === 0}
    <Empty icon="info" title="No hay valores fijos para convertir" />
  {:else}
    <table class="list-table">
      <thead>
        <tr>
          <th class="w-8"></th>
          <th>Valor</th>
          <th class="w-28">Tipo</th>
          <th class="w-48">Nombre de la variable</th>
        </tr>
      </thead>
      <tbody>
        {#each rows as row, index (index)}
          <tr>
            <td>
              <input
                type="checkbox"
                aria-label="Convertir {row.literal.text}"
                bind:checked={row.checked}
              />
            </td>
            <td class="font-mono">{row.literal.text}</td>
            <td class="text-xs muted">{KIND[row.literal.kind]}</td>
            <td>
              <input
                class="field w-full font-mono"
                aria-label="Nombre para {row.literal.text}"
                bind:value={row.name}
                disabled={!row.checked}
                spellcheck="false"
              />
            </td>
          </tr>
        {/each}
      </tbody>
    </table>

    <p class="label mt-3 mb-1">Así queda</p>
    <div class="max-h-48 overflow-auto rounded border border-zinc-200 p-2 dark:border-zinc-700">
      <Sql code={result} />
    </div>
  {/if}

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onskip}>Dejar como está</button>
    <button class="btn btn-primary" disabled={chosen === 0} onclick={() => onapply(result)}>
      Convertir {chosen === 1 ? "el elegido" : `los ${chosen} elegidos`}
    </button>
  {/snippet}
</Modal>
