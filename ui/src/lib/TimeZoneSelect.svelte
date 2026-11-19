<script lang="ts">
  import { timeZone } from "./tz.svelte";
  import { ZONE_MODES } from "./tz";
  import { numberFormat } from "./numfmt.svelte";
</script>

<!-- Solo cambia cómo se ven los números; lo que se copia sigue siendo el valor original. -->
<button
  class="btn btn-ghost btn-icon btn-toggle text-[11px] font-semibold"
  aria-pressed={numberFormat.thousands}
  aria-label="Separador de miles"
  title="Muestra los números con separador de miles (1.234.567,89); no cambia lo que se copia"
  onclick={() => numberFormat.toggle()}
>
  1.0
</button>

<!-- Solo cambia cómo se ven los `timestamptz`; lo que se copia sigue siendo el valor original. -->
<label class="flex items-center gap-1 text-xs muted" title="Zona horaria con la que se muestran las fechas con zona (timestamptz)">
  <span>Zona</span>
  <select
    class="field py-0.5 text-xs"
    value={timeZone.mode}
    onchange={(event) => timeZone.set(event.currentTarget.value as "server" | "local" | "utc")}
  >
    {#each ZONE_MODES as mode (mode.value)}
      <option value={mode.value} title={mode.hint}>{mode.label}</option>
    {/each}
  </select>
</label>
