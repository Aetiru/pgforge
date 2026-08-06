<script lang="ts">
  import { untrack } from "svelte";
  import { confirmMutation } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    describeError,
    domainApply,
    domainInfo,
    domainPreview,
    type DomainChange,
    type DomainConstraint,
  } from "./ipc";

  let {
    profileId,
    database,
    schema,
    existing,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    schema: string;
    existing: { oid: number; name: string } | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  /** Una restricción en pantalla. `original` vacío es una fila nueva. */
  interface Row {
    original: string;
    name: string;
    check: string;
    notValid: boolean;
  }

  let name = $state(untrack(() => existing?.name ?? ""));
  let dataType = $state("text");
  let defaultValue = $state("");
  let notNull = $state(false);
  let rows = $state<Row[]>([]);

  let before = $state<{ default: string | null; notNull: boolean; constraints: DomainConstraint[] }>(
    { default: null, notNull: false, constraints: [] },
  );

  let loading = $state(untrack(() => existing !== null));
  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  $effect(() => {
    if (!existing) return;
    loading = true;
    domainInfo(profileId, existing.oid, database)
      .then((info) => {
        dataType = info.dataType;
        defaultValue = info.default ?? "";
        notNull = info.notNull;
        before = { default: info.default, notNull: info.notNull, constraints: info.constraints };
        rows = info.constraints.map((constraint) => ({
          original: constraint.name ?? "",
          name: constraint.name ?? "",
          check: constraint.check,
          notValid: constraint.notValid,
        }));
      })
      .catch((e) => (error = describeError(e)))
      .finally(() => (loading = false));
  });

  function constraintOf(row: Row): DomainConstraint {
    return {
      name: row.name.trim() === "" ? null : row.name.trim(),
      check: row.check,
      notValid: row.notValid,
    };
  }

  function changes(): DomainChange[] {
    const target = existing ? existing.name : name.trim();

    if (!existing) {
      return [
        {
          kind: "createDomain",
          schema,
          name: target,
          dataType: dataType.trim(),
          collation: null,
          default: defaultValue.trim() === "" ? null : defaultValue.trim(),
          notNull,
          constraints: rows.filter((row) => row.check.trim() !== "").map(constraintOf),
        },
      ];
    }

    const list: DomainChange[] = [];
    const nuevoDefault = defaultValue.trim() === "" ? null : defaultValue.trim();
    if (nuevoDefault !== before.default) {
      list.push({ kind: "setDomainDefault", schema, name: target, default: nuevoDefault });
    }
    if (notNull !== before.notNull) {
      list.push({ kind: "setDomainNotNull", schema, name: target, notNull });
    }

    // Una restricción no se edita en el lugar: cambiarle la expresión es borrarla y agregarla de
    // nuevo, y las dos cosas van en la misma transacción.
    const vivas = new Set(rows.filter((row) => row.original !== "").map((row) => row.original));
    for (const previa of before.constraints) {
      const nombre = previa.name;
      if (!nombre) continue;
      const fila = rows.find((row) => row.original === nombre);
      if (!vivas.has(nombre) || (fila && fila.check.trim() !== previa.check)) {
        list.push({
          kind: "dropDomainConstraint",
          schema,
          name: target,
          constraint: nombre,
          ifExists: false,
          cascade: false,
        });
      }
    }
    for (const row of rows) {
      if (row.check.trim() === "") continue;
      const previa = before.constraints.find((c) => c.name === row.original);
      if (!previa || previa.check !== row.check.trim()) {
        list.push({
          kind: "addDomainConstraint",
          schema,
          name: target,
          constraint: constraintOf(row),
        });
      }
    }

    return list;
  }

  function validate(): string | null {
    if (!existing && !name.trim()) return "Poné un nombre para el dominio.";
    if (!dataType.trim()) return "Poné el tipo base del dominio.";
    if (existing && changes().length === 0) return "No hay nada que cambiar.";
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
      const statements = await domainPreview(changes());
      preview = statements.map((statement) => statement.sql).join(";\n\n");
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

    if (!(await confirmMutation(profileId, "Se va a modificar un dominio."))) return;

    saving = true;
    try {
      await domainApply(profileId, changes(), database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={existing ? `Editar ${existing.name}` : "Nuevo dominio"}
  subtitle={schema}
  size="lg"
  busy={saving}
  {onclose}
>
  {#if loading}
    <p class="flex items-center justify-center gap-2 py-8 text-sm muted">
      <span class="spinner"></span>
      Leyendo la definición…
    </p>
  {:else}
    <div class="grid grid-cols-2 gap-3">
      <label class="flex flex-col gap-1">
        <span class="label">Nombre</span>
        <input class="field" data-autofocus bind:value={name} disabled={existing !== null} />
      </label>

      <label class="flex flex-col gap-1">
        <span class="label">Tipo base</span>
        <input class="field" bind:value={dataType} disabled={existing !== null} />
      </label>
    </div>

    <div class="mt-3 grid grid-cols-2 gap-3">
      <label class="flex flex-col gap-1">
        <span class="label">Valor por omisión</span>
        <input class="field" bind:value={defaultValue} placeholder="expresión SQL, opcional" />
      </label>

      <label class="check self-end pb-2">
        <input type="checkbox" bind:checked={notNull} />
        No admite NULL
      </label>
    </div>

    <div class="mt-4 flex flex-col gap-2">
      <span class="label">Restricciones</span>
      {#each rows as row, index (index)}
        <div class="flex items-center gap-2">
          <input
            class="field w-40"
            placeholder="nombre (opcional)"
            bind:value={row.name}
            disabled={row.original !== ""}
          />
          <input class="field flex-1" placeholder="VALUE > 0" bind:value={row.check} />
          <label class="check shrink-0" title="No revisa lo que ya está guardado">
            <input type="checkbox" bind:checked={row.notValid} />
            NOT VALID
          </label>
          <button
            class="btn btn-icon"
            title="Quitar"
            aria-label="Quitar restricción"
            onclick={() => (rows = rows.filter((_, i) => i !== index))}
          >
            <Icon name="trash" size={12} />
          </button>
        </div>
      {/each}
      <button
        class="btn btn-sm self-start"
        onclick={() =>
          (rows = [...rows, { original: "", name: "", check: "", notValid: false }])}
      >
        <Icon name="plus" size={11} />
        Agregar restricción
      </button>
      <p class="text-xs muted">
        <code>VALUE</code> es el valor que se está validando. La expresión va cruda al servidor, que
        es quien la valida.
      </p>
    </div>
  {/if}

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#if preview}
    <SqlPreview sql={preview} />
  {/if}

  {#snippet footer()}
    <button class="btn btn-ghost btn-sm" onclick={showPreview} disabled={saving || loading}>
      Ver SQL
    </button>
    <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
    <button class="btn btn-primary" onclick={submit} disabled={saving || loading}>
      {#if saving}<span class="spinner"></span>{/if}
      {existing ? "Guardar" : "Crear"}
    </button>
  {/snippet}
</Modal>
