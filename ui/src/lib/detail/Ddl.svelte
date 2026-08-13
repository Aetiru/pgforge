<script lang="ts">
  /**
   * El DDL del objeto. El rótulo de la derecha dice de dónde salió: no es lo mismo lo que afirma el
   * servidor que lo que reconstruyó `pg_dump`.
   */
  import FontSize from "../FontSize.svelte";
  import Icon from "../Icon.svelte";
  import Sql from "../Sql.svelte";
  import type { Ddl } from "../ipc";
  import Card from "./Card.svelte";

  let {
    ddl,
    loading = false,
    error = null,
    copied = false,
    oncopy,
  }: {
    ddl: Ddl | null;
    loading?: boolean;
    error?: string | null;
    copied?: boolean;
    oncopy: () => void;
  } = $props();
</script>

<Card title="DDL" loading={loading ? "Generando DDL…" : null} {error}>
  {#snippet actions()}
    {#if ddl}
      <span class="text-xs muted">
        {ddl.source === "pgDump" ? "reconstruido con pg_dump" : "generado por PostgreSQL"}
      </span>
      <span class="ml-auto flex items-center gap-1">
        <FontSize />
        <button class="btn btn-sm" onclick={oncopy}>
          <Icon name={copied ? "check" : "copy"} size={11} />
          {copied ? "Copiado" : "Copiar"}
        </button>
      </span>
    {/if}
  {/snippet}

  {#if ddl}
    <div class="overflow-auto px-4 py-3 leading-relaxed">
      <Sql code={ddl.sql} />
    </div>
  {/if}
</Card>
