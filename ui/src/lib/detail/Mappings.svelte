<script lang="ts">
  import { t } from "../i18n.svelte";
  /** Los mapeos de usuario de un servidor foráneo: qué rol local se autentica y con qué. */
  import Icon from "../Icon.svelte";
  import type { UserMapping } from "../ipc";
  import Card from "./Card.svelte";

  let {
    mappings,
    error = null,
    blocked = {},
    onnew,
    onedit,
    ondrop,
  }: {
    mappings: UserMapping[];
    error?: string | null;
    blocked?: { disabled?: boolean; title?: string };
    onnew: () => void;
    onedit: (mapping: UserMapping) => void;
    ondrop: (user: string) => void;
  } = $props();
</script>

<Card
  title={t("Mapeos de usuario")}
  {error}
  empty={mappings.length === 0 ? t("No tiene mapeos de usuario.") : null}
>
  {#snippet actions()}
    <button class="btn btn-sm ml-auto" {...blocked} onclick={onnew}>
      <Icon name="plus" size={11} />
      {t("Mapeo")}
    </button>
  {/snippet}

  <div class="table-scroll">
    <table class="list-table">
      <thead>
        <tr>
          <th class="w-px whitespace-nowrap">{t("Rol")}</th>
          <th class="w-full">{t("Opciones")}</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each mappings as mapping (mapping.user)}
          <tr class="group">
            <td class="w-px font-medium whitespace-nowrap">{mapping.user}</td>
            <td class="max-w-0 truncate font-mono text-xs muted">
              {#if mapping.options === null}
                {t("(ocultas)")}
              {:else}
                {mapping.options
                  .map(([key, value]) => (key === "password" ? `${key}=••••` : `${key}=${value}`))
                  .join(", ") || "—"}
              {/if}
            </td>
            <td class="w-24">
              <div class="row-actions">
                <button
                  class="btn btn-ghost btn-icon size-6"
                  title={t("Editar el mapeo")}
                  aria-label={t("Editar el mapeo")}
                  {...blocked}
                  onclick={() => onedit(mapping)}
                >
                  <Icon name="edit" size={12} />
                </button>
                <button
                  class="btn btn-danger-ghost btn-icon size-6"
                  title={t("Quitar el mapeo")}
                  aria-label={t("Quitar el mapeo")}
                  onclick={() => ondrop(mapping.user)}
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
</Card>
