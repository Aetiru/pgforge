<script lang="ts">
  import { t } from "./i18n.svelte";
  import { timeZone } from "./tz.svelte";
  import { ZONE_MODES } from "./tz";
  import { numberFormat } from "./numfmt.svelte";
</script>

<!-- Solo cambia cómo se ven los números; lo que se copia sigue siendo el valor original. -->
<button
  class="btn btn-ghost btn-icon btn-toggle text-[11px] font-semibold"
  aria-pressed={numberFormat.thousands}
  aria-label={t("Separador de miles")}
  title={t("Muestra los números con separador de miles (1.234.567,89); no cambia lo que se copia")}
  onclick={() => numberFormat.toggle()}
>
  1.0
</button>

<!-- Solo cambia cómo se ven los `timestamptz`; lo que se copia sigue siendo el valor original. -->
<label class="flex items-center gap-1 text-xs muted" title={t("Zona horaria con la que se muestran las fechas con zona (timestamptz)")}>
  <span>{t("Zona")}</span>
  <select
    class="field py-0.5 text-xs"
    value={timeZone.mode}
    onchange={(event) => timeZone.set(event.currentTarget.value as "server" | "local" | "utc")}
  >
    {#each ZONE_MODES as mode (mode.value)}
      <option value={mode.value} title={t(mode.hint)}>{t(mode.label)}</option>
    {/each}
  </select>
</label>
