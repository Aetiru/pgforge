<script lang="ts">
  import { t } from "../i18n.svelte";
  /** Las columnas de la tabla elegida, con lo que se puede hacer con cada una. */
  import Icon from "../Icon.svelte";
  import type { TableColumn, TableShape } from "../ipc";
  import Card from "./Card.svelte";

  let {
    shape,
    loading = false,
    error = null,
    blocked = {},
    onnew,
    onedit,
    ondrop,
  }: {
    shape: TableShape | null;
    loading?: boolean;
    error?: string | null;
    blocked?: { disabled?: boolean; title?: string };
    onnew: () => void;
    onedit: (column: TableColumn) => void;
    ondrop: (column: string) => void;
  } = $props();
</script>

<Card title={t("Columnas")} loading={loading ? t("Leyendo columnas…") : null} {error}>
  {#snippet actions()}
    {#if shape}
      <button class="btn btn-sm ml-auto" {...blocked} onclick={onnew}>
        <Icon name="plus" size={11} />
        {t("Columna")}
      </button>
    {/if}
  {/snippet}

  {#if shape}
    <div class="table-scroll">
      <table class="list-table">
        <thead>
          <tr>
            <th class="w-px whitespace-nowrap">{t("Nombre")}</th>
            <th class="w-px whitespace-nowrap">{t("Tipo")}</th>
            <th class="w-full">{t("Valor por omisión")}</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {#each shape.columns as column (column.name)}
            <tr class="group">
              <td class="w-px font-medium whitespace-nowrap">
                {column.name}
                {#if column.notNull}
                  <span class="tag tag-neutral ml-1">NOT NULL</span>
                {/if}
              </td>
              <td class="w-px font-mono text-xs whitespace-nowrap muted">{column.typeName}</td>
              <td class="max-w-0 truncate text-xs muted">
                {column.default ?? (column.generated ? t("generada por el servidor") : "—")}
              </td>
              <td class="w-28">
                <div class="row-actions">
                  {#if !column.generated}
                    <button
                      class="btn btn-ghost btn-icon size-6"
                      title={t("Editar la columna")}
                      aria-label={t("Editar la columna")}
                      {...blocked}
                      onclick={() => onedit(column)}
                    >
                      <Icon name="edit" size={12} />
                    </button>
                  {/if}
                  <button
                    class="btn btn-danger-ghost btn-icon size-6"
                    title={t("Eliminar la columna")}
                    aria-label={t("Eliminar la columna")}
                    {...blocked}
                    onclick={() => ondrop(column.name)}
                  >
                    <Icon name="trash" size={12} />
                  </button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</Card>
