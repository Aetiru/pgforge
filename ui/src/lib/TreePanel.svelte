<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import { environmentOf, isReadOnly } from "./access.svelte";
  import { envLook, lookOf, READ_ONLY_LOOK, tagLook } from "./badges";
  import { explorer, visibleRows, type Row } from "./explorer.svelte";
  import { describeError, folderOf } from "./ipc";
  import {
    connectionUrl,
    dataTargetOf,
    erdTargetOf,
    qualifiedNameOf,
    queryTargetOf,
  } from "./tree-actions";
  import { guideAt, guideSpans } from "./tree-guides";

  let {
    onconnect,
    onnew,
    onedit,
    onduplicate,
    ondelete,
    ongroup,
    onquery,
    ondata,
    onerd,
  }: {
    onconnect: (profileId: string) => void;
    /** Abre el diálogo de servidor nuevo desde el estado vacío. */
    onnew?: () => void;
    /** Abre el diálogo del servidor para editarlo. */
    onedit?: (profileId: string) => void;
    /** Abre el diálogo con una copia del servidor, para guardarla como uno nuevo. */
    onduplicate?: (profileId: string) => void;
    /** Pide confirmación y borra el perfil. */
    ondelete?: (profileId: string) => void;
    /** Abre el diálogo para renombrar o deshacer una carpeta de conexiones. */
    ongroup?: (name: string) => void;
    onquery: (profileId: string, database: string, title: string) => void;
    ondata: (profileId: string, database: string, title: string, oid: number) => void;
    onerd: (profileId: string, database: string, schema: string) => void;
  } = $props();

  /**
   * El árbol tiene tres registros y no uno con quince variantes: una **carpeta** —de conexiones o de
   * catálogo— es un encabezado de sección, un **servidor** una ficha de dos líneas —nombre arriba, a
   * dónde apunta abajo— y todo lo demás una fila densa. El servidor es la única que se lee eligiendo
   * entre veinte parecidas; las tablas de un esquema se recorren, y ahí lo que sirve es que entren
   * muchas.
   *
   * Las alturas dejan de ser iguales, así que la ventana visible ya no sale de una división: se
   * acumulan los desplazamientos una vez por lista y se busca en ellos por bisección. Un esquema con
   * miles de tablas sigue dibujando solo lo que entra en pantalla.
   */
  const ROW_HEIGHT = 28;
  const SERVER_HEIGHT = 42;
  const SECTION_HEIGHT = 24;
  const OVERSCAN = 8;

  /** Sangría por nivel y desde dónde arranca la primera, las dos en píxeles. */
  const INDENT = 11;
  const INDENT_BASE = 6;

  /**
   * Si la fila agrupa en vez de nombrar algo: la carpeta de conexiones y las del catálogo («Tablas»,
   * «Índices»). Se dibujan como rótulo —versalita, sin ícono, con el contador a la derecha—, así que
   * el ojo separa contenedor de objeto por la forma y no por el color de un ícono de carpeta.
   */
  const isSection = (row: Row) =>
    row.kind === "group" || (row.node !== null && folderOf(row.node.kind) !== null);

  const heightOf = (row: Row) =>
    row.kind === "server" ? SERVER_HEIGHT : isSection(row) ? SECTION_HEIGHT : ROW_HEIGHT;

  let scrollTop = $state(0);
  let viewportHeight = $state(600);
  let viewport = $state<HTMLDivElement | null>(null);

  /** El servidor que se está arrastrando y la carpeta sobre la que caería si se soltara ahora. */
  let dragging = $state<string | null>(null);
  let dropGroup = $state<string | null | undefined>(undefined);
  let moveError = $state<string | null>(null);

  const rows = $derived(visibleRows(explorer.roots, explorer.search, explorer.onlyConnected));

  /** Dónde arranca cada fila, más la altura total al final. */
  const offsets = $derived.by(() => {
    const out = new Array<number>(rows.length + 1);
    let top = 0;
    for (let index = 0; index < rows.length; index++) {
      out[index] = top;
      top += heightOf(rows[index]);
    }
    out[rows.length] = top;
    return out;
  });

  /** La última fila que empieza en `top` o antes. */
  function indexAt(top: number): number {
    let low = 0;
    let high = rows.length - 1;
    let found = 0;
    while (low <= high) {
      const mid = (low + high) >> 1;
      if (offsets[mid] <= top) {
        found = mid;
        low = mid + 1;
      } else {
        high = mid - 1;
      }
    }
    return found;
  }

  const start = $derived(Math.max(0, indexAt(scrollTop) - OVERSCAN));
  const visible = $derived(
    rows.slice(start, Math.min(rows.length, indexAt(scrollTop + viewportHeight) + 1 + OVERSCAN)),
  );

  const needle = $derived(explorer.search.trim().toLowerCase());

  /**
   * Las guías de sangría que se dibujan: solo la cadena de la fila elegida (ver `tree-guides`). Se
   * recalculan al cambiar la selección o la lista, no una vez por fila dibujada.
   */
  const guides = $derived(
    guideSpans(
      rows.map((row) => row.level),
      rows.findIndex((row) => row.key === explorer.selected?.key),
    ),
  );

  function activate(row: Row) {
    explorer.select(row);
    // Un contenedor se abre con el mismo clic que lo selecciona: pedir doble clic para algo tan
    // frecuente es una regla que hay que descubrir.
    if (row.hasChildren && !explorer.needsConnection(row)) {
      explorer.toggle(row);
    }
  }

  /**
   * El menú del clic derecho, con la fila sobre la que se abrió.
   *
   * Va fijo a la ventana y no adentro del árbol: adentro lo recortaría el desplazamiento en cuanto
   * se abriera cerca del borde de abajo, que es donde están las tablas que uno busca.
   */
  let menu = $state<{ x: number; y: number; row: Row } | null>(null);

  /**
   * Cuánto alto reservarle al menú para que no se abra pasando el borde de abajo. Es el caso más
   * largo —un servidor conectado, con todo lo del perfil— y no el promedio: quedarse corto acá se
   * paga con un menú recortado justo en «Eliminar».
   */
  const MENU_HEIGHT = 340;

  const menuProfile = $derived(
    menu ? explorer.profiles.find((profile) => profile.id === menu?.row.profileId) : undefined,
  );
  const menuQuery = $derived(menu ? queryTargetOf(menu.row, menuProfile) : null);
  const menuData = $derived(menu ? dataTargetOf(menu.row.node) : null);
  const menuErd = $derived(menu ? erdTargetOf(menu.row.node) : null);
  const menuName = $derived(menu ? qualifiedNameOf(menu.row.node) : null);
  const menuIsServer = $derived(menu?.row.kind === "server");

  function openMenu(event: MouseEvent, row: Row) {
    event.preventDefault();
    // Abrir el menú también selecciona: lo que se hace desde él es sobre esta fila, y verla marcada
    // evita el clic derecho sobre una tabla con otra resaltada.
    explorer.select(row);
    menu = { x: event.clientX, y: event.clientY, row };
  }

  /**
   * Cada acción se escribe entera y no pasa por un ayudante que reciba una función.
   *
   * `menuQuery` y compañía son `$derived` de `menu`, así que cerrar el menú primero los recalcula a
   * `null` **antes** de que la acción los lea, y el clic termina en un `TypeError` en vez de abrir
   * nada. Lo único que lo evita es leer lo que hace falta mientras `menu` sigue en pie, y recién
   * después cerrarlo; que sean cinco funciones parecidas es el precio de que eso quede a la vista.
   */
  function newQuery() {
    const row = menu?.row;
    const target = menuQuery;
    menu = null;
    if (row && target) onquery(row.profileId, target.database, target.title);
  }

  function openData() {
    const row = menu?.row;
    const target = menuQuery;
    const oid = menuData;
    menu = null;
    if (row && target && oid !== null) ondata(row.profileId, target.database, target.title, oid);
  }

  function openErd() {
    const row = menu?.row;
    const target = menuErd;
    menu = null;
    if (row && target) onerd(row.profileId, target.database, target.schema);
  }

  function connect() {
    const row = menu?.row;
    menu = null;
    if (row) onconnect(row.profileId);
  }

  function disconnect() {
    const profileId = menu?.row.profileId;
    menu = null;
    if (profileId) {
      explorer.disconnect(profileId).catch((error) => (moveError = describeError(error)));
    }
  }

  function editServer() {
    const profileId = menu?.row.profileId;
    menu = null;
    if (profileId) onedit?.(profileId);
  }

  function duplicateServer() {
    const profileId = menu?.row.profileId;
    menu = null;
    if (profileId) onduplicate?.(profileId);
  }

  function deleteServer() {
    const profileId = menu?.row.profileId;
    menu = null;
    if (profileId) ondelete?.(profileId);
  }

  async function copyUrl() {
    const profile = menuProfile;
    menu = null;
    if (profile) await navigator.clipboard.writeText(connectionUrl(profile)).catch(() => {});
  }

  function reload() {
    const row = menu?.row;
    menu = null;
    if (row) explorer.reload(row);
  }

  async function copyName() {
    const name = menuName;
    menu = null;
    if (name) await navigator.clipboard.writeText(name).catch(() => {});
  }

  /** Deja la fila `index` dentro de la ventana visible, sin moverla si ya se ve. */
  function reveal(index: number) {
    if (!viewport) return;
    const top = offsets[index];
    const height = heightOf(rows[index]);
    if (top < scrollTop) viewport.scrollTop = top;
    else if (top + height > scrollTop + viewportHeight) {
      viewport.scrollTop = top + height - viewportHeight;
    }
  }

  /**
   * La selección también llega de afuera del árbol: «Revelar en el árbol» desde una pestaña de
   * datos, o el servidor que se acaba de conectar. Sin esto la fila queda elegida en algún punto
   * del desplazamiento que nadie está mirando.
   *
   * Lo que se lee adentro va en `untrack` a propósito: si el efecto dependiera del desplazamiento,
   * alejarse de la fila elegida con la rueda haría que el árbol volviera solo, peleando con el
   * usuario. Depende únicamente de qué fila está elegida.
   */
  $effect(() => {
    const key = explorer.selected?.key;
    if (!key) return;

    untrack(() => {
      const index = rows.findIndex((row) => row.key === key);
      if (index >= 0) reveal(index);
    });
  });

  function move(delta: number) {
    if (rows.length === 0) return;
    const current = rows.findIndex((row) => row.key === explorer.selected?.key);
    const next = Math.min(rows.length - 1, Math.max(0, current < 0 ? 0 : current + delta));
    explorer.select(rows[next]);
    reveal(next);
  }

  function goTo(index: number) {
    if (rows.length === 0) return;
    explorer.select(rows[index]);
    reveal(index);
  }

  /**
   * El teclado se maneja en el contenedor y no en cada fila: con la ventana deslizante, las filas
   * de arriba y de abajo ni siquiera existen en el DOM, así que no hay adónde mover el foco. El
   * árbol entero es un punto de tabulación y la fila elegida se anuncia con `aria-activedescendant`.
   */
  function onKey(event: KeyboardEvent) {
    const row = explorer.selected;

    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        move(1);
        break;
      case "ArrowUp":
        event.preventDefault();
        move(-1);
        break;
      case "Home":
        event.preventDefault();
        goTo(0);
        break;
      case "End":
        event.preventDefault();
        goTo(rows.length - 1);
        break;
      case "ArrowRight":
        if (!row) return;
        event.preventDefault();
        // Abrir; si ya estaba abierto, bajar al primer hijo, que es lo que acaba de aparecer.
        if (row.hasChildren && !row.expanded) explorer.toggle(row);
        else if (row.expanded) move(1);
        break;
      case "ArrowLeft":
        if (!row) return;
        event.preventDefault();
        // Cerrar; si ya estaba cerrado, subir al padre, que es de donde cuelga.
        if (row.expanded) explorer.toggle(row);
        else move(-1);
        break;
      case "Enter":
      case " ":
        if (!row) return;
        event.preventDefault();
        // Sobre un servidor desconectado no hay nada que expandir: lo que se espera es conectarlo.
        if (explorer.needsConnection(row)) onconnect(row.profileId);
        else activate(row);
        break;
    }
  }

  /**
   * A qué carpeta va a parar un servidor soltado sobre esta fila. Vale cualquier fila de la
   * carpeta y no solo su encabezado: apuntar a una línea de 28 píxeles con el mouse apretado es
   * más difícil de lo que parece.
   */
  function dropTargetOf(row: Row): string | null {
    return row.kind === "group" ? row.group! : (row.group ?? null);
  }

  function onDrop(group: string | null) {
    const profileId = dragging;
    dragging = null;
    dropGroup = undefined;
    if (!profileId) return;
    // Mover un servidor reescribe el archivo de conexiones: si no se pudo, hay que decirlo, o la
    // fila vuelve a su lugar sin explicación.
    explorer.moveToGroup(profileId, group).catch((error) => (moveError = describeError(error)));
  }

  /**
   * Si la fila lleva una línea de separación arriba. Solo la llevan servidores y carpetas: son los
   * bloques que se leen enteros, y sin separación una lista de veinte conexiones es una sola mancha
   * de texto. El primer servidor de una carpeta no la lleva, porque el encabezado y lo que agrupa
   * son la misma cosa.
   */
  function separated(index: number): boolean {
    if (index === 0) return false;
    const row = rows[index];
    if (row.kind === "node") return false;
    return !(row.kind === "server" && rows[index - 1].kind === "group");
  }

  /**
   * Parte la etiqueta en los tramos que coinciden con la búsqueda y los que no. Sin esto hay que
   * releer cada fila para encontrar por qué apareció en el resultado.
   */
  function pieces(label: string): { text: string; hit: boolean }[] {
    if (needle === "") return [{ text: label, hit: false }];

    const out: { text: string; hit: boolean }[] = [];
    const lower = label.toLowerCase();
    let at = 0;
    for (;;) {
      const found = lower.indexOf(needle, at);
      if (found < 0) break;
      if (found > at) out.push({ text: label.slice(at, found), hit: false });
      out.push({ text: label.slice(found, found + needle.length), hit: true });
      at = found + needle.length;
    }
    if (at < label.length) out.push({ text: label.slice(at), hit: false });
    return out;
  }
</script>

<svelte:window onclick={() => (menu = null)} />

<!-- El contenedor recibe lo que se suelta fuera de toda carpeta: ahí es donde el servidor queda
     suelto. Las filas que sí tienen carpeta cortan la propagación antes de llegar acá. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  bind:this={viewport}
  class="h-full overflow-auto outline-none {dragging !== null && dropGroup === null
    ? 'rounded-md ring-1 ring-blue-500/50'
    : ''}"
  onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
  onkeydown={onKey}
  ondragover={(event) => {
    if (dragging === null) return;
    event.preventDefault();
    dropGroup = null;
  }}
  ondrop={(event) => {
    event.preventDefault();
    onDrop(null);
  }}
  bind:clientHeight={viewportHeight}
  role="tree"
  tabindex="0"
  aria-label="Servidores y objetos"
  aria-activedescendant={explorer.selected ? `tree-${explorer.selected.key}` : undefined}
>
  {#if moveError}
    <Alert tone="bad" onclose={() => (moveError = null)}>{moveError}</Alert>
  {/if}

  {#if explorer.hits !== null}
    <!--
      El resultado de buscar contra el servidor reemplaza al árbol mientras dura. No se mezcla con
      las filas: lo que se encontró puede estar en esquemas que el árbol nunca abrió, y meterlo
      adentro obligaría a inventar ramas para poder colgarlo.
    -->
    <div class="flex items-center gap-2 px-3 py-1.5 text-[11px] muted">
      {#if explorer.searching}
        <span class="spinner"></span>
        <span>Buscando en el servidor…</span>
      {:else}
        <span>
          {explorer.hits.length}
          {explorer.hits.length === 1 ? "coincidencia" : "coincidencias"} en el servidor
        </span>
      {/if}
      <button class="btn btn-ghost btn-sm ml-auto" onclick={() => explorer.clearHits()}>
        Volver al árbol
      </button>
    </div>

    {#if explorer.searchError}
      <Alert tone="bad">{explorer.searchError}</Alert>
    {:else if explorer.hits.length === 0 && !explorer.searching}
      <Empty
        icon="search"
        title="Sin coincidencias en el servidor"
        hint="No hay ninguna tabla, vista, secuencia, función ni tipo cuyo nombre contenga «{explorer.search}» en esa base."
      />
    {:else}
      {#each explorer.hits as hit (`${hit.schema}.${hit.oid}`)}
        {@const look = lookOf(hit.kind)}
        <button
          class="flex w-full items-center gap-1.5 rounded-md py-1 pr-2 pl-3 text-left text-sm
                 hover:bg-zinc-100 dark:hover:bg-zinc-800/70"
          title="Revelar en el árbol"
          onclick={() => explorer.revealHit(hit)}
        >
          <Icon name={look.icon} class={look.tone} />
          <span class="min-w-0 truncate">{hit.label}</span>
          <span class="min-w-0 shrink-[100] truncate text-xs muted">{hit.schema}</span>
          {#if hit.detail}
            <span class="ml-auto shrink-0 pl-1 text-xs muted">{hit.detail}</span>
          {/if}
        </button>
      {/each}
    {/if}
  {:else if explorer.roots.length === 0}
    <Empty
      icon="server"
      title="Todavía no hay servidores"
      hint="Agregá una conexión para empezar a explorar sus bases, esquemas y tablas."
    >
      {#if onnew}
        <button class="btn btn-primary" onclick={onnew}>
          <Icon name="plus" size={12} />
          Nuevo servidor
        </button>
      {/if}
    </Empty>
  {:else if rows.length === 0 && explorer.onlyConnected && needle === ""}
    <Empty
      icon="plug"
      title="Ningún servidor conectado"
      hint="El árbol está mostrando solo los servidores conectados y todavía no hay ninguno."
    >
      <button class="btn" onclick={() => (explorer.onlyConnected = false)}>
        Mostrar todos los servidores
      </button>
    </Empty>
  {:else if rows.length === 0}
    <Empty
      icon="search"
      title="Sin coincidencias"
      hint="La búsqueda solo alcanza a lo que ya se cargó del árbol: abrí los nodos donde puede estar «{explorer.search}»."
    >
      {#if explorer.onlyConnected}
        <!-- Con el filtro puesto, lo que falta puede estar en un servidor que el árbol no muestra. -->
        <button class="btn" onclick={() => (explorer.onlyConnected = false)}>
          Buscar también en los servidores sin conectar
        </button>
      {/if}
    </Empty>
  {:else}
    {#if needle !== ""}
      <p class="px-3 py-1.5 text-[11px] muted">
        {rows.length}
        {rows.length === 1 ? "coincidencia" : "coincidencias"} entre lo ya cargado
      </p>
    {/if}

    <div class="relative" style="height: {offsets[rows.length]}px">
      {#each visible as row, index (start + index)}
        {@const at = start + index}
        {@const isGroup = row.kind === "group"}
        {@const isServer = row.kind === "server"}
        {@const look = lookOf(row.node?.kind ?? null)}
        {@const section = isSection(row)}
        {@const isSelected = explorer.selected?.key === row.key}
        {@const isDropTarget = dragging !== null && dropGroup === dropTargetOf(row)}
        {@const environment = environmentOf(row.profileId)}
        <!-- El teclado lo maneja el contenedor: las filas fuera de la ventana no están en el DOM. -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          id="tree-{row.key}"
          class="group absolute left-0 flex w-full items-center gap-1.5 rounded-md pr-1 text-sm
                 {isSelected
            ? 'bg-blue-50 text-blue-900 dark:bg-blue-950/60 dark:text-blue-100'
            : 'hover:bg-zinc-100 dark:hover:bg-zinc-800/70'}
                 {separated(at) ? 'border-t border-zinc-200/80 dark:border-zinc-800' : ''}
                 {isDropTarget && dropGroup !== null ? 'bg-blue-100/70 dark:bg-blue-900/40' : ''}
                 {dragging === row.profileId ? 'opacity-40' : ''}"
          style="top: {offsets[at]}px; height: {heightOf(row)}px; padding-left: {INDENT_BASE +
            row.level * INDENT}px"
          role="treeitem"
          tabindex="-1"
          aria-expanded={row.hasChildren ? row.expanded : undefined}
          aria-selected={isSelected}
          onclick={() => activate(row)}
          oncontextmenu={(event) => openMenu(event, row)}
          ondblclick={() => {
            // Doble clic sobre un servidor apagado: lo que se quiere es entrar.
            if (explorer.needsConnection(row)) onconnect(row.profileId);
          }}
          draggable={isServer}
          ondragstart={(event) => {
            dragging = row.profileId;
            event.dataTransfer?.setData("text/plain", row.label);
            if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
          }}
          ondragend={() => {
            dragging = null;
            dropGroup = undefined;
          }}
          ondragover={(event) => {
            if (dragging === null) return;
            event.preventDefault();
            event.stopPropagation();
            dropGroup = dropTargetOf(row);
          }}
          ondrop={(event) => {
            event.preventDefault();
            event.stopPropagation();
            onDrop(dropTargetOf(row));
          }}
        >
          <!--
            Guías de indentación: saber de qué esquema cuelga una tabla no debería requerir contar
            sangrías con el dedo. Se dibuja solo la cadena de la fila elegida —dibujarlas todas deja
            cinco rayas fijas por fila, que es ruido permanente por una pregunta ocasional—.
          -->
          {#each { length: row.level }, depth (depth)}
            {#if guideAt(guides, at, depth)}
              <span
                class="pointer-events-none absolute inset-y-0 w-px bg-zinc-300 dark:bg-zinc-700"
                style="left: {INDENT_BASE + depth * INDENT + 8}px"
              ></span>
            {/if}
          {/each}

          <!--
            El borde izquierdo dice a qué servidor pertenece la fila y de qué entorno es: la línea
            arranca en el servidor y baja por todo lo que cuelga de él, así que no hay que rastrear
            la sangría hasta la raíz para saber que se está editando producción. Sin entorno marcado
            queda para la fila elegida, que necesita el mismo píxel.

            Mismo ancho en el servidor que en lo que cuelga de él: es una sola columna que baja, y
            un escalón en la raíz la partía en dos.
          -->
          {#if environment}
            {@const badge = envLook(environment)}
            <span
              class="pointer-events-none absolute inset-y-0 left-0 w-0.5 rounded-r {badge.spine}"
            ></span>
          {:else if isSelected}
            <span
              class="pointer-events-none absolute inset-y-0 left-0 w-0.5 rounded-r bg-blue-600
                     dark:bg-blue-400"
            ></span>
          {/if}

          <button
            class="relative grid size-4 shrink-0 place-items-center rounded text-zinc-400
                   hover:text-zinc-900 dark:hover:text-zinc-100"
            onclick={(event) => {
              event.stopPropagation();
              explorer.toggle(row);
            }}
            aria-label={row.expanded ? "Contraer" : "Expandir"}
            tabindex="-1"
            disabled={!row.hasChildren || explorer.needsConnection(row)}
          >
            {#if row.loading}
              <span class="spinner"></span>
            {:else if row.hasChildren && !explorer.needsConnection(row)}
              <Icon
                name="chevron"
                size={12}
                class="transition-transform {row.expanded ? 'rotate-90' : ''}"
              />
            {/if}
          </button>

          <!--
            Una sección no lleva ícono: la carpeta gris del catálogo y la de conexiones eran dos
            formas iguales para dos cosas distintas, y el ámbar de esta última chocaba con el de las
            secuencias y con el del resaltado de la búsqueda. Ahora agrupar se nota por el rótulo.

            El punto de conexión va encima del ícono del servidor y no antes: como columna propia
            corría los íconos de las raíces media pulgada respecto de todo el resto del árbol.
          -->
          {#if !section}
            <span class="relative grid shrink-0 place-items-center">
              <Icon name={look.icon} class={look.tone} />
              {#if isServer}
                <span
                  class="dot absolute -right-1 -bottom-0.5 ring-2 ring-zinc-50 dark:ring-zinc-900
                         {row.connected ? 'dot-on' : 'dot-off'}"
                  title={row.connected ? "Conectado" : "Sin conectar"}
                ></span>
              {/if}
            </span>
          {/if}

          <!--
            El nombre y lo que lo acompaña van en columna: en un servidor son dos líneas —nombre
            arriba, a dónde apunta abajo—, en el resto una sola. Así el nombre del servidor se queda
            con el ancho entero de la fila, que era lo que se le comían el host, el entorno y el
            botón de conectar. El nombre cede espacio último; el detalle se achica primero.
          -->
          <span class="flex min-w-0 flex-1 flex-col justify-center gap-px">
            <span class="flex min-w-0 items-center gap-1.5">
              <span
                class="min-w-0 truncate {isServer ? 'font-medium' : ''}
                       {section ? 'text-[11px] font-semibold tracking-wide uppercase' : ''}
                       {section && !isSelected ? 'muted' : ''}"
                title={row.comment ?? row.label}
              >
                {#each pieces(row.label) as piece, position (position)}
                  {#if piece.hit}
                    <mark class="rounded bg-amber-200 text-inherit dark:bg-amber-500/40"
                      >{piece.text}</mark
                    >
                  {:else}{piece.text}{/if}
                {/each}
              </span>

              <!--
                Los rasgos del objeto van como etiquetas y no como texto: que un rol pueda entrar o
                que una tabla filtre filas se reconoce por el color, sin leer la fila entera.
              -->
              {#each row.node?.tags ?? [] as tag (tag)}
                {@const badge = tagLook(tag)}
                <span class="tag {badge.tone} shrink-0" title={badge.title}>{badge.label}</span>
              {/each}

              <!-- Producción lo dice con palabras y arriba, junto al nombre: es la marca que no se
                   puede depender de reconocer por el color, y la que decide si se sigue o no. Los
                   otros entornos se conforman con la línea del borde y el texto de abajo. -->
              {#if isServer && environment === "prod"}
                {@const badge = envLook(environment)}
                <span class="tag {badge.tone} shrink-0" title={badge.title}>{badge.label}</span>
              {/if}

              {#if !isServer && row.error}
                <span
                  class="min-w-0 shrink-[100] truncate text-xs text-rose-600 dark:text-rose-400"
                  title={row.error}
                >
                  {row.error}
                </span>
              {/if}
            </span>

            <!-- Segunda línea: todo lo que describe al servidor sin identificarlo. Se lee cuando se
                 duda entre dos conexiones parecidas, no en cada pasada por el árbol. -->
            {#if isServer}
              <span class="flex min-w-0 items-center gap-1.5 text-[11px] leading-none muted">
                {#if row.detail}
                  <span class="min-w-0 shrink truncate" title={row.detail}>{row.detail}</span>
                {/if}

                <!-- Solo lectura, sin la palabra: el candado dice lo mismo y el texto se comía el
                     host. El entorno tampoco se repite acá —ya lo dice la línea del borde, y
                     producción además la pastilla de arriba—. -->
                {#if isReadOnly(row.profileId)}
                  <span class="shrink-0" title={READ_ONLY_LOOK.title}>
                    <Icon name={READ_ONLY_LOOK.icon} size={10} />
                  </span>
                {/if}

                {#if row.error}
                  <span
                    class="min-w-0 shrink-[100] truncate text-rose-600 dark:text-rose-400"
                    title={row.error}
                  >
                    {row.error}
                  </span>
                {/if}
              </span>
            {/if}
          </span>

          <span class="ml-auto flex shrink-0 items-center gap-0.5 pl-1">
            {#if isGroup && ongroup}
              <button
                class="btn btn-ghost btn-icon size-6 opacity-0 focus-visible:opacity-100
                       group-hover:opacity-100"
                title="Renombrar o deshacer la carpeta"
                aria-label="Editar la carpeta"
                tabindex="-1"
                onclick={(event) => {
                  event.stopPropagation();
                  ongroup(row.group!);
                }}
              >
                <Icon name="edit" size={11} />
              </button>
            {/if}

            {#if !isGroup && row.children !== null && row.hasChildren}
              <button
                class="btn btn-ghost btn-icon size-6 opacity-0 focus-visible:opacity-100
                       group-hover:opacity-100"
                title="Volver a leer este nodo del servidor"
                aria-label="Recargar"
                tabindex="-1"
                onclick={(event) => {
                  event.stopPropagation();
                  explorer.reload(row);
                }}
              >
                <Icon name="refresh" size={11} />
              </button>
            {/if}

            <!-- «Conectar» era una palabra fija en cada servidor apagado: con veinte, era una
                 columna de texto repetido comiéndose el nombre. Ahora es un ícono que aparece al
                 pasar por encima o al elegir la fila —doble clic y Enter siguen conectando—. -->
            {#if isServer && !row.connected}
              <button
                class="btn btn-ghost btn-icon size-6 text-blue-600 group-hover:opacity-100
                       focus-visible:opacity-100 dark:text-blue-400 {isSelected
                  ? ''
                  : 'opacity-0'}"
                title="Conectar"
                aria-label="Conectar"
                tabindex="-1"
                onclick={(event) => {
                  event.stopPropagation();
                  onconnect(row.profileId);
                }}
              >
                <Icon name="plug" size={12} />
              </button>
            {/if}

            <!--
              Lo que describe a la fila va al final, en columna: el tipo de una columna, el tamaño
              de una tabla y el contador de una sección se comparan de un vistazo cuando están
              alineados, y pegados al nombre lo empujaban a truncarse. El contador de una sección
              lleva pastilla; el resto es texto, que se achica primero.
            -->
            {#if row.detail && section}
              <span class="seg-count tabular-nums">{row.detail}</span>
            {:else if row.detail && !isServer}
              <span class="max-w-28 truncate text-xs muted" title={row.detail}>{row.detail}</span>
            {/if}
          </span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<!--
  Lo que se puede abrir desde una fila, sin pasar por el panel de detalle. Era el camino de todos
  los días —elegir la tabla, ir a «Detalle», bajar hasta el botón— y son dos clics de más cada vez.
-->
{#if menu}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="card fixed z-40 min-w-52 p-1 text-sm shadow-lg"
    style="left: {Math.min(menu.x, window.innerWidth - 250)}px;
           top: {Math.min(menu.y, window.innerHeight - MENU_HEIGHT)}px"
    role="menu"
    tabindex="-1"
    onclick={(event) => event.stopPropagation()}
  >
    {#if explorer.needsConnection(menu.row)}
      <button class="row-menu" onclick={connect}>
        <span class="flex items-center gap-2"><Icon name="plug" size={13} /> Conectar</span>
      </button>
    {:else if menuIsServer}
      <button class="row-menu" onclick={disconnect}>
        <span class="flex items-center gap-2">
          <Icon name="unplug" size={13} /> Desconectar
        </span>
      </button>
    {/if}

    {#if menuQuery}
      <button class="row-menu" onclick={newQuery}>
        <span class="flex items-center gap-2"><Icon name="sql" size={13} /> Nueva consulta</span>
        <span class="muted">Ctrl+Q</span>
      </button>
    {/if}

    {#if menuQuery && menuData !== null}
      <button class="row-menu" onclick={openData}>
        <span class="flex items-center gap-2"><Icon name="table" size={13} /> Ver los datos</span>
      </button>
    {/if}

    {#if menuErd}
      <button class="row-menu" onclick={openErd}>
        <span class="flex items-center gap-2">
          <Icon name="diagram" size={13} /> Diagrama del esquema
        </span>
      </button>
    {/if}

    {#if menuName}
      <button class="row-menu" onclick={copyName}>
        <span class="flex items-center gap-2"><Icon name="copy" size={13} /> Copiar el nombre</span>
      </button>
    {/if}

    {#if menuIsServer}
      <button class="row-menu" onclick={copyUrl}>
        <span class="flex items-center gap-2">
          <Icon name="copy" size={13} /> Copiar la cadena de conexión
        </span>
      </button>
    {/if}

    {#if menu.row.hasChildren && !explorer.needsConnection(menu.row)}
      <button class="row-menu" onclick={reload}>
        <span class="flex items-center gap-2"><Icon name="refresh" size={13} /> Refrescar</span>
      </button>
    {/if}

    <!--
      Lo que se le hace al perfil, separado de lo que se le hace al servidor: editar, duplicar y
      eliminar no tocan el servidor de PostgreSQL, tocan lo que esta aplicación tiene guardado. Sin
      esto había que ir al panel de detalle para cualquiera de las tres.
    -->
    {#if menuIsServer && (onedit || onduplicate || ondelete)}
      <div class="divider-t my-1"></div>

      {#if onedit}
        <button class="row-menu" onclick={editServer}>
          <span class="flex items-center gap-2"><Icon name="edit" size={13} /> Editar</span>
        </button>
      {/if}

      {#if onduplicate}
        <button class="row-menu" onclick={duplicateServer}>
          <span class="flex items-center gap-2">
            <Icon name="copy" size={13} /> Duplicar
          </span>
        </button>
      {/if}

      {#if ondelete}
        <button class="row-menu text-rose-600 dark:text-rose-400" onclick={deleteServer}>
          <span class="flex items-center gap-2"><Icon name="trash" size={13} /> Eliminar</span>
        </button>
      {/if}
    {/if}
  </div>
{/if}
