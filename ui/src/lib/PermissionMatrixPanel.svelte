<script lang="ts">
  /**
   * La matriz de permisos: roles×objetos de un esquema, de solo lectura. Para cambiar algo se sigue
   * pasando por `PrivilegeDialog` desde el árbol — acá no se otorga ni se revoca nada.
   */
  import Alert from "./Alert.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import { cellOf, indexEffective, objectsOf, PRIVILEGES_OF } from "./permission-matrix";
  import type { MatrixObjectKind, PermissionMatrixTab } from "./permission-matrix.svelte";

  let { tab }: { tab: PermissionMatrixTab } = $props();

  // Derivada: la etiqueta sigue al idioma vigente.
  const OBJECT_KINDS: { value: MatrixObjectKind; label: string; icon: "table" | "sequence" | "function" }[] =
    $derived([
      { value: "table", label: t("Tablas"), icon: "table" },
      { value: "sequence", label: t("Secuencias"), icon: "sequence" },
      { value: "function", label: t("Funciones"), icon: "function" },
    ]);

  const privileges = $derived(PRIVILEGES_OF[tab.objectKind]);
  const objects = $derived(objectsOf(tab.effective));
  const index = $derived(indexEffective(tab.effective));

  async function changeSchema(schema: string) {
    tab.schema = schema;
    tab.title = t("Permisos · {schema}", { schema });
    await tab.load();
  }

  async function changeObjectKind(kind: MatrixObjectKind) {
    tab.objectKind = kind;
    await tab.load();
  }

  async function toggle(role: string) {
    tab.toggleRole(role);
    await tab.load();
  }
</script>

<div class="flex h-full flex-col">
  <div class="toolbar divider-b flex-wrap gap-3">
    <label class="flex flex-col gap-1">
      <span class="label">{t("Esquema")}</span>
      <select
        class="field"
        value={tab.schema}
        onchange={(event) => changeSchema(event.currentTarget.value)}
      >
        {#each tab.schemas as schema (schema)}
          <option value={schema}>{schema}</option>
        {/each}
      </select>
    </label>

    <div class="toolbar-sep"></div>

    <div class="flex flex-col gap-1">
      <span class="label">{t("Objetos")}</span>
      <div class="seg" role="tablist">
        {#each OBJECT_KINDS as { value, label, icon } (value)}
          <button
            class="seg-item"
            role="tab"
            aria-selected={tab.objectKind === value}
            onclick={() => changeObjectKind(value)}
          >
            <Icon name={icon} size={11} />
            {label}
          </button>
        {/each}
      </div>
    </div>

    <div class="toolbar-sep"></div>

    <div class="flex min-w-64 flex-1 flex-col gap-1">
      <span class="label">{t("Roles a mostrar")}</span>
      <div class="flex flex-wrap items-center gap-1.5">
        {#each tab.availableRoles as role (role)}
          <button
            class="tag {tab.selectedRoles.includes(role) ? 'tag-info' : 'tag-neutral'}"
            onclick={() => toggle(role)}
            title={tab.selectedRoles.includes(role) ? t("Sacar a {role}", { role }) : t("Mostrar a {role}", { role })}
          >
            {role}
          </button>
        {/each}
      </div>
    </div>

    <button class="btn btn-icon ml-auto" title={t("Volver a leer")} onclick={() => tab.load()}>
      <Icon name="refresh" size={12} />
    </button>
  </div>

  {#if tab.error}
    <Alert tone="bad" onclose={() => (tab.error = null)}>{tab.error}</Alert>
  {/if}

  {#if tab.selectedRoles.length === 0}
    <Empty icon="role" title={t("Sin roles elegidos")} hint={t("Marcá al menos un rol arriba para ver la matriz.")} />
  {:else if !tab.loading && objects.length === 0}
    <Empty
      icon="table"
      title={t("Sin objetos")}
      hint={tab.objectKind === "table"
        ? t("Este esquema no tiene tablas ni vistas.")
        : tab.objectKind === "sequence"
          ? t("Este esquema no tiene secuencias.")
          : t("Este esquema no tiene funciones ni procedimientos.")}
    />
  {:else}
    <div class="min-h-0 flex-1 overflow-auto">
      <table class="w-full border-collapse text-xs">
        <thead>
          <tr>
            <th
              class="sticky top-0 left-0 z-20 border-r border-b border-zinc-200 bg-zinc-50 px-3 py-2
                     text-left font-semibold text-zinc-500 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-400"
            >
              {OBJECT_KINDS.find((o) => o.value === tab.objectKind)?.label}
            </th>
            {#each tab.selectedRoles as role (role)}
              <th
                class="sticky top-0 z-10 min-w-32 border-b border-zinc-200 bg-zinc-50 px-2 py-2 text-center
                       font-semibold text-cyan-700 dark:border-zinc-700 dark:bg-zinc-800 dark:text-cyan-400"
              >
                {role}
              </th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each objects as object (object)}
            <tr class="hover:bg-blue-50 dark:hover:bg-zinc-700/50">
              <th
                class="sticky left-0 z-10 border-r border-b border-zinc-200 bg-white px-3 py-1.5
                       text-left font-medium whitespace-nowrap text-zinc-800
                       dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
              >
                {object}
              </th>
              {#each tab.selectedRoles as role (role)}
                <td class="border-b border-zinc-200/70 px-2 py-1.5 text-center dark:border-zinc-700/70">
                  <div class="flex flex-wrap items-center justify-center gap-1">
                    {#each privileges as privilege (privilege)}
                      {@const cell = cellOf(index, object, role, privilege)}
                      {#if cell?.granted}
                        <span
                          class="inline-flex h-4 w-4 items-center justify-center rounded text-[9px] font-bold
                                 {cell.direct
                            ? 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-400'
                            : 'border border-amber-500/60 text-amber-600 dark:text-amber-400'}"
                          title="{privilege} — {cell.direct ? t('otorgado directo') : t('heredado por membresía')}"
                        >
                          {privilege[0]}
                        </span>
                      {/if}
                    {/each}
                  </div>
                </td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    <div class="divider-t flex items-center gap-4 px-3 py-1.5 text-[11px] muted">
      <span class="flex items-center gap-1.5">
        <span class="inline-flex h-3.5 w-3.5 items-center justify-center rounded bg-emerald-500/15 text-emerald-700 dark:text-emerald-400"></span>
        {t("Otorgado directo")}
      </span>
      <span class="flex items-center gap-1.5">
        <span class="inline-flex h-3.5 w-3.5 items-center justify-center rounded border border-amber-500/60"></span>
        {t("Heredado por membresía")}
      </span>
      <span class="ml-auto">{privileges.map((p) => `${p[0]} = ${p}`).join(" · ")}</span>
    </div>
  {/if}
</div>
