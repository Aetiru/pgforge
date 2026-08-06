<script lang="ts">
  import { untrack } from "svelte";
  import { confirmMutation } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    databaseApply,
    databaseInfo,
    databasePreview,
    describeError,
    type DatabaseChange,
  } from "./ipc";

  let {
    profileId,
    existing,
    onclose,
    onsaved,
  }: {
    profileId: string;
    /** `null` da de alta; si no, edita la base que llega acá. */
    existing: string | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  let name = $state(untrack(() => existing ?? ""));
  let owner = $state("");
  let template = $state("");
  let encoding = $state("");
  let connectionLimit = $state("-1");
  let allowConnections = $state(true);

  let before = $state<{ owner: string; connectionLimit: number; allowConnections: boolean } | null>(
    null,
  );

  let loading = $state(untrack(() => existing !== null));
  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  $effect(() => {
    if (!existing) return;
    loading = true;
    databaseInfo(profileId, existing)
      .then((info) => {
        owner = info.owner;
        encoding = info.encoding;
        connectionLimit = String(info.connectionLimit);
        allowConnections = info.allowConnections;
        before = {
          owner: info.owner,
          connectionLimit: info.connectionLimit,
          allowConnections: info.allowConnections,
        };
      })
      .catch((e) => (error = describeError(e)))
      .finally(() => (loading = false));
  });

  function limit(): number | null {
    const trimmed = connectionLimit.trim();
    if (trimmed === "") return null;
    const value = Number(trimmed);
    return Number.isInteger(value) ? value : null;
  }

  function changes(): DatabaseChange[] {
    if (!existing) {
      return [
        {
          kind: "createDatabase",
          name: name.trim(),
          options: {
            owner: owner.trim() === "" ? null : owner.trim(),
            template: template.trim() === "" ? null : template.trim(),
            encoding: encoding.trim() === "" ? null : encoding.trim(),
            connectionLimit: limit(),
          },
        },
      ];
    }

    const list: DatabaseChange[] = [];
    if (name.trim() !== existing) {
      list.push({ kind: "renameDatabase", name: existing, newName: name.trim() });
    }
    const target = name.trim();
    if (before && owner.trim() !== "" && owner.trim() !== before.owner) {
      list.push({ kind: "setDatabaseOwner", name: target, owner: owner.trim() });
    }
    const nuevoLimite = limit();
    if (before && nuevoLimite !== null && nuevoLimite !== before.connectionLimit) {
      list.push({ kind: "setDatabaseConnectionLimit", name: target, limit: nuevoLimite });
    }
    if (before && allowConnections !== before.allowConnections) {
      list.push({ kind: "setDatabaseAllowConnections", name: target, allow: allowConnections });
    }
    return list;
  }

  function validate(): string | null {
    if (!name.trim()) return "Poné un nombre para la base.";
    if (limit() === null && connectionLimit.trim() !== "") {
      return "El límite de conexiones tiene que ser un entero (-1 es sin límite).";
    }
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
      const statements = await databasePreview(changes());
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

    if (!(await confirmMutation(profileId, "Se va a modificar una base."))) return;

    saving = true;
    try {
      await databaseApply(profileId, changes());
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={existing ? `Editar ${existing}` : "Nueva base"}
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
        <input class="field" data-autofocus bind:value={name} />
      </label>

      <label class="flex flex-col gap-1">
        <span class="label">Dueño</span>
        <input class="field" bind:value={owner} placeholder="el rol conectado" />
      </label>

      {#if !existing}
        <label class="flex flex-col gap-1">
          <span class="label">Plantilla</span>
          <input class="field" bind:value={template} placeholder="template1" />
        </label>

        <label class="flex flex-col gap-1">
          <span class="label">Codificación</span>
          <input class="field" bind:value={encoding} placeholder="la de la plantilla" />
        </label>
      {/if}

      <label class="flex flex-col gap-1">
        <span class="label">Límite de conexiones</span>
        <input class="field" bind:value={connectionLimit} />
      </label>

      {#if existing}
        <label class="check self-end pb-2" title="Impide que se conecte nadie más">
          <input type="checkbox" bind:checked={allowConnections} />
          Admite conexiones
        </label>
      {/if}
    </div>

    <p class="mt-3 text-xs muted">
      Crear o borrar una base no corre adentro de una transacción, así que los cambios se mandan uno
      por uno y lo que ya se hizo queda hecho aunque el siguiente falle.
    </p>
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
