<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { explorer } from "./explorer.svelte";
  import { describeError, saveProfile, type ConnectionProfile, type SslMode } from "./ipc";

  let {
    profile,
    onclose,
    onsaved,
  }: {
    profile: ConnectionProfile | null;
    onclose: () => void;
    onsaved: (profile: ConnectionProfile, password?: string) => void;
  } = $props();

  function blank(): ConnectionProfile {
    return {
      id: crypto.randomUUID(),
      name: "",
      host: "localhost",
      port: 5432,
      database: "postgres",
      user: "postgres",
      sslMode: "prefer",
      connectTimeoutSecs: 10,
      savePassword: false,
    };
  }

  // El diálogo se crea de nuevo cada vez que se abre, así que tomar el valor inicial es lo que
  // corresponde: el formulario es una copia editable, no un espejo del perfil guardado.
  let form = $state<ConnectionProfile>(untrack(() => (profile ? { ...profile } : blank())));
  let password = $state("");
  let error = $state<string | null>(null);
  let saving = $state(false);

  const SSL_MODES: { value: SslMode; label: string }[] = [
    { value: "disable", label: "Sin cifrado" },
    { value: "prefer", label: "Preferir cifrado" },
    { value: "require", label: "Exigir cifrado (sin validar)" },
    { value: "verifyCa", label: "Validar la cadena del certificado" },
    { value: "verifyFull", label: "Validar cadena y nombre del servidor" },
  ];

  async function submit(connect: boolean) {
    error = null;
    if (!form.name.trim()) {
      error = "Poné un nombre para identificar el servidor.";
      return;
    }

    saving = true;
    try {
      const saved = await saveProfile($state.snapshot(form), password || undefined);
      onsaved(saved, connect ? password || undefined : undefined);
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={profile ? "Editar servidor" : "Nuevo servidor"}
  subtitle="{form.user}@{form.host}:{form.port}/{form.database}"
  busy={saving}
  {onclose}
>
  <div class="grid grid-cols-2 gap-3">
    <label class="flex flex-col gap-1">
      <span class="label">Nombre</span>
      <input class="field" data-autofocus bind:value={form.name} placeholder="Producción" />
    </label>

    <!--
      La carpeta es texto libre con las que ya existen a mano: no hay una lista de carpetas que
      administrar aparte, una carpeta es el nombre que comparten unos servidores.
    -->
    <label class="flex flex-col gap-1">
      <span class="label">Carpeta</span>
      <input
        class="field"
        list="carpetas-de-conexiones"
        placeholder="Sin carpeta"
        title="Agrupa este servidor con los demás que tengan el mismo nombre de carpeta"
        bind:value={form.group}
      />
      <datalist id="carpetas-de-conexiones">
        {#each explorer.groups as group (group)}
          <option value={group}></option>
        {/each}
      </datalist>
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Servidor</span>
      <input class="field" bind:value={form.host} />
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Puerto</span>
      <input class="field" type="number" bind:value={form.port} />
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Base de datos</span>
      <input class="field" bind:value={form.database} />
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Usuario</span>
      <input class="field" bind:value={form.user} />
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Contraseña</span>
      <input
        class="field"
        type="password"
        bind:value={password}
        autocomplete="off"
        onkeydown={(event) => {
          if (event.key === "Enter") submit(true);
        }}
      />
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Cifrado</span>
      <select class="field" bind:value={form.sslMode}>
        {#each SSL_MODES as mode (mode.value)}
          <option value={mode.value}>{mode.label}</option>
        {/each}
      </select>
    </label>

    <label class="check col-span-2">
      <input type="checkbox" bind:checked={form.savePassword} />
      Recordar la contraseña en el almacén de credenciales del sistema
    </label>
  </div>

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#snippet footer()}
    <span class="basis-full text-xs muted">
      La contraseña nunca se guarda en los archivos de la aplicación.
    </span>
    <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
    <button class="btn" onclick={() => submit(false)} disabled={saving}>Guardar</button>
    <button class="btn btn-primary" onclick={() => submit(true)} disabled={saving}>
      {#if saving}<span class="spinner"></span>{/if}
      Guardar y conectar
    </button>
  {/snippet}
</Modal>
