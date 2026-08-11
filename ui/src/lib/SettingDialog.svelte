<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    describeError,
    settingsApply,
    settingsPreview,
    type Setting,
    type SettingChange,
  } from "./ipc";

  let {
    profileId,
    setting,
    onclose,
    onsaved,
  }: {
    profileId: string;
    setting: Setting;
    onclose: () => void;
    /** `pendingRestart` es true si el cambio necesita reiniciar el servidor. */
    onsaved: (pendingRestart: boolean) => void;
  } = $props();

  // Sin permiso para leerlo, el campo arranca vacío en vez de con `null`: escribir uno nuevo sigue
  // estando permitido, porque `ALTER SYSTEM` no exige poder ver el valor de ahora.
  let value = $state(untrack(() => setting.value ?? ""));
  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  const needsRestart = untrack(() => setting.context === "postmaster");
  const unit = untrack(() => (setting.unit ? ` ${setting.unit}` : ""));

  function setChange(): SettingChange[] {
    return [{ kind: "set", name: setting.name, value: value.trim() }];
  }

  async function showPreview() {
    error = null;
    try {
      const statements = await settingsPreview(setChange());
      preview = statements.map((statement) => statement.sql).join(";\n\n");
    } catch (e) {
      error = describeError(e);
    }
  }

  async function run(changes: SettingChange[]) {
    error = null;
    saving = true;
    try {
      const pending = await settingsApply(profileId, changes);
      onsaved(pending);
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal title={setting.name} subtitle={setting.category} busy={saving} {onclose}>
  <p class="text-sm muted select-text">{setting.shortDesc}</p>

  <div class="mt-3 flex flex-col gap-1">
    <span class="label">Valor{unit ? ` (${setting.unit})` : ""}</span>
    {#if setting.varType === "bool"}
      <select class="field" bind:value>
        <option value="on">on</option>
        <option value="off">off</option>
      </select>
    {:else if setting.varType === "enum"}
      <select class="field" bind:value>
        {#each setting.enumVals as option (option)}
          <option value={option}>{option}</option>
        {/each}
      </select>
    {:else if setting.varType === "integer" || setting.varType === "real"}
      <input
        class="field"
        type="number"
        bind:value
        min={setting.minVal ?? undefined}
        max={setting.maxVal ?? undefined}
        step={setting.varType === "real" ? "any" : "1"}
      />
    {:else}
      <input class="field" bind:value />
    {/if}

    <p class="text-xs muted">
      Por omisión: <span class="select-text font-mono">{setting.bootVal ?? "—"}{setting.bootVal
          ? unit
          : ""}</span>
      {#if setting.minVal && setting.maxVal}
        · rango {setting.minVal}–{setting.maxVal}
      {/if}
    </p>
  </div>

  {#if needsRestart}
    <Alert tone="warn" box class="mt-3">
      Este parámetro solo toma efecto tras reiniciar el servidor: el cambio queda guardado pero
      pendiente.
    </Alert>
  {/if}

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#if preview}
    <SqlPreview sql={preview} />
  {/if}

  {#snippet footer()}
    <button class="btn btn-ghost btn-sm" onclick={showPreview} disabled={saving}>Ver SQL</button>
    <button
      class="btn btn-danger-ghost ml-auto"
      title="ALTER SYSTEM RESET: vuelve al valor por omisión"
      onclick={() => run([{ kind: "reset", name: setting.name }])}
      disabled={saving}
    >
      Restablecer
    </button>
    <button class="btn" onclick={onclose} disabled={saving}>Cancelar</button>
    <button class="btn btn-primary" onclick={() => run(setChange())} disabled={saving}>
      {#if saving}<span class="spinner"></span>{/if}
      Aplicar
    </button>
  {/snippet}
</Modal>
