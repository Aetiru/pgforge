<script lang="ts">
  /**
   * La estructura de la tabla que se está mirando, sin salir de su pestaña de datos: columnas,
   * índices, restricciones, triggers, estadísticas, dependencias y DDL.
   *
   * Reusa las mismas secciones del panel de detalle (`detail/*`), así que lo que se ve es
   * exactamente lo mismo; la diferencia es que acá van **de solo lectura**. Crear, editar o borrar
   * un objeto sigue siendo del panel de detalle, que es donde viven los diálogos y la confirmación
   * de producción: duplicarlos acá serían dos caminos para una misma mutación.
   */
  import Columns from "./detail/Columns.svelte";
  import Constraints from "./detail/Constraints.svelte";
  import DdlSection from "./detail/Ddl.svelte";
  import DependenciesSection from "./detail/Dependencies.svelte";
  import Indexes from "./detail/Indexes.svelte";
  import StatisticsSection from "./detail/Statistics.svelte";
  import Triggers from "./detail/Triggers.svelte";
  import {
    describeError,
    objectDdl,
    relationDependencies,
    tableConstraints,
    tableIndexes,
    tableStat,
    tableTriggers,
    type ConstraintInfo,
    type Ddl,
    type Dependency,
    type IndexInfo,
    type TableStat,
    type TriggerInfo,
  } from "./ipc";
  import type { DataTab } from "./data.svelte";
  import { openQuery } from "./query.svelte";

  export type StructureSection =
    | "columns"
    | "indexes"
    | "constraints"
    | "triggers"
    | "stats"
    | "dependencies"
    | "ddl";

  let { tab, section }: { tab: DataTab; section: StructureSection } = $props();

  /** Todo apagado con el mismo motivo: así los botones de las secciones se ven y explican por qué no. */
  const blocked = {
    disabled: true,
    title: "Se cambia desde el panel de detalle de la tabla",
  };
  const noop = () => {};

  type Slot<T> = { value: T | null; loading: boolean; error: string | null };
  const empty = <T,>(): Slot<T> => ({ value: null, loading: false, error: null });

  let indexes = $state<Slot<IndexInfo[]>>(empty());
  let constraints = $state<Slot<ConstraintInfo[]>>(empty());
  let triggers = $state<Slot<TriggerInfo[]>>(empty());
  let stat = $state<Slot<TableStat>>(empty());
  let dependencies = $state<Slot<Dependency[]>>(empty());
  let ddl = $state<Slot<Ddl>>(empty());
  let copied = $state(false);

  /** Lee `fetch` y lo deja en `slot`; una respuesta que llega tarde, de otra sección, no pisa nada. */
  async function fill<T>(slot: Slot<T>, fetch: () => Promise<T | null>) {
    slot.loading = true;
    slot.error = null;
    try {
      slot.value = await fetch();
    } catch (error) {
      slot.error = describeError(error);
    } finally {
      slot.loading = false;
    }
  }

  // Se lee al elegir la sección, cada vez: es lo que hace que volver tras crear un índice en el
  // panel de detalle lo muestre sin tener que cerrar la pestaña.
  $effect(() => {
    const current = section;
    const { profileId, database, oid, shape } = tab;
    if (current === "indexes") fill(indexes, () => tableIndexes(profileId, oid, database));
    else if (current === "constraints") fill(constraints, () => tableConstraints(profileId, oid, database));
    else if (current === "triggers") fill(triggers, () => tableTriggers(profileId, oid, database));
    else if (current === "stats") fill(stat, () => tableStat(profileId, oid, database));
    else if (current === "dependencies")
      fill(dependencies, () => relationDependencies(profileId, oid, database));
    else if (current === "ddl" && shape) {
      fill(ddl, () =>
        objectDdl(profileId, {
          id: `relation:${oid}`,
          label: shape.name,
          kind: "table",
          hasChildren: false,
          database,
          schema: shape.schema,
          oid,
        }),
      );
    }
  });

  let openError = $state<string | null>(null);

  /**
   * Abre el código de la función de un trigger en una pestaña de consulta, listo para leer o editar.
   * Es el `CREATE OR REPLACE` que escribe `pg_get_functiondef`: correrlo la reemplaza, que es lo que
   * se quiere al editarla, y por eso pasa por el editor y su confirmación de producción y no se
   * ejecuta solo.
   */
  async function openFunction(trigger: TriggerInfo) {
    openError = null;
    try {
      const result = await objectDdl(tab.profileId, {
        id: `function:${trigger.functionOid}`,
        label: trigger.functionName,
        kind: "function",
        hasChildren: false,
        database: tab.database,
        schema: trigger.functionSchema,
        oid: trigger.functionOid,
      });
      const query = await openQuery(tab.profileId, tab.database, trigger.functionName, false);
      query.sql = result.sql;
    } catch (error) {
      openError = describeError(error);
    }
  }

  async function copyDdl() {
    const text = ddl.value?.sql;
    if (!text) return;
    await navigator.clipboard.writeText(text).catch(() => {});
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

<div class="h-full overflow-auto p-3">
  {#if section === "columns"}
    <Columns
      shape={tab.shape}
      loading={tab.shape === null}
      {blocked}
      onnew={noop}
      onedit={noop}
      ondrop={noop}
    />
  {:else if section === "indexes"}
    <Indexes
      indexes={indexes.value}
      loading={indexes.loading}
      error={indexes.error}
      {blocked}
      onnew={noop}
      ondrop={noop}
    />
  {:else if section === "constraints"}
    <Constraints
      constraints={constraints.value}
      loading={constraints.loading}
      error={constraints.error}
      {blocked}
      onnew={noop}
      ondrop={noop}
    />
  {:else if section === "triggers"}
    <Triggers
      triggers={triggers.value}
      loading={triggers.loading}
      error={triggers.error}
      {blocked}
      onnew={noop}
      onedit={noop}
      ondrop={noop}
      onopenfunction={openFunction}
    />
    {#if openError}
      <p class="mt-2 text-xs text-rose-600 dark:text-rose-400">{openError}</p>
    {/if}
  {:else if section === "stats"}
    <StatisticsSection
      stat={stat.value}
      loading={stat.loading}
      error={stat.error}
      {blocked}
      onmaintenance={noop}
    />
  {:else if section === "dependencies"}
    <DependenciesSection
      dependencies={dependencies.value}
      loading={dependencies.loading}
      error={dependencies.error}
    />
  {:else}
    <DdlSection ddl={ddl.value} loading={ddl.loading} error={ddl.error} {copied} oncopy={copyDdl} />
  {/if}
</div>
