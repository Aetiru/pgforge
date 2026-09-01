<script lang="ts">
  /** Lo que no cabe en el encabezado: los datos de la conexión, o los del objeto. */
  import type { Property } from "../detail-node";
  import Card from "./Card.svelte";

  let { rows }: { rows: Property[] } = $props();
</script>

<Card title="Propiedades">
  <div class="table-scroll">
    <table class="list-table">
      <tbody>
        {#each rows as row (row.label)}
          <tr>
            <td class="w-32 align-top muted @md/detail:w-56">{row.label}</td>
            <td class={row.bad ? "text-amber-700 dark:text-amber-400" : ""}>{row.value}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</Card>
