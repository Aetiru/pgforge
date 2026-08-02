<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    describeError,
    privilegeApply,
    privilegePreview,
    type PrivilegeChange,
    type SchemaPrivilege,
    type TablePrivilege,
  } from "./ipc";

  let {
    profileId,
    database,
    kind,
    schema,
    table,
    existing,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    kind: "table" | "schema";
    schema: string;
    /** Solo hace falta cuando `kind === "table"`. */
    table?: string;
    /** `null` da de alta; si no, edita los privilegios de este `grantee`. */
    existing: { grantee: string; privileges: string[]; grantable: boolean } | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  const TABLE_OPTIONS: { value: TablePrivilege; label: string }[] = [
    { value: "select", label: "SELECT" },
    { value: "insert", label: "INSERT" },
    { value: "update", label: "UPDATE" },
    { value: "delete", label: "DELETE" },
    { value: "truncate", label: "TRUNCATE" },
    { value: "references", label: "REFERENCES" },
    { value: "trigger", label: "TRIGGER" },
  ];
  const SCHEMA_OPTIONS: { value: SchemaPrivilege; label: string }[] = [
    { value: "usage", label: "USAGE" },
    { value: "create", label: "CREATE" },
  ];
  const options = $derived(kind === "table" ? TABLE_OPTIONS : SCHEMA_OPTIONS);

  let grantee = $state(untrack(() => existing?.grantee ?? ""));
  let selected = $state<string[]>(untrack(() => existing?.privileges ?? []));
  let grantOption = $state(untrack(() => existing?.grantable ?? false));
  let cascade = $state(false);

  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  function toggle(value: string) {
    selected = selected.includes(value) ? selected.filter((v) => v !== value) : [...selected, value];
  }

  function changes(): PrivilegeChange[] {
    const out: PrivilegeChange[] = [];
    const name = existing ? existing.grantee : grantee.trim();

    if (selected.length > 0) {
      out.push(
        kind === "table"
          ? {
              kind: "grantTable",
              schema,
              table: table ?? "",
              privileges: selected as TablePrivilege[],
              grantee: name,
              grantOption,
            }
          : {
              kind: "grantSchema",
              schema,
              privileges: selected as SchemaPrivilege[],
              grantee: name,
              grantOption,
            },
      );
    }

    if (existing) {
      const removed = existing.privileges.filter((p) => !selected.includes(p));
      if (removed.length > 0) {
        out.push(
          kind === "table"
            ? {
                kind: "revokeTable",
                schema,
                table: table ?? "",
                privileges: removed as TablePrivilege[],
                grantee: name,
                grantOptionOnly: false,
                cascade,
              }
            : {
                kind: "revokeSchema",
                schema,
                privileges: removed as SchemaPrivilege[],
                grantee: name,
                grantOptionOnly: false,
                cascade,
              },
        );
      }
    }

    return out;
  }

  function validate(): string | null {
    if (!existing && !grantee.trim()) return "Poné a quién otorgarle los privilegios.";
    if (selected.length === 0 && !existing) return "Elegí al menos un privilegio.";
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
</script>

<Modal
  title={existing ? `Privilegios de ${existing.grantee}` : "Nuevo privilegio"}
  subtitle={kind === "table" ? `${schema}.${table}` : `esquema ${schema}`}
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
      {#each options as option (option.value)}
        <label class="check">
          <input
            type="checkbox"
            checked={selected.includes(option.value)}
            onchange={() => toggle(option.value)}
          />
          {option.label}
        </label>
      {/each}
    </div>
  </div>

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
