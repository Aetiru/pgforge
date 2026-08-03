<script lang="ts" module>
  import type { Grantable } from "./ipc";

  /**
   * El objeto del que habla el diálogo: un `Grantable` sin su lista de privilegios, que es
   * justamente lo que se elige acá.
   */
  export type Subject =
    | { on: "table"; schema: string; table: string }
    | { on: "schema"; schema: string }
    | { on: "sequence"; schema: string; sequence: string }
    | { on: "function"; schema: string; name: string; args: string; procedure: boolean }
    | { on: "database"; database: string };

  /** Lo que ya tiene otorgado el destinatario que se está editando. */
  export interface Existing {
    grantee: string;
    privileges: string[];
    grantable: boolean;
    /** Por columna: pares columna/privilegio, ya filtrados por este destinatario. */
    columns?: { column: string; privilege: string }[];
  }

  const OPTIONS: Record<Subject["on"], string[]> = {
    table: ["select", "insert", "update", "delete", "truncate", "references", "trigger"],
    schema: ["usage", "create"],
    sequence: ["usage", "select", "update"],
    function: ["execute"],
    database: ["connect", "create", "temporary"],
  };

  const COLUMN_OPTIONS = ["select", "insert", "update", "references"];
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    describeError,
    privilegeApply,
    privilegePreview,
    type ColumnPrivilege,
    type PrivilegeChange,
  } from "./ipc";

  let {
    profileId,
    database,
    subject,
    columns = [],
    existing,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    subject: Subject;
    /** Las columnas de la tabla, para poder acotar un privilegio a algunas de ellas. */
    columns?: string[];
    /** `null` da de alta; si no, edita lo que ya tiene este destinatario. */
    existing: Existing | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  const options = $derived(OPTIONS[subject.on]);
  const acceptsColumns = $derived(subject.on === "table" && columns.length > 0);

  let grantee = $state(untrack(() => existing?.grantee ?? ""));
  let selected = $state<string[]>(untrack(() => existing?.privileges ?? []));
  let grantOption = $state(untrack(() => existing?.grantable ?? false));
  let cascade = $state(false);

  // Lo que ya estaba otorgado por columna se muestra como una sola combinación —estas columnas,
  // estos privilegios—, que es como se otorga desde acá. Una matriz completa entraría, pero nadie
  // arma privilegios por columna de a una celda.
  const originalColumns = untrack(() => [
    ...new Set((existing?.columns ?? []).map((grant) => grant.column)),
  ]);
  const originalColumnPrivileges = untrack(() => [
    ...new Set((existing?.columns ?? []).map((grant) => grant.privilege.toLowerCase())),
  ]);

  let pickedColumns = $state<string[]>(originalColumns);
  let columnPrivileges = $state<string[]>(originalColumnPrivileges);

  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  function toggle(list: string[], value: string): string[] {
    return list.includes(value) ? list.filter((item) => item !== value) : [...list, value];
  }

  /**
   * El `Grantable` que corresponde a este objeto, con los privilegios elegidos. La conversión es
   * segura por construcción: los valores salen de `OPTIONS[subject.on]`, que es el vocabulario de
   * este tipo de objeto y de ningún otro.
   */
  function target(privileges: string[]): Grantable {
    return { ...subject, privileges } as Grantable;
  }

  function name(): string {
    return existing ? existing.grantee : grantee.trim();
  }

  function objectChanges(): PrivilegeChange[] {
    const out: PrivilegeChange[] = [];

    if (selected.length > 0) {
      out.push({ kind: "grant", target: target(selected), grantee: name(), grantOption });
    }

    const removed = (existing?.privileges ?? []).filter((p) => !selected.includes(p));
    if (removed.length > 0) {
      out.push({
        kind: "revoke",
        target: target(removed),
        grantee: name(),
        grantOptionOnly: false,
        cascade,
      });
    }

    return out;
  }

  function columnTarget(picked: string[], privileges: string[]): Grantable {
    if (subject.on !== "table") throw new Error("solo una tabla tiene privilegios por columna");
    return {
      on: "columns",
      schema: subject.schema,
      table: subject.table,
      columns: picked,
      privileges: privileges as ColumnPrivilege[],
    };
  }

  /**
   * El diff entre lo que había y lo que quedó son dos revocaciones distintas: las columnas que
   * salieron pierden todo lo que tenían, y las que siguen pierden solo los privilegios
   * desmarcados.
   */
  function columnChanges(): PrivilegeChange[] {
    if (!acceptsColumns) return [];
    const out: PrivilegeChange[] = [];

    if (pickedColumns.length > 0 && columnPrivileges.length > 0) {
      out.push({
        kind: "grant",
        target: columnTarget(pickedColumns, columnPrivileges),
        grantee: name(),
        grantOption,
      });
    }

    const dropped = originalColumns.filter((column) => !pickedColumns.includes(column));
    if (dropped.length > 0 && originalColumnPrivileges.length > 0) {
      out.push({
        kind: "revoke",
        target: columnTarget(dropped, originalColumnPrivileges),
        grantee: name(),
        grantOptionOnly: false,
        cascade,
      });
    }

    const kept = originalColumns.filter((column) => pickedColumns.includes(column));
    const lost = originalColumnPrivileges.filter((p) => !columnPrivileges.includes(p));
    if (kept.length > 0 && lost.length > 0) {
      out.push({
        kind: "revoke",
        target: columnTarget(kept, lost),
        grantee: name(),
        grantOptionOnly: false,
        cascade,
      });
    }

    return out;
  }

  function changes(): PrivilegeChange[] {
    return [...objectChanges(), ...columnChanges()];
  }

  function validate(): string | null {
    if (!name()) return "Poné a quién otorgarle los privilegios.";
    if (!existing && changes().length === 0) return "Elegí al menos un privilegio.";
    if (pickedColumns.length > 0 && columnPrivileges.length === 0) {
      return "Elegí qué privilegio dar sobre esas columnas.";
    }
    return null;
  }

  async function showPreview() {
    error = null;
    const problem = validate();
    if (problem) {
      error = problem;
      return;
    }
    try {
      const statements = await privilegePreview(changes());
      preview = statements.map((statement) => statement.sql).join(";\n\n") || "Nada que aplicar.";
    } catch (e) {
      error = describeError(e);
    }
  }

  async function submit() {
    error = null;
    const problem = validate();
    if (problem) {
      error = problem;
      return;
    }

    const list = changes();
    if (list.length === 0) {
      onsaved();
      return;
    }

    saving = true;
    try {
      await privilegeApply(profileId, list, database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }

  const subtitle = $derived.by(() => {
    switch (subject.on) {
      case "table":
        return `${subject.schema}.${subject.table}`;
      case "schema":
        return `esquema ${subject.schema}`;
      case "sequence":
        return `secuencia ${subject.schema}.${subject.sequence}`;
      case "function":
        return `${subject.procedure ? "procedimiento" : "función"} ${subject.schema}.${subject.name}(${subject.args})`;
      case "database":
        return `base ${subject.database}`;
    }
  });
</script>

<Modal
  title={existing ? `Privilegios de ${existing.grantee}` : "Nuevo privilegio"}
  {subtitle}
  busy={saving}
  {onclose}
>
  <label class="flex flex-col gap-1">
    <span class="label">Rol destinatario</span>
    <input
      class="field"
      data-autofocus
      bind:value={grantee}
      disabled={existing !== null}
      placeholder="ana, o PUBLIC para todos"
    />
  </label>

  <div class="mt-3">
    <span class="label">Privilegios</span>
    <div class="mt-1 flex flex-wrap gap-x-4 gap-y-1.5">
      {#each options as option (option)}
        <label class="check">
          <input
            type="checkbox"
            checked={selected.includes(option)}
            onchange={() => (selected = toggle(selected, option))}
          />
          {option.toUpperCase()}
        </label>
      {/each}
    </div>
  </div>

  {#if acceptsColumns}
    <div class="divider-t mt-4 pt-3">
      <span class="label">Acotado a columnas</span>
      <p class="mt-0.5 text-xs muted">
        Se suma a lo de arriba, no lo reemplaza. Un SELECT sobre toda la tabla ya alcanza cada
        columna.
      </p>

      <div class="mt-2 flex flex-wrap gap-x-4 gap-y-1.5">
        {#each columns as column (column)}
          <label class="check">
            <input
              type="checkbox"
              checked={pickedColumns.includes(column)}
              onchange={() => (pickedColumns = toggle(pickedColumns, column))}
            />
            {column}
          </label>
        {/each}
      </div>

      {#if pickedColumns.length > 0}
        <div class="mt-2 flex flex-wrap gap-x-4 gap-y-1.5">
          {#each COLUMN_OPTIONS as option (option)}
            <label class="check">
              <input
                type="checkbox"
                checked={columnPrivileges.includes(option)}
                onchange={() => (columnPrivileges = toggle(columnPrivileges, option))}
              />
              {option.toUpperCase()}
            </label>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  <label class="check mt-3">
    <input type="checkbox" bind:checked={grantOption} />
    Con GRANT OPTION (puede volver a otorgarlos)
  </label>

  {#if existing}
    <label class="check mt-2">
      <input type="checkbox" bind:checked={cascade} />
      CASCADE al revocar lo que se desmarque
    </label>
  {/if}

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#if preview}
    <SqlPreview sql={preview} />
  {/if}

  {#snippet footer()}
    <button class="btn btn-ghost btn-sm" onclick={showPreview} disabled={saving}>Ver SQL</button>
    <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
    <button class="btn btn-primary" onclick={submit} disabled={saving}>
      {#if saving}<span class="spinner"></span>{/if}
      {existing ? "Guardar" : "Otorgar"}
    </button>
  {/snippet}
</Modal>
