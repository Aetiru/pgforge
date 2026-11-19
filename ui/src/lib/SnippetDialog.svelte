<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Confirm from "./Confirm.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import ShortcutField from "./ShortcutField.svelte";
  import SnippetBodyEditor from "./SnippetBodyEditor.svelte";
  import SnippetLibraryDialog from "./SnippetLibraryDialog.svelte";
  import SnippetLiteralsDialog from "./SnippetLiteralsDialog.svelte";
  import Sql from "./Sql.svelte";
  import { explorer } from "./explorer.svelte";
  import { pickOpen, pickSave, type ImportMode, type ImportReport, type Snippet } from "./ipc";
  import { checkAbbreviation, checkShortcut } from "./snippet-check";
  import {
    askedVariables,
    autoNames,
    duplicateOf,
    emptySnippet,
    formToScope,
    hereOnly,
    importSummary,
    loadFolded,
    saveFolded,
    scopeToForm,
    usageText,
    type ScopeForm,
  } from "./snippet-dialog";
  import {
    NO_CATEGORY,
    filterSnippets,
    groupByCategory,
    scopeLabel,
    sortSnippets,
    unusedIds,
    type SnippetOrder,
  } from "./snippet-list";
  import { findLiterals } from "./snippet-literals";
  import { previewBody } from "./snippet-vars";
  import { SHORTCUTS, formatCombo } from "./shortcuts";
  import { MAC, shortcutStore } from "./shortcuts.svelte";
  import { snippets } from "./snippets.svelte";
  import { preview } from "./sql-snippet";

  /**
   * Las abreviaturas del editor.
   *
   * Se abre desde la barra de la pestaña de consulta y no desde una pantalla de preferencias
   * general: uno se acuerda de que quiere una abreviatura mientras escribe la consulta que la
   * pediría, igual que el umbral de los avisos se configura en la vista de procesos.
   *
   * Guardar valida del lado de Rust —la abreviatura única, sin espacios, con cuerpo, el atajo
   * único— porque es el que tiene la lista entera; los avisos de acá (`snippet-check`) llegan antes
   * y solo adelantan lo que el núcleo iba a decir, más lo que él no prohíbe pero sorprende después.
   *
   * Los diálogos de la biblioteca y de los literales **reemplazan** a éste mientras están abiertos
   * en vez de apilarse encima: dos modales anidados se pisan el foco y el Escape.
   */
  let {
    onclose,
    draft = undefined,
    profileId = null,
    database = null,
  }: {
    onclose: () => void;
    /** Abre directo el formulario de una nueva con este texto (p. ej. la selección del editor). */
    draft?: { body: string; abbreviation?: string; convertLiterals?: boolean };
    /** El servidor y la base de la pestaña desde la que se abrió: para «Solo aquí» y para el ejemplo. */
    profileId?: string | null;
    database?: string | null;
  } = $props();

  /** La que se está editando, o `null` mientras se mira la lista. */
  let editing = $state<Snippet | null>(
    untrack(() =>
      draft ? { ...emptySnippet(), body: draft.body, abbreviation: draft.abbreviation ?? "" } : null,
    ),
  );
  let scopeForm = $state<ScopeForm>({ only: false, profileIds: [], databasesText: "" });
  let busy = $state(false);
  let confirmingReset = $state(false);
  let confirmingDelete = $state(false);
  let showLibrary = $state(false);
  /** Mientras es `true`, el diálogo de literales va en lugar de éste. */
  let converting = $state(
    untrack(() => !!draft?.convertLiterals && findLiterals(draft.body).length > 0),
  );
  /** Cuando se abre de la lista con el formulario ya en pantalla, «Dejar como está» vuelve a él. */
  let notice = $state<string | null>(null);

  // --- Lista ---------------------------------------------------------------------------------
  let query = $state("");
  let order = $state<SnippetOrder>("name");
  let folded = $state<Set<string>>(loadFolded());
  let marked = $state<string[]>([]);
  const nowSeconds = Date.now() / 1000;

  const profileName = (id: string) => explorer.profiles.find((p) => p.id === id)?.name ?? null;

  const groups = $derived(
    groupByCategory(sortSnippets(filterSnippets(snippets.items, query), order, snippets.usage)),
  );
  /** Con una sola carpeta —o ninguna— los rótulos de grupo sobran. */
  const showGroups = $derived(groups.length > 1 || (groups[0] && groups[0].category !== NO_CATEGORY));

  // Sin ningún uso registrado todas figurarían «sin usar», y avisarlo en una instalación nueva es
  // ruido: el aviso espera a que el contador tenga algo que decir.
  const hasUsage = $derived(Object.values(snippets.usage).some((info) => info.uses > 0));
  const unused = $derived(hasUsage ? unusedIds(snippets.items, snippets.usage, 60, nowSeconds) : []);
  const categories = $derived([
    ...new Set(snippets.items.map((item) => item.category).filter((name) => name !== "")),
  ]);

  function toggleFolded(category: string) {
    const next = new Set(folded);
    if (next.has(category)) next.delete(category);
    else next.add(category);
    folded = next;
    saveFolded(next);
  }

  function toggleMark(id: string) {
    marked = marked.includes(id) ? marked.filter((item) => item !== id) : [...marked, id];
  }

  // --- Deshacer el borrado ---------------------------------------------------------------------
  let undo = $state<Snippet | null>(null);
  let undoTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => () => clearTimeout(undoTimer));

  async function remove(item: Snippet) {
    busy = true;
    const removed = await snippets.removeWithUndo(item.id!);
    busy = false;
    if (!removed) return;
    marked = marked.filter((id) => id !== item.id);
    undo = removed;
    clearTimeout(undoTimer);
    undoTimer = setTimeout(() => (undo = null), 8000);
  }

  async function undoRemove() {
    if (!undo) return;
    const back = undo;
    undo = null;
    clearTimeout(undoTimer);
    busy = true;
    await snippets.save(back);
    busy = false;
  }

  async function removeMarked() {
    confirmingDelete = false;
    busy = true;
    for (const id of marked) await snippets.remove(id);
    busy = false;
    marked = [];
  }

  // --- Formulario ------------------------------------------------------------------------------
  const builtins = $derived(
    SHORTCUTS.map((def) => ({ id: def.id, label: def.label, combo: shortcutStore.combo(def.id) })),
  );
  const abbreviationCheck = $derived(
    editing
      ? checkAbbreviation(editing.abbreviation, snippets.items, editing.id ?? null)
      : { errors: [], warnings: [] },
  );
  const shortcutCheck = $derived(
    editing
      ? checkShortcut(editing.shortcut, snippets.items, editing.id ?? null, builtins)
      : { errors: [], warnings: [] },
  );
  const canSave = $derived(
    !!editing &&
      editing.body.trim() !== "" &&
      editing.abbreviation.trim() !== "" &&
      abbreviationCheck.errors.length === 0 &&
      shortcutCheck.errors.length === 0,
  );
  const asked = $derived(editing ? askedVariables(editing.body) : []);
  const automatic = $derived(editing ? autoNames(editing.body) : []);
  const shown = $derived(
    editing
      ? previewBody(editing.body, {
          ...(database ? { base: database } : {}),
          ...(profileId && profileName(profileId) ? { servidor: profileName(profileId)! } : {}),
        })
      : "",
  );
  const hasLiterals = $derived(editing ? findLiterals(editing.body).length > 0 : false);

  function edit(item: Snippet) {
    editing = { ...item, scope: { ...item.scope } };
    scopeForm = scopeToForm(item.scope);
  }

  function create() {
    editing = emptySnippet();
    scopeForm = scopeToForm(editing.scope);
  }

  function duplicate(item: Snippet) {
    editing = duplicateOf(
      item,
      snippets.items.map((s) => s.abbreviation),
    );
    scopeForm = scopeToForm(editing.scope);
  }

  async function submit() {
    if (!editing || !canSave) return;
    busy = true;
    const ok = await snippets.save({
      ...editing,
      abbreviation: editing.abbreviation.trim(),
      category: editing.category.trim(),
      scope: formToScope(scopeForm),
    });
    busy = false;
    if (ok) editing = null;
  }

  function toggleProfile(id: string) {
    scopeForm.profileIds = scopeForm.profileIds.includes(id)
      ? scopeForm.profileIds.filter((item) => item !== id)
      : [...scopeForm.profileIds, id];
  }

  // --- Restablecer, exportar e importar ----------------------------------------------------------
  async function reset() {
    confirmingReset = false;
    busy = true;
    await snippets.reset();
    busy = false;
    marked = [];
  }

  async function exportSnippets() {
    notice = null;
    // La ruta sale de un diálogo abierto desde Rust: es lo único que `snippets_export` acepta.
    const path = await pickSave({
      title: "Exportar abreviaturas",
      defaultPath: "abreviaturas.json",
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (!path) return;
    busy = true;
    const count = await snippets.exportTo(path, marked.length > 0 ? marked : null);
    busy = false;
    if (count !== null) notice = `Se exportaron ${count} ${count === 1 ? "abreviatura" : "abreviaturas"}.`;
  }

  /** El archivo elegido y qué hacer con las que chocan; mientras es `null` no hay importación en curso. */
  let importing = $state<{ path: string; mode: ImportMode } | null>(null);
  let imported = $state<ImportReport | null>(null);

  async function pickImport() {
    notice = null;
    imported = null;
    const path = await pickOpen({
      title: "Importar abreviaturas",
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (path) importing = { path, mode: "skip" };
  }

  async function runImport() {
    if (!importing) return;
    busy = true;
    const report = await snippets.importFrom(importing.path, importing.mode);
    busy = false;
    importing = null;
    if (report) imported = report;
  }
</script>

{#if showLibrary}
  <SnippetLibraryDialog onclose={() => (showLibrary = false)} />
{:else if converting && editing}
  <SnippetLiteralsDialog
    body={editing.body}
    onapply={(body) => {
      if (editing) editing.body = body;
      converting = false;
    }}
    onskip={() => (converting = false)}
  />
{:else}
  <Modal
    title="Abreviaturas"
    subtitle="Escribí la abreviatura en el editor y apretá Tab para expandirla."
    size="xl"
    {busy}
    {onclose}
  >
    {#if snippets.error}
      <Alert tone="bad" box class="mb-3" onclose={() => (snippets.error = null)}>
        {snippets.error}
      </Alert>
    {/if}
    {#if notice}
      <Alert tone="ok" box class="mb-3" onclose={() => (notice = null)}>{notice}</Alert>
    {/if}

    {#if editing}
      <div class="space-y-3">
        <div class="grid grid-cols-[160px_1fr_200px] gap-3">
          <label class="field">
            <span class="label">Abreviatura</span>
            <input
              class="w-full"
              data-autofocus
              bind:value={editing.abbreviation}
              placeholder="sf"
              spellcheck="false"
            />
          </label>
          <label class="field">
            <span class="label">Para qué es</span>
            <input class="w-full" bind:value={editing.description} placeholder="Consulta base" />
          </label>
          <label class="field">
            <span class="label">Categoría</span>
            <input
              class="w-full"
              list="pgforge-snippet-categories"
              bind:value={editing.category}
              placeholder="Consultas"
              maxlength="40"
            />
            <datalist id="pgforge-snippet-categories">
              {#each categories as name (name)}<option value={name}></option>{/each}
            </datalist>
          </label>
        </div>

        {#if editing.abbreviation !== ""}
          {#each abbreviationCheck.errors as message (message)}
            <p class="text-xs text-rose-600 dark:text-rose-400">{message}</p>
          {/each}
          {#each abbreviationCheck.warnings as message (message)}
            <p class="text-xs text-amber-600 dark:text-amber-400">{message}</p>
          {/each}
        {/if}

        <div class="field">
          <div class="mb-1 flex items-center gap-2">
            <span class="label mb-0">Texto</span>
            {#if hasLiterals}
              <button
                class="btn btn-ghost btn-sm ml-auto"
                title="Reemplaza los valores fijos —fechas, números, textos— por variables que se preguntan al expandir"
                onclick={() => (converting = true)}
              >
                Convertir valores en variables…
              </button>
            {/if}
          </div>
          <SnippetBodyEditor bind:value={editing.body} placeholder={"SELECT ${*}\nFROM ${tabla}\nWHERE ${}"} />
        </div>

        <div class="grid grid-cols-2 gap-3">
          <div class="min-w-0">
            <p class="label mb-1">Así se inserta (con los valores por omisión)</p>
            <div
              class="max-h-44 min-h-12 overflow-auto rounded border border-zinc-200 p-2 dark:border-zinc-700"
            >
              {#if editing.body.trim() === ""}
                <span class="text-xs muted">Escribí el texto para ver cómo queda.</span>
              {:else}
                <Sql code={shown} />
              {/if}
            </div>
          </div>
          <div class="min-w-0 space-y-2 text-xs">
            <div>
              <p class="label mb-1">Variables que preguntará</p>
              {#if asked.length === 0}
                <p class="muted">Ninguna: se inserta de una.</p>
              {:else}
                <ul class="space-y-0.5">
                  {#each asked as variable (variable.name)}
                    <li>
                      <span class="font-mono">{variable.name}</span>
                      <span class="muted">· {variable.kind}</span>
                      {#if variable.detail}<span class="muted"> · {variable.detail}</span>{/if}
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
            {#if automatic.length > 0}
              <div>
                <p class="label mb-1">Se llenan solas</p>
                <p class="font-mono">{automatic.join(", ")}</p>
              </div>
            {/if}
          </div>
        </div>

        <div class="grid grid-cols-2 gap-3">
          <div class="field">
            <span class="label">Dónde vale</span>
            <div class="seg" role="tablist">
              <button
                class="seg-item"
                role="tab"
                aria-selected={!scopeForm.only}
                onclick={() => (scopeForm.only = false)}>Todos los servidores</button
              >
              <button
                class="seg-item"
                role="tab"
                aria-selected={scopeForm.only}
                onclick={() => (scopeForm.only = true)}>Solo estos…</button
              >
            </div>
            {#if scopeForm.only}
              <div class="mt-2 space-y-2">
                <div class="max-h-28 space-y-1 overflow-y-auto">
                  {#each explorer.profiles as profile (profile.id)}
                    <label class="check text-xs">
                      <input
                        type="checkbox"
                        checked={scopeForm.profileIds.includes(profile.id)}
                        onchange={() => toggleProfile(profile.id)}
                      />
                      {profile.name}
                    </label>
                  {/each}
                </div>
                <label class="field">
                  <span class="label">Bases (separadas por coma; vacío = todas)</span>
                  <input
                    class="w-full"
                    bind:value={scopeForm.databasesText}
                    placeholder="ventas, rrhh"
                    spellcheck="false"
                  />
                </label>
              </div>
            {/if}
            {#if profileId || database}
              <button
                class="btn btn-ghost btn-sm mt-1 self-start"
                onclick={() => (scopeForm = hereOnly(profileId, database))}
              >
                Solo aquí{database ? ` (${database})` : ""}
              </button>
            {/if}
          </div>
          <div class="field">
            <span class="label">Atajo de teclado</span>
            <ShortcutField bind:value={editing.shortcut} />
            {#each shortcutCheck.errors as message (message)}
              <p class="mt-1 text-xs text-rose-600 dark:text-rose-400">{message}</p>
            {/each}
            {#each shortcutCheck.warnings as message (message)}
              <p class="mt-1 text-xs text-amber-600 dark:text-amber-400">{message}</p>
            {/each}
            <p class="mt-1 text-xs muted">Expande la abreviatura de un golpe, sin escribir su nombre.</p>
          </div>
        </div>

        <p class="text-xs muted">
          Lo que va entre <code>{"${}"}</code> es un hueco: al expandir queda seleccionado y con Tab se
          salta al siguiente. <code>{"${nombre:valor}"}</code> es una variable con valor por omisión;
          <code>{"${estado|activo|baja}"}</code>, una lista;
          <code>{"${t@tabla}"}</code>, <code>{"${c@columna}"}</code>, <code>{"${d@fecha}"}</code> y
          <code>{"${n@numero}"}</code>, campos tipados. <code>{"${hoy}"}</code>,
          <code>{"${ahora}"}</code>, <code>{"${usuario}"}</code>, <code>{"${base}"}</code>,
          <code>{"${servidor}"}</code>, <code>{"${seleccion}"}</code> y
          <code>{"${portapapeles}"}</code> se llenan solos. Un <code>$1</code> sin llaves es un
          parámetro de PostgreSQL y se inserta tal cual.
        </p>
      </div>
    {:else if importing}
      <div class="space-y-3">
        <p class="text-sm">
          Se importan las abreviaturas de <span class="font-mono break-all">{importing.path}</span>.
        </p>
        <p class="label">Si la abreviatura ya existe</p>
        <div class="seg" role="tablist">
          <button
            class="seg-item"
            role="tab"
            aria-selected={importing.mode === "skip"}
            onclick={() => importing && (importing.mode = "skip")}>Omitirla</button
          >
          <button
            class="seg-item"
            role="tab"
            aria-selected={importing.mode === "replace"}
            onclick={() => importing && (importing.mode = "replace")}>Reemplazarla</button
          >
        </div>
        <p class="text-xs muted">
          {importing.mode === "skip"
            ? "Se conservan las tuyas y se agregan solo las que no existen."
            : "Se pisa el texto de las tuyas con el del archivo. Las que no choquen se agregan."}
        </p>
      </div>
    {:else}
      {#if imported}
        <Alert tone={imported.skipped.length > 0 ? "warn" : "ok"} box class="mb-3" onclose={() => (imported = null)}>
          <strong>{importSummary(imported)}.</strong>
          {#if imported.skipped.length > 0}
            <ul class="mt-1 list-disc pl-5 text-xs">
              {#each imported.skipped as line (line)}<li>{line}</li>{/each}
            </ul>
          {/if}
        </Alert>
      {/if}
      {#if undo}
        <Alert tone="warn" box class="mb-3">
          Se borró «{undo.abbreviation}».
          <button class="btn btn-sm ml-2" onclick={undoRemove}>Deshacer</button>
          <span class="ml-2 text-xs muted">Si cerrás este diálogo ya no se puede deshacer.</span>
        </Alert>
      {/if}
      {#if unused.length > 0 && query === ""}
        <Alert tone="warn" box class="mb-3">
          {unused.length === 1 ? "1 abreviatura sin usar" : `${unused.length} abreviaturas sin usar`} en 60
          días.
          <button class="btn btn-sm ml-2" onclick={() => (marked = unused)}>Seleccionar para borrar</button>
        </Alert>
      {/if}

      {#if snippets.items.length === 0}
        <Empty icon="sql" title="No hay abreviaturas" hint="Creá una, mirá la biblioteca o volvé a las de fábrica." />
      {:else}
        <div class="mb-2 flex items-center gap-2">
          <input
            class="field w-64"
            data-autofocus
            placeholder="Buscar por abreviatura, texto o categoría…"
            aria-label="Buscar abreviaturas"
            bind:value={query}
            onkeydown={(event) => {
              if (event.key === "Escape" && query !== "") {
                event.stopPropagation();
                query = "";
              }
            }}
          />
          <label class="check text-xs">
            Orden
            <select class="field py-0.5 text-xs" bind:value={order}>
              <option value="name">Por nombre</option>
              <option value="uses">Más usadas</option>
              <option value="recent">Recientes</option>
            </select>
          </label>
          {#if marked.length > 0}
            <span class="ml-auto text-xs">{marked.length} marcadas</span>
            <button class="btn btn-sm" onclick={exportSnippets}>Exportar</button>
            <button class="btn btn-sm btn-danger" onclick={() => (confirmingDelete = true)}>Borrar</button>
            <button class="btn btn-sm btn-ghost" onclick={() => (marked = [])}>Quitar selección</button>
          {/if}
        </div>

        <div class="max-h-[50vh] overflow-y-auto">
          <table class="list-table">
            <thead>
              <tr>
                <th class="w-8"></th>
                <th class="w-32">Abreviatura</th>
                <th>Se expande a</th>
                <th class="w-44">Usos</th>
                <th class="w-28"></th>
              </tr>
            </thead>
            <tbody>
              {#each groups as group (group.category)}
                {#if showGroups}
                  <tr>
                    <td colspan="5" class="bg-zinc-50 dark:bg-zinc-800/50">
                      <button
                        class="flex w-full items-center gap-1 text-left text-xs font-semibold uppercase"
                        aria-expanded={!folded.has(group.category)}
                        onclick={() => toggleFolded(group.category)}
                      >
                        <Icon name="chevron" size={11} class={folded.has(group.category) ? "" : "rotate-90"} />
                        {group.category}
                        <span class="font-normal muted">· {group.items.length}</span>
                      </button>
                    </td>
                  </tr>
                {/if}
                {#if !showGroups || !folded.has(group.category)}
                  {#each group.items as item (item.id)}
                    {@const where = scopeLabel(item, profileName)}
                    <tr>
                      <td>
                        <input
                          type="checkbox"
                          aria-label="Marcar «{item.abbreviation}»"
                          checked={marked.includes(item.id!)}
                          onchange={() => toggleMark(item.id!)}
                        />
                      </td>
                      <td class="font-mono">{item.abbreviation}</td>
                      <td class="truncate">
                        <span>{item.description || preview(item.body)}</span>
                        {#if item.description}
                          <span class="ml-2 text-xs muted">{preview(item.body)}</span>
                        {/if}
                        {#if where !== "Todos los servidores"}
                          <span class="tag tag-info ml-2" title="Solo vale en: {where}">{where}</span>
                        {/if}
                        {#if item.shortcut}
                          <span class="tag tag-neutral ml-1 font-mono" title="Atajo de teclado">
                            {formatCombo(item.shortcut, MAC)}
                          </span>
                        {/if}
                      </td>
                      <td class="text-xs {snippets.usage[item.id!]?.uses ? '' : 'muted'}">
                        {usageText(snippets.usage[item.id!], nowSeconds)}
                      </td>
                      <td>
                        <div class="row-actions">
                          <button
                            class="btn btn-icon btn-sm"
                            title="Editar «{item.abbreviation}»"
                            onclick={() => edit(item)}
                          >
                            <Icon name="edit" />
                          </button>
                          <button
                            class="btn btn-icon btn-sm"
                            title="Duplicar «{item.abbreviation}»"
                            onclick={() => duplicate(item)}
                          >
                            <Icon name="copy" />
                          </button>
                          <button
                            class="btn btn-icon btn-sm"
                            title="Borrar «{item.abbreviation}»"
                            onclick={() => remove(item)}
                          >
                            <Icon name="trash" />
                          </button>
                        </div>
                      </td>
                    </tr>
                  {/each}
                {/if}
              {:else}
                <tr><td colspan="5" class="muted">Ninguna abreviatura coincide con «{query}».</td></tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    {/if}

    {#snippet footer()}
      {#if editing}
        <button class="btn btn-ghost" onclick={() => (editing = null)} disabled={busy}>Cancelar</button>
        <button class="btn btn-primary" onclick={submit} disabled={busy || !canSave}>Guardar</button>
      {:else if importing}
        <button class="btn btn-ghost" onclick={() => (importing = null)} disabled={busy}>Cancelar</button>
        <button class="btn btn-primary" onclick={runImport} disabled={busy}>Importar</button>
      {:else if confirmingReset}
        <span class="mr-auto text-xs muted">Se descartan las tuyas y vuelven las de fábrica.</span>
        <button class="btn btn-ghost" onclick={() => (confirmingReset = false)}>Cancelar</button>
        <button class="btn btn-danger" onclick={reset} disabled={busy}>Restablecer</button>
      {:else}
        <button class="btn btn-ghost mr-auto" onclick={() => (confirmingReset = true)}>Restablecer</button>
        <button class="btn" onclick={() => (showLibrary = true)}>Biblioteca…</button>
        <button class="btn" onclick={pickImport} disabled={busy}>Importar…</button>
        <button
          class="btn"
          onclick={exportSnippets}
          disabled={busy || snippets.items.length === 0}
          title={marked.length > 0 ? `Exporta las ${marked.length} marcadas` : "Exporta todas"}
        >
          Exportar…
        </button>
        <button class="btn" onclick={create}>Nueva</button>
        <button class="btn btn-primary" onclick={onclose}>Listo</button>
      {/if}
    {/snippet}
  </Modal>
{/if}

{#if confirmingDelete}
  <Confirm
    title="Borrar {marked.length === 1 ? 'la abreviatura marcada' : `las ${marked.length} abreviaturas marcadas`}"
    message="No se pueden recuperar desde acá. Si no estás seguro, exportalas antes."
    confirmLabel="Borrar"
    onconfirm={removeMarked}
    onclose={() => (confirmingDelete = false)}
  />
{/if}
