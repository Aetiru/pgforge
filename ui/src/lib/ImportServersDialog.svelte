<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import Alert from "./Alert.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import { explorer, groupStartsWith } from "./explorer.svelte";
  import {
    describeError,
    importApply,
    importScan,
    importScanWorkspace,
    type ImportCandidate,
  } from "./ipc";

  /**
   * Traer servidores que ya están configurados en otra herramienta.
   *
   * La primera pared de una herramienta nueva es volver a cargar a mano los veinte servidores que uno
   * ya tiene anotados en otro lado. Acá se muestran los que hay en los archivos de `libpq` y en
   * DBeaver, y el usuario elige cuáles.
   *
   * Se ven antes de guardarlos, y no se guarda ninguna contraseña: lo que llega es a qué servidor
   * conectarse (ver `conn::import`).
   */
  let { onclose, onimported }: { onclose: () => void; onimported: () => void } = $props();

  const ORIGIN_LABEL: Record<ImportCandidate["origin"], string> = {
    pgpass: ".pgpass",
    service: "pg_service.conf",
    dbeaver: "DBeaver",
  };

  let candidates = $state<ImportCandidate[]>([]);
  let chosen = $state<Set<string>>(new Set());
  /**
   * Carpeta para todos. Vacía deja la que cada servidor tenía en la otra herramienta —DBeaver las
   * usa igual que pgforge—, que casi siempre es la que uno quiere.
   *
   * En una ventana de workspace arranca con la carpeta raíz de esta ventana: sin eso, un candidato
   * importado sin carpeta —o con una que no cuelga de `rootGroup`— se guarda bien pero no aparece
   * en el árbol de esta ventana, como si la importación hubiera fallado. Sigue siendo editable.
   */
  let group = $state(explorer.workspace?.rootGroup ?? "");
  /** Usuario para los que llegan sin uno. DBeaver lo guarda cifrado junto con la contraseña. */
  let user = $state("");
  let loading = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);

  // `group` entra en la clave: `import_scan_workspace` (backend) a propósito no fusiona candidatos
  // con mismo host/puerto/usuario/base pero de proyectos DBeaver distintos —DBeaver deja `user` vacío
  // seguido, así que ese choque es común—, y sin `group` acá dos candidatos así comparten clave: el
  // `{#each … (key(candidate))}` de abajo revienta con `each_key_duplicate`, y aunque no reventara,
  // comparten tilde y el segundo "desaparece" de la lista apenas se tilda el primero. `?? ""` porque
  // los candidatos de `.pgpass`/`.pg_service.conf` no traen carpeta.
  const key = (candidate: ImportCandidate) =>
    `${candidate.origin}/${candidate.host}:${candidate.port}/${candidate.database}/${candidate.user}/${candidate.group ?? ""}`;

  /** Los que ya están cargados no se ofrecen: importar dos veces el mismo servidor es ruido. */
  const known = $derived(
    new Set(
      explorer.profiles.map(
        (profile) =>
          `${profile.host}:${profile.port}/${profile.database}/${profile.user}/${profile.group ?? ""}`,
      ),
    ),
  );

  const nuevos = $derived(
    candidates.filter(
      (candidate) =>
        !known.has(
          `${candidate.host}:${candidate.port}/${candidate.database}/${candidate.user}/${candidate.group ?? ""}`,
        ),
    ),
  );

  $effect(() => {
    importScan()
      .then((result) => {
        candidates = result;
        // Todo tildado: el que abrió este diálogo quiere importar, y destildar lo que no va es menos
        // trabajo que tildar de a uno.
        chosen = new Set(result.map(key));
      })
      .catch((e) => (error = describeError(e)))
      .finally(() => (loading = false));
  });

  /**
   * Alternativa al escaneo automático de arriba: en vez de mirar el `data-sources.json` de un solo
   * DBeaver instalado, escanea la carpeta raíz de un **workspace** entero —todos sus proyectos—, con
   * `group` ya prefijado por proyecto (ver `import_scan_workspace`). Reemplaza la lista de candidatos
   * en vez de sumarse a ella: es otra fuente, no un agregado a la que ya se trajo.
   */
  async function chooseDbeaverWorkspace() {
    const root = await open({ directory: true, title: "Carpeta raíz del workspace de DBeaver" });
    if (typeof root !== "string") return;

    loading = true;
    error = null;
    try {
      const result = await importScanWorkspace(root);
      candidates = result;
      chosen = new Set(result.map(key));
    } catch (e) {
      error = describeError(e);
    } finally {
      loading = false;
    }
  }

  function toggle(candidate: ImportCandidate, checked: boolean) {
    const next = new Set(chosen);
    if (checked) next.add(key(candidate));
    else next.delete(key(candidate));
    chosen = next;
  }

  const sinUsuario = $derived(
    nuevos.filter((candidate) => chosen.has(key(candidate)) && !candidate.user).length,
  );

  /**
   * La carpeta con la que van a quedar los elegidos no cuelga del `rootGroup` de esta ventana: no
   * bloquea —puede ser a propósito—, pero sin este aviso un candidato guardado bien y fuera de
   * alcance se ve igual que uno que no se guardó.
   */
  const outOfWorkspaceScope = $derived.by(() => {
    const root = explorer.workspace?.rootGroup;
    if (!root) return false;
    const trimmed = group.trim();
    if (trimmed) return !groupStartsWith(trimmed, root);
    // Sin «carpeta para todos», cada candidato se guarda con la que traía —o suelto—: avisa si
    // alguno de los elegidos queda fuera.
    return nuevos.some(
      (candidate) =>
        chosen.has(key(candidate)) && candidate.group && !groupStartsWith(candidate.group, root),
    );
  });

  async function submit() {
    const elegidos = nuevos
      .filter((candidate) => chosen.has(key(candidate)))
      // El usuario escrito arriba solo completa a los que llegaron sin ninguno: el que trae el suyo
      // lo conserva.
      .map((candidate) => (candidate.user ? candidate : { ...candidate, user: user.trim() }));
    if (elegidos.length === 0) {
      onclose();
      return;
    }

    saving = true;
    error = null;
    try {
      await importApply(elegidos, group.trim() || undefined);
      await explorer.refreshProfiles();
      onimported();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title="Importar servidores"
  subtitle="De los archivos de libpq y de DBeaver"
  size="lg"
  busy={saving}
  {onclose}
>
  <!-- El escaneo automático de arriba mira un solo DBeaver instalado; esto apunta a la carpeta raíz
       de un workspace entero, con todos sus proyectos, y reemplaza la lista con lo que encuentre. -->
  <div class="mb-3 flex justify-end">
    <button
      class="btn btn-ghost btn-sm"
      disabled={loading || saving}
      onclick={chooseDbeaverWorkspace}
    >
      <Icon name="folder" size={12} />
      Elegir carpeta de workspace de DBeaver…
    </button>
  </div>

  {#if loading}
    <p class="flex items-center gap-2 text-sm muted"><span class="spinner"></span> Buscando…</p>
  {:else if nuevos.length === 0}
    <Empty
      icon="server"
      title={candidates.length === 0
        ? "No se encontró ningún servidor configurado"
        : "Todos los que se encontraron ya están cargados"}
      hint="Se miran ~/.pgpass, ~/.pg_service.conf y el data-sources.json de DBeaver."
    />
  {:else}
    <div class="rounded-md border border-zinc-200 dark:border-zinc-700">
      <div class="max-h-72 overflow-auto px-2 py-1.5">
        {#each nuevos as candidate (key(candidate))}
          <label class="check py-1">
            <input
              type="checkbox"
              checked={chosen.has(key(candidate))}
              onchange={(event) => toggle(candidate, event.currentTarget.checked)}
            />
            <span class="min-w-0 flex-1 truncate">
              {candidate.name}
              <span class="muted">
                · {candidate.user || user.trim() || "sin usuario"}@{candidate.host}:{candidate.port}/{candidate.database}
              </span>
            </span>
            {#if candidate.environment === "prod"}
              <span class="tag tag-bad shrink-0">producción</span>
            {/if}
            {#if candidate.group}
              <span class="tag tag-neutral shrink-0" title="Carpeta en la otra herramienta">
                {candidate.group}
              </span>
            {/if}
            <span class="tag tag-neutral shrink-0" title={candidate.source}>
              {ORIGIN_LABEL[candidate.origin]}
            </span>
          </label>
        {/each}
      </div>
    </div>

    <div class="mt-3 grid grid-cols-2 gap-3">
      <label class="flex flex-col gap-1">
        <span class="label">Carpeta para todos</span>
        <input class="field" bind:value={group} placeholder="la que ya tenían" />
      </label>
      <label class="flex flex-col gap-1">
        <span class="label">
          Usuario {sinUsuario > 0 ? `(${sinUsuario} sin uno)` : "(solo si falta)"}
        </span>
        <input class="field" bind:value={user} data-autofocus placeholder="postgres" />
      </label>
    </div>

    <p class="mt-2 text-xs muted">
      No se importa ninguna contraseña, ni siquiera las que están en texto plano en «.pgpass»: se
      piden al conectar y se guardan en el almacén del sistema solo si se pide recordarlas.
    </p>

    {#if outOfWorkspaceScope}
      <Alert tone="warn" class="mt-2">
        Esto no va a verse en esta ventana: queda fuera de «{explorer.workspace?.rootGroup}».
      </Alert>
    {/if}
  {/if}

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
    <button class="btn btn-primary" onclick={submit} disabled={saving || nuevos.length === 0}>
      {#if saving}<span class="spinner"></span>{/if}
      Importar {chosen.size > 0 ? `(${nuevos.filter((c) => chosen.has(key(c))).length})` : ""}
    </button>
  {/snippet}
</Modal>
