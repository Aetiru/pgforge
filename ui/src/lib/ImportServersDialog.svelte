<script lang="ts">
  import Alert from "./Alert.svelte";
  import Empty from "./Empty.svelte";
  import Modal from "./Modal.svelte";
  import { explorer } from "./explorer.svelte";
  import {
    describeError,
    importApply,
    importScan,
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
   */
  let group = $state("");
  /** Usuario para los que llegan sin uno. DBeaver lo guarda cifrado junto con la contraseña. */
  let user = $state("");
  let loading = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);

  const key = (candidate: ImportCandidate) =>
    `${candidate.origin}/${candidate.host}:${candidate.port}/${candidate.database}/${candidate.user}`;

  /** Los que ya están cargados no se ofrecen: importar dos veces el mismo servidor es ruido. */
  const known = $derived(
    new Set(
      explorer.profiles.map(
        (profile) => `${profile.host}:${profile.port}/${profile.database}/${profile.user}`,
      ),
    ),
  );

  const nuevos = $derived(
    candidates.filter(
      (candidate) =>
        !known.has(`${candidate.host}:${candidate.port}/${candidate.database}/${candidate.user}`),
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

  function toggle(candidate: ImportCandidate, checked: boolean) {
    const next = new Set(chosen);
    if (checked) next.add(key(candidate));
    else next.delete(key(candidate));
    chosen = next;
  }

  const sinUsuario = $derived(
    nuevos.filter((candidate) => chosen.has(key(candidate)) && !candidate.user).length,
  );

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
