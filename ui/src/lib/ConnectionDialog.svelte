<script lang="ts">
  import { untrack } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import Alert from "./Alert.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import { serverColorLook } from "./badges";
  import { explorer, groupStartsWith } from "./explorer.svelte";
  import {
    describeError,
    saveProfile,
    sshHostKey,
    sshTest,
    type ConnectionProfile,
    type Environment,
    type ServerColor,
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
      readOnly: false,
      autocommit: true,
      // Un servidor nuevo creado desde una ventana de workspace nace en su carpeta: guardado sin
      // carpeta, o en una que cuelga de otra rama, no aparecería en el árbol de esta ventana y se
      // vería como si el guardado hubiera fallado. Sigue siendo el valor de partida nada más — el
      // campo de abajo lo deja cambiar.
      group: explorer.workspace?.rootGroup,
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

  /** La carpeta elegida no cuelga del `rootGroup` de esta ventana: se guarda igual, solo se avisa. */
  const outOfWorkspaceScope = $derived.by(() => {
    const root = explorer.workspace?.rootGroup;
    return !!root && !!form.group && !groupStartsWith(form.group, root);
  });

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

  // El formulario se partió en pestañas cuando dejó de entrar de un vistazo: lo de todos los días
  // arriba, lo que se toca una vez por servidor detrás. La validación tiene que poder traer al
  // usuario de vuelta a la pestaña del campo que falla, o el error señala algo que no está en pantalla.
  type Section = "general" | "connection" | "ssh";
  const SECTIONS: { value: Section; label: string }[] = [
    { value: "general", label: "General" },
    { value: "connection", label: "Conexión" },
    { value: "ssh", label: "SSH" },
  ];
  let section = $state<Section>("general");

  const ENVIRONMENTS: { value: Environment | ""; label: string }[] = [
    { value: "", label: "Sin marcar" },
    { value: "dev", label: "Desarrollo" },
    { value: "test", label: "Pruebas" },
    { value: "prod", label: "Producción" },
  ];

  const SERVER_COLORS: ServerColor[] = [
    "red",
    "orange",
    "amber",
    "green",
    "teal",
    "blue",
    "purple",
    "pink",
  ];

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

  async function pickRootCert() {
    const chosen = await open({ title: "Elegir el certificado raíz" });
    if (typeof chosen === "string") form.rootCert = chosen;
  }

  /** Vacío en el formulario es «sin límite»: el perfil lo guarda como campo ausente. */
  function optionalNumber(value: string): number | undefined {
    return value.trim() === "" ? undefined : Number(value);
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
      section = "general";
      error = "Poné un nombre para identificar el servidor.";
      return;
    }
    if (form.tunnel && !form.tunnel.host.trim()) {
      section = "ssh";
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
  <div class="seg mb-4" role="tablist">
    {#each SECTIONS as item (item.value)}
      <button
        class="seg-item"
        role="tab"
        aria-selected={section === item.value}
        onclick={() => (section = item.value)}
      >
        {item.label}
      </button>
    {/each}
  </div>

  <div class="grid grid-cols-2 gap-3" class:hidden={section !== "general"}>
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
        title="Agrupa este servidor con los demás que tengan el mismo nombre de carpeta. Una barra anida: «Clientes/ACME» se dibuja adentro de «Clientes»"
        bind:value={form.group}
      />
      <datalist id="carpetas-de-conexiones">
        {#each explorer.groups as group (group)}
          <option value={group}></option>
        {/each}
      </datalist>
      {#if outOfWorkspaceScope}
        <!-- No bloquea: el usuario puede haberlo elegido a propósito. Guardarlo bien y no verlo en
             esta ventana se parecía a que el guardado hubiera fallado, sin ningún aviso. -->
        <Alert tone="warn" class="mt-1">
          Esto no va a verse en esta ventana: queda fuera de «{explorer.workspace?.rootGroup}».
        </Alert>
      {/if}
    </label>

    <!--
      Independiente del entorno: el color no tiene significado fijo, solo distingue servidores a
      simple vista en el árbol. Paleta cerrada — nada de selector libre — para no tener que validar
      ni pensar en contraste contra el tema oscuro.
    -->
    <label class="col-span-2 flex flex-col gap-1">
      <span class="label">Color</span>
      <div class="flex items-center gap-1.5">
        <button
          type="button"
          class="grid size-6 shrink-0 place-items-center rounded-full border border-dashed
                 border-zinc-400 text-[10px] text-zinc-400 dark:border-zinc-500 dark:text-zinc-500
                 {form.color === undefined ? 'ring-2 ring-blue-500 ring-offset-1' : ''}"
          title="Sin color"
          onclick={() => (form.color = undefined)}
        >
          <Icon name="close" size={11} />
        </button>
        {#each SERVER_COLORS as color (color)}
          <button
            type="button"
            class="size-6 shrink-0 rounded-full {serverColorLook(color).dot}
                   {form.color === color ? 'ring-2 ring-blue-500 ring-offset-1' : ''}"
            title={serverColorLook(color).label}
            aria-label={serverColorLook(color).label}
            onclick={() => (form.color = color)}
          ></button>
        {/each}
      </div>
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

  <div class="grid grid-cols-2 gap-3" class:hidden={section !== "connection"}>
    <label class="flex flex-col gap-1">
      <span class="label">Entorno</span>
      <select
        class="field"
        title="Solo cambia cómo se ve y cuánto se pregunta antes de modificar algo"
        value={form.environment ?? ""}
        onchange={(event) =>
          (form.environment = (event.currentTarget.value || undefined) as Environment | undefined)}
      >
        {#each ENVIRONMENTS as item (item.value)}
          <option value={item.value}>{item.label}</option>
        {/each}
      </select>
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Límite por sentencia (ms)</span>
      <input
        class="field"
        type="number"
        min="0"
        placeholder="El del servidor"
        value={form.statementTimeoutMs ?? ""}
        onchange={(event) => (form.statementTimeoutMs = optionalNumber(event.currentTarget.value))}
      />
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Espera al conectar (s)</span>
      <input class="field" type="number" min="1" bind:value={form.connectTimeoutSecs} />
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Certificado raíz</span>
      <div class="flex gap-2">
        <input
          class="field grow"
          bind:value={form.rootCert}
          placeholder="Vacío: las CA del sistema"
        />
        <button type="button" class="btn btn-sm" onclick={pickRootCert}>Elegir…</button>
      </div>
    </label>

    <label class="check col-span-2">
      <input type="checkbox" bind:checked={form.readOnly} />
      Conexión de solo lectura
    </label>
    <p class="col-span-2 -mt-2 text-xs muted">
      Abre cada conexión con <code>default_transaction_read_only</code>: el rechazo lo hace el
      servidor, así vale igual para el explorador, el editor de SQL y las importaciones.
    </p>

    <label class="check col-span-2">
      <input type="checkbox" bind:checked={form.autocommit} />
      Autocommit en las pestañas de consulta
    </label>
    <p class="col-span-2 -mt-2 text-xs muted">
      Apagado, cada ejecución abre una transacción que hay que confirmar con Commit. Es el valor
      inicial: cada pestaña lo puede cambiar por su cuenta.
    </p>
  </div>

  <div class:hidden={section !== "ssh"}>
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
        Por un túnel el nombre validado no es aquel al que conecta el cliente —un puerto local— sino
        el del campo «Servidor». Se avisa porque de ahí sale el único error posible de esta
        combinación: un certificado emitido para otro nombre, o para una IP sin SAN de IP.
      -->
      {#if form.sslMode === "verifyFull"}
        <p class="col-span-2 text-xs muted">
          Por un túnel, el nombre se valida contra «{form.host || "el servidor"}»: el certificado del
          servidor tiene que estar emitido para ese nombre, no para el puerto local del túnel.
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
