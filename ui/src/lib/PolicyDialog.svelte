<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    describeError,
    policyApply,
    policyPreview,
    type PolicyChange,
    type PolicyCommand,
    type PolicyInfo,
    type PolicyKind,
  } from "./ipc";

  let {
    profileId,
    database,
    schema,
    table,
    existing,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    schema: string;
    table: string;
    /** `null` da de alta; si no, reemplaza la política que llega acá (borra y crea de nuevo). */
    existing: PolicyInfo | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  const COMMAND_OPTIONS: { value: PolicyCommand; label: string }[] = [
    { value: "all", label: "ALL" },
    { value: "select", label: "SELECT" },
    { value: "insert", label: "INSERT" },
    { value: "update", label: "UPDATE" },
    { value: "delete", label: "DELETE" },
  ];

  // Copia editable, tomada una sola vez: el diálogo se crea de nuevo cada vez que se abre.
  let name = $state(untrack(() => existing?.name ?? ""));
  let command = $state<PolicyCommand>(untrack(() => existing?.command ?? "all"));
  let kind = $state<PolicyKind>(untrack(() => existing?.kind ?? "permissive"));
  let roles = $state(untrack(() => existing?.roles.join(", ") ?? ""));
  let usingText = $state(untrack(() => existing?.using ?? ""));
  let checkText = $state(untrack(() => existing?.check ?? ""));

  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  // Las dos expresiones no valen para todos los comandos, y en vez de dejar escribir algo que el
  // servidor va a rechazar se esconde el campo que no corresponde.
  const acceptsUsing = $derived(command !== "insert");
  const acceptsCheck = $derived(command !== "select" && command !== "delete");

  const roleList = $derived(
    roles
      .split(",")
      .map((role) => role.trim())
      .filter((role) => role.length > 0),
  );

  function changes(): PolicyChange[] {
    const definition = {
      command,
      kind,
      roles: roleList,
      using: acceptsUsing ? usingText.trim() || null : null,
      check: acceptsCheck ? checkText.trim() || null : null,
    };
    const create: PolicyChange = {
      kind: "createPolicy",
      schema,
      table,
      name: name.trim(),
      definition,
    };
    if (!existing) return [create];
    return [{ kind: "dropPolicy", schema, table, name: existing.name }, create];
  }

  function validate(): string | null {
    if (!name.trim()) return "Poné un nombre para la política.";
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
      const statements = await policyPreview(changes());
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

    saving = true;
    try {
      await policyApply(profileId, changes(), database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={existing ? `Editar la política ${existing.name}` : "Nueva política"}
  subtitle="{schema}.{table}"
  size="lg"
  busy={saving}
  {onclose}
>
  {#if existing}
    <p class="mb-3 text-xs muted">
      PostgreSQL no deja cambiar el comando ni el carácter restrictivo de una política que ya
      existe: se borra y se vuelve a crear con lo que quede acá, todo dentro de la misma
      transacción.
    </p>
  {/if}

  <div class="grid grid-cols-2 gap-3">
    <label class="flex flex-col gap-1">
      <span class="label">Nombre</span>
      <input class="field" data-autofocus bind:value={name} />
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Comando</span>
      <select class="field" bind:value={command}>
        {#each COMMAND_OPTIONS as option (option.value)}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    </label>
  </div>

  <label class="mt-3 flex flex-col gap-1">
    <span class="label">Roles</span>
    <input class="field" bind:value={roles} placeholder="ana, beto — vacío es PUBLIC" />
    <span class="text-xs muted">
      Separados por coma. Sin roles la política alcanza a todo el mundo.
    </span>
  </label>

  <div class="mt-3">
    <span class="label">Cómo se combina con las demás</span>
    <div class="mt-1 flex flex-col gap-1.5">
      <label class="check">
        <input type="radio" value="permissive" bind:group={kind} />
        Permisiva: suma a lo que dejan pasar las otras
      </label>
      <label class="check">
        <input type="radio" value="restrictive" bind:group={kind} />
        Restrictiva: recorta lo que las permisivas dejaron pasar
      </label>
    </div>
  </div>

  {#if acceptsUsing}
    <label class="mt-3 flex flex-col gap-1">
      <span class="label">USING — qué filas se ven</span>
      <input
        class="field font-mono"
        bind:value={usingText}
        placeholder="condición SQL, p. ej. dueno = current_user"
      />
    </label>
  {:else}
    <p class="mt-3 text-xs muted">
      Una política de INSERT no lleva USING: no hay filas previas que filtrar.
    </p>
  {/if}

  {#if acceptsCheck}
    <label class="mt-3 flex flex-col gap-1">
      <span class="label">WITH CHECK — qué filas se pueden escribir</span>
      <input
        class="field font-mono"
        bind:value={checkText}
        placeholder="condición SQL; vacío repite la de USING"
      />
    </label>
  {:else}
    <p class="mt-3 text-xs muted">
      Una política de {command === "select" ? "SELECT" : "DELETE"} no lleva WITH CHECK: no escribe ninguna
      fila que verificar.
    </p>
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
      {existing ? "Guardar" : "Crear política"}
    </button>
  {/snippet}
</Modal>
