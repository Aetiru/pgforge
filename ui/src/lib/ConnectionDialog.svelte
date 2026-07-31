<script lang="ts">
  import { untrack } from "svelte";
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

<div class="fixed inset-0 z-10 flex items-center justify-center bg-black/40 p-4">
  <div
    class="w-full max-w-lg rounded-lg bg-white shadow-xl dark:bg-neutral-900"
    role="dialog"
    aria-modal="true"
    aria-label="Datos del servidor"
  >
    <h2 class="border-b border-neutral-200 px-5 py-3 text-base font-medium dark:border-neutral-800">
      {profile ? "Editar servidor" : "Nuevo servidor"}
    </h2>

    <div class="grid grid-cols-2 gap-3 px-5 py-4 text-sm">
      <label class="col-span-2 flex flex-col gap-1">
        <span class="text-xs text-neutral-500">Nombre</span>
        <input class="field" bind:value={form.name} placeholder="Producción" />
      </label>

      <label class="flex flex-col gap-1">
        <span class="text-xs text-neutral-500">Servidor</span>
        <input class="field" bind:value={form.host} />
      </label>

      <label class="flex flex-col gap-1">
        <span class="text-xs text-neutral-500">Puerto</span>
        <input class="field" type="number" bind:value={form.port} />
      </label>

      <label class="flex flex-col gap-1">
        <span class="text-xs text-neutral-500">Base de datos</span>
        <input class="field" bind:value={form.database} />
      </label>

      <label class="flex flex-col gap-1">
        <span class="text-xs text-neutral-500">Usuario</span>
        <input class="field" bind:value={form.user} />
      </label>

      <label class="flex flex-col gap-1">
        <span class="text-xs text-neutral-500">Contraseña</span>
        <input class="field" type="password" bind:value={password} autocomplete="off" />
      </label>

      <label class="flex flex-col gap-1">
        <span class="text-xs text-neutral-500">Cifrado</span>
        <select class="field" bind:value={form.sslMode}>
          {#each SSL_MODES as mode (mode.value)}
            <option value={mode.value}>{mode.label}</option>
          {/each}
        </select>
      </label>

      <label class="col-span-2 flex items-center gap-2">
        <input type="checkbox" bind:checked={form.savePassword} />
        <span class="text-xs text-neutral-500">
          Recordar la contraseña en el almacén de credenciales del sistema
        </span>
      </label>

      {#if error}
        <p class="col-span-2 text-sm text-red-600 dark:text-red-400">{error}</p>
      {/if}
    </div>

    <div
      class="flex justify-end gap-2 border-t border-neutral-200 px-5 py-3 dark:border-neutral-800"
    >
      <button class="btn" onclick={onclose} disabled={saving}>Cancelar</button>
      <button class="btn" onclick={() => submit(false)} disabled={saving}>Guardar</button>
      <button class="btn btn-primary" onclick={() => submit(true)} disabled={saving}>
        Guardar y conectar
      </button>
    </div>
  </div>
</div>
