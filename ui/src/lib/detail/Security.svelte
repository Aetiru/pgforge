<script lang="ts">
  /**
   * La seguridad por fila de la tabla: los dos interruptores y las políticas.
   *
   * Los avisos del medio son la razón de que esta sección exista aparte de las demás: las tres
   * combinaciones que engañan —filtro sin políticas, políticas sin filtro, y el dueño que se saltea
   * todo— no dan error en ningún lado, así que hay que decirlas.
   */
  import Alert from "../Alert.svelte";
  import Icon from "../Icon.svelte";
  import type { PolicyInfo, TableSecurity } from "../ipc";
  import Card from "./Card.svelte";

  let {
    security,
    loading = false,
    error = null,
    canCreate = false,
    blocked = {},
    onnew,
    onedit,
    ondrop,
    onenabled,
    onforced,
  }: {
    security: TableSecurity | null;
    loading?: boolean;
    error?: string | null;
    canCreate?: boolean;
    blocked?: { disabled?: boolean; title?: string };
    onnew: () => void;
    onedit: (policy: PolicyInfo) => void;
    ondrop: (name: string) => void;
    onenabled: (enabled: boolean) => void;
    onforced: (forced: boolean) => void;
  } = $props();
</script>

<Card title="Seguridad por fila" loading={loading ? "Leyendo las políticas…" : null} {error}>
  {#snippet actions()}
    {#if canCreate}
      <button class="btn btn-sm ml-auto" {...blocked} onclick={onnew}>
        <Icon name="plus" size={11} />
        Política
      </button>
    {/if}
  {/snippet}

  {#if security}
    <div class="flex flex-col gap-2 border-b border-zinc-200 p-3 dark:border-zinc-800">
      <label class="check">
        <input
          type="checkbox"
          checked={security.enabled}
          onchange={() => onenabled(!security!.enabled)}
        />
        Filtrar las filas según las políticas
      </label>
      <label class="check">
        <input
          type="checkbox"
          checked={security.forced}
          disabled={!security.enabled}
          onchange={() => onforced(!security!.forced)}
        />
        Aplicarlo también al dueño de la tabla
      </label>
    </div>

    {#if security.enabled && security.policies.length === 0}
      <Alert tone="warn" box class="m-3">
        El filtro está activo y no hay ninguna política: la tabla no devuelve ninguna fila.
      </Alert>
    {:else if !security.enabled && security.policies.length > 0}
      <Alert tone="warn" box class="m-3">
        Hay políticas definidas pero el filtro está apagado: no se aplica ninguna.
      </Alert>
    {:else if security.enabled && !security.forced}
      <Alert tone="ok" box class="m-3">
        El dueño de la tabla se saltea el filtro. Para probar las políticas hay que conectarse con
        otro rol, o marcar la segunda casilla.
      </Alert>
    {/if}

    {#if security.policies.length === 0}
      <p class="px-3 py-4 text-sm muted">No tiene políticas.</p>
    {:else}
      <table class="list-table">
        <thead>
          <tr>
            <th class="w-px whitespace-nowrap">Nombre</th>
            <th class="w-px whitespace-nowrap">Comando</th>
            <th class="w-px whitespace-nowrap">Roles</th>
            <th class="w-full">Condición</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {#each security.policies as policy (policy.oid)}
            <tr class="group">
              <td class="w-px font-medium whitespace-nowrap">{policy.name}</td>
              <td class="w-px whitespace-nowrap">
                <span class="tag tag-neutral font-mono">{policy.command.toUpperCase()}</span>
                {#if policy.kind === "restrictive"}
                  <span class="tag tag-info">restrictiva</span>
                {/if}
              </td>
              <td class="w-px text-xs whitespace-nowrap muted">
                {policy.roles.length === 0 ? "PUBLIC" : policy.roles.join(", ")}
              </td>
              <td class="max-w-0 truncate font-mono text-xs muted">
                {policy.using ?? ""}{policy.using && policy.check ? " · " : ""}{policy.check
                  ? `CHECK ${policy.check}`
                  : ""}
              </td>
              <td class="w-24">
                <div class="row-actions">
                  <button
                    class="btn btn-ghost btn-icon size-6"
                    title="Editar la política"
                    aria-label="Editar la política"
                    {...blocked}
                    onclick={() => onedit(policy)}
                  >
                    <Icon name="edit" size={12} />
                  </button>
                  <button
                    class="btn btn-danger-ghost btn-icon size-6"
                    title="Eliminar la política"
                    aria-label="Eliminar la política"
                    {...blocked}
                    onclick={() => ondrop(policy.name)}
                  >
                    <Icon name="trash" size={12} />
                  </button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  {/if}
</Card>
