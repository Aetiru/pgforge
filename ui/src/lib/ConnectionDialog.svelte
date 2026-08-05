<script lang="ts">
  import { untrack } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { explorer } from "./explorer.svelte";
  import {
    describeError,
    saveProfile,
    sshHostKey,
    sshTest,
    type ConnectionProfile,
    type SshTunnel,
    type SslMode,
  } from "./ipc";

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

  function blankTunnel(): SshTunnel {
    return { host: "", port: 22, user: "" };
  }

  // El diálogo se crea de nuevo cada vez que se abre, así que tomar el valor inicial es lo que
  // corresponde: el formulario es una copia editable, no un espejo del perfil guardado.
  let form = $state<ConnectionProfile>(untrack(() => (profile ? { ...profile } : blank())));
  let password = $state("");
  // Contraseña del bastión, o frase de la clave privada. Solo se guarda en el almacén del sistema.
  let sshPassword = $state("");
  let error = $state<string | null>(null);
  let saving = $state(false);

  // El túnel se activa con un interruptor, pero el estado real es `form.tunnel`: cuando está apagado
  // el perfil no lleva túnel, no queda uno "escondido" que se guardaría igual.
  let tunnelOn = $state(untrack(() => !!form.tunnel));
  $effect(() => {
    if (tunnelOn && !form.tunnel) form.tunnel = blankTunnel();
    if (!tunnelOn && form.tunnel) form.tunnel = undefined;
  });

  // Prueba del túnel, con su propio flujo de confirmación de clave de host: la huella sin verificar
  // se acepta acá mismo y se reintenta, sin salir del diálogo.
  let testing = $state(false);
  let testResult = $state<{ ok: boolean; message: string } | null>(null);
  let testTrust = $state<{ fingerprint: string; changed: boolean } | null>(null);

  const SSL_MODES: { value: SslMode; label: string }[] = [
    { value: "disable", label: "Sin cifrado" },
    { value: "prefer", label: "Preferir cifrado" },
    { value: "require", label: "Exigir cifrado (sin validar)" },
    { value: "verifyCa", label: "Validar la cadena del certificado" },
    { value: "verifyFull", label: "Validar cadena y nombre del servidor" },
  ];

  async function pickKey() {
    const chosen = await open({ title: "Elegir la clave privada SSH" });
    if (typeof chosen === "string" && form.tunnel) form.tunnel.privateKey = chosen;
  }

  async function testTunnel(trustHostKey = false) {
    testResult = null;
    testTrust = null;
    testing = true;
    try {
      await sshTest($state.snapshot(form), sshPassword || undefined, trustHostKey);
      testResult = { ok: true, message: "El túnel funciona: el bastión alcanza el servidor." };
    } catch (e) {
      const host = sshHostKey(e);
      if (host) {
        testTrust = { fingerprint: host.fingerprint, changed: host.changed };
      } else {
        testResult = { ok: false, message: describeError(e) };
      }
    } finally {
      testing = false;
    }
  }

  async function submit(connect: boolean) {
    error = null;
    if (!form.name.trim()) {
      error = "Poné un nombre para identificar el servidor.";
      return;
    }
    if (form.tunnel && !form.tunnel.host.trim()) {
      error = "El túnel SSH necesita el host del bastión, o desactivalo.";
      return;
    }

    saving = true;
    try {
      const saved = await saveProfile(
        $state.snapshot(form),
        password || undefined,
        sshPassword || undefined,
      );
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

  <div class="divider-t my-4"></div>

  <label class="check">
    <input type="checkbox" bind:checked={tunnelOn} />
    Conectar a través de un túnel SSH (bastión)
  </label>

  {#if form.tunnel}
    <div class="mt-3 grid grid-cols-2 gap-3">
      <label class="flex flex-col gap-1">
        <span class="label">Bastión</span>
        <input class="field" bind:value={form.tunnel.host} placeholder="bastion.interno" />
      </label>

      <label class="flex flex-col gap-1">
        <span class="label">Puerto SSH</span>
        <input class="field" type="number" bind:value={form.tunnel.port} />
      </label>

      <label class="flex flex-col gap-1">
        <span class="label">Usuario SSH</span>
        <input class="field" bind:value={form.tunnel.user} placeholder="deploy" />
      </label>

      <label class="flex flex-col gap-1">
        <span class="label">Clave privada</span>
        <div class="flex gap-2">
          <input
            class="field grow"
            bind:value={form.tunnel.privateKey}
            placeholder="Vacío: autenticar por contraseña"
          />
          <button type="button" class="btn btn-sm" onclick={pickKey}>Elegir…</button>
        </div>
      </label>

      <label class="col-span-2 flex flex-col gap-1">
        <span class="label">
          {form.tunnel.privateKey ? "Frase de la clave (si tiene)" : "Contraseña del bastión"}
        </span>
        <input class="field" type="password" bind:value={sshPassword} autocomplete="off" />
      </label>

      <!--
        La huella del certificado del servidor real no coincide con 127.0.0.1: por un túnel, «validar
        cadena y nombre» valida solo la cadena, como «validar la cadena». Se avisa para no prometer de
        más.
      -->
      {#if form.sslMode === "verifyFull"}
        <p class="col-span-2 text-xs muted">
          Por un túnel, «validar cadena y nombre del servidor» valida solo la cadena: la conexión
          termina en un puerto local y el nombre nunca coincide.
        </p>
      {/if}

      <div class="col-span-2 flex items-center gap-3">
        <button type="button" class="btn btn-sm" onclick={() => testTunnel()} disabled={testing}>
          {#if testing}<span class="spinner"></span>{/if}
          Probar túnel
        </button>
        {#if testResult}
          <span class="text-xs {testResult.ok ? 'text-emerald-600 dark:text-emerald-400' : 'muted'}">
            {testResult.message}
          </span>
        {/if}
      </div>

      {#if testTrust}
        <Alert tone={testTrust.changed ? "bad" : "warn"} box class="col-span-2">
          {testTrust.changed
            ? "La clave del bastión cambió respecto de la registrada."
            : "El bastión no está en tu known_hosts."}
          Huella SHA256: {testTrust.fingerprint}.
          <button type="button" class="btn btn-sm mt-2" onclick={() => testTunnel(true)}>
            Confiar y volver a probar
          </button>
        </Alert>
      {/if}
    </div>
  {/if}

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
