<script lang="ts">
  import Alert from "./Alert.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import { save } from "@tauri-apps/plugin-dialog";

  import { METRICS, layout, neighbors, pathOf, type ErdBox } from "./erd";
  import { describeError, erdExportSvg } from "./ipc";
  import type { ErdTab } from "./erd.svelte";

  let { tab }: { tab: ErdTab } = $props();

  const ZOOM_MIN = 0.25;
  const ZOOM_MAX = 2.5;

  let zoom = $state(1);
  let panX = $state(0);
  let panY = $state(0);

  /** El SVG del diagrama. Hace falta para exportarlo y para pasar de coordenadas de pantalla. */
  let svg = $state<SVGSVGElement | null>(null);

  /** Qué se está arrastrando: el fondo (pan) o una caja, con el desfase del punto agarrado. */
  let drag = $state<
    { kind: "pan"; x: number; y: number } | { kind: "box"; oid: number; dx: number; dy: number } | null
  >(null);

  const plan = $derived(tab.graph ? layout(tab.graph, tab.moved) : null);
  const linked = $derived(
    tab.graph && tab.selected !== null ? neighbors(tab.graph, tab.selected) : new Set<number>(),
  );

  /** De coordenadas de pantalla a coordenadas del diagrama. */
  function at(event: PointerEvent | WheelEvent) {
    const box = svg?.getBoundingClientRect();
    if (!box) return { x: 0, y: 0 };
    return {
      x: (event.clientX - box.left - panX) / zoom,
      y: (event.clientY - box.top - panY) / zoom,
    };
  }

  function startBox(event: PointerEvent, box: ErdBox) {
    const point = at(event);
    tab.selected = box.oid;
    drag = { kind: "box", oid: box.oid, dx: point.x - box.x, dy: point.y - box.y };
    (event.currentTarget as Element).setPointerCapture(event.pointerId);
    event.stopPropagation();
  }

  function startPan(event: PointerEvent) {
    // Clic en el fondo: deselecciona y empieza a mover el lienzo.
    tab.selected = null;
    drag = { kind: "pan", x: event.clientX - panX, y: event.clientY - panY };
    (event.currentTarget as Element).setPointerCapture(event.pointerId);
  }

  function move(event: PointerEvent) {
    if (!drag) return;
    if (drag.kind === "pan") {
      panX = event.clientX - drag.x;
      panY = event.clientY - drag.y;
      return;
    }
    const point = at(event);
    tab.move(drag.oid, Math.round(point.x - drag.dx), Math.round(point.y - drag.dy));
  }

  function stop() {
    drag = null;
  }

  /** La rueda acerca sobre el puntero: hacer zoom al centro obliga a repanear en cada paso. */
  function wheel(event: WheelEvent) {
    event.preventDefault();
    const before = at(event);
    const next = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, zoom * (event.deltaY < 0 ? 1.1 : 1 / 1.1)));
    const box = svg?.getBoundingClientRect();
    if (!box) return;

    zoom = next;
    panX = event.clientX - box.left - before.x * next;
    panY = event.clientY - box.top - before.y * next;
  }

  /** Deja todo el diagrama a la vista. */
  function fit() {
    const box = svg?.getBoundingClientRect();
    if (!box || !plan || plan.width === 0 || plan.height === 0) return;

    zoom = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, Math.min(box.width / plan.width, box.height / plan.height)));
    panX = (box.width - plan.width * zoom) / 2;
    panY = (box.height - plan.height * zoom) / 2;
  }

  function rowY(box: ErdBox, index: number): number {
    return box.y + METRICS.headerHeight + index * METRICS.rowHeight + METRICS.rowHeight / 2;
  }

  /**
   * El color al que llega una variable del tema. Se resuelve midiendo y no leyendo la variable
   * porque su valor es otra variable de Tailwind: fuera de la aplicación, el SVG exportado no
   * tendría con qué resolverla.
   */
  function resolved(name: string): string {
    const probe = document.createElement("span");
    probe.style.color = `var(${name})`;
    probe.style.display = "none";
    document.body.append(probe);
    const value = getComputedStyle(probe).color;
    probe.remove();
    return value;
  }

  /** Guarda el diagrama entero —no lo que se ve— como SVG. */
  async function exportSvg() {
    if (!svg || !plan) return;

    const chosen = await save({
      title: "Dónde guardar el diagrama",
      defaultPath: `${tab.schema}.svg`,
    });
    if (typeof chosen !== "string") return;

    const clone = svg.cloneNode(true) as SVGSVGElement;
    clone.setAttribute("xmlns", "http://www.w3.org/2000/svg");
    clone.setAttribute("width", String(Math.round(plan.width)));
    clone.setAttribute("height", String(Math.round(plan.height)));
    clone.setAttribute("viewBox", `0 0 ${Math.round(plan.width)} ${Math.round(plan.height)}`);
    clone.removeAttribute("class");
    // El archivo lleva el diagrama completo, sin el zoom ni el desplazamiento de la pantalla.
    clone.querySelector("g")?.setAttribute("transform", "translate(0 0)");

    const vars = ["bg", "box", "header", "border", "text", "muted", "edge", "accent"]
      .map((name) => `--erd-${name}: ${resolved(`--erd-${name}`)};`)
      .join(" ");
    const style = document.createElementNS("http://www.w3.org/2000/svg", "style");
    style.textContent = `svg { ${vars} font-family: 'Source Code Pro', monospace; }`;
    clone.prepend(style);

    try {
      await erdExportSvg(chosen, new XMLSerializer().serializeToString(clone));
    } catch (error) {
      tab.error = describeError(error);
    }
  }

  /** Una tabla se apaga cuando hay otra seleccionada y no la toca ninguna arista compartida. */
  function dimmed(oid: number): boolean {
    return tab.selected !== null && tab.selected !== oid && !linked.has(oid);
  }

  function edgeActive(source: number, target: number): boolean {
    return tab.selected !== null && (tab.selected === source || tab.selected === target);
  }
</script>

<div class="flex h-full flex-col">
  <div class="toolbar">
    <button class="btn" title="Vuelve a traer el grafo del servidor" onclick={() => tab.load()}>
      <Icon name="refresh" size={12} />
      Actualizar
    </button>

    <span class="toolbar-sep"></span>

    <button class="btn btn-icon" title="Alejar" onclick={() => (zoom = Math.max(ZOOM_MIN, zoom / 1.2))}>
      −
    </button>
    <span class="text-xs muted tabular-nums">{Math.round(zoom * 100)}%</span>
    <button class="btn btn-icon" title="Acercar" onclick={() => (zoom = Math.min(ZOOM_MAX, zoom * 1.2))}>
      +
    </button>
    <button class="btn" title="Deja todo el diagrama a la vista" onclick={fit}>Encajar</button>
    <button
      class="btn"
      title="Descarta las tablas que se movieron a mano"
      disabled={Object.keys(tab.moved).length === 0}
      onclick={() => tab.reset()}
    >
      Reacomodar
    </button>

    <span class="toolbar-sep"></span>

    <button
      class="btn"
      title="Guarda el diagrama completo como SVG"
      disabled={!tab.graph}
      onclick={exportSvg}
    >
      <Icon name="download" size={12} />
      Exportar SVG
    </button>

    <span class="ml-auto flex items-center gap-2 text-xs muted">
      {#if tab.loading}<span class="spinner"></span>{/if}
      {#if tab.graph}
        <span class="tabular-nums">
          {tab.graph.tables.length} tablas · {tab.graph.edges.length} referencias
        </span>
      {/if}
    </span>
  </div>

  {#if tab.error}
    <div class="p-3">
      <Alert tone="bad" box>{tab.error}</Alert>
    </div>
  {:else if tab.graph && tab.graph.tables.length === 0}
    <Empty
      icon="schema"
      title="El esquema no tiene tablas"
      hint="Un diagrama necesita al menos una tabla que dibujar."
    />
  {:else if plan}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <svg
      bind:this={svg}
      class="min-h-0 flex-1 cursor-grab"
      style="background: var(--erd-bg)"
      role="img"
      aria-label={`Diagrama del esquema ${tab.schema}`}
      onpointerdown={startPan}
      onpointermove={move}
      onpointerup={stop}
      onpointercancel={stop}
      onwheel={wheel}
    >
      <defs>
        <marker
          id="erd-arrow"
          viewBox="0 0 8 8"
          refX="7"
          refY="4"
          markerWidth="7"
          markerHeight="7"
          orient="auto-start-reverse"
        >
          <path d="M 0 1 L 7 4 L 0 7 z" fill="var(--erd-edge)" />
        </marker>
        <marker
          id="erd-arrow-activa"
          viewBox="0 0 8 8"
          refX="7"
          refY="4"
          markerWidth="7"
          markerHeight="7"
          orient="auto-start-reverse"
        >
          <path d="M 0 1 L 7 4 L 0 7 z" fill="var(--erd-accent)" />
        </marker>
      </defs>

      <g transform={`translate(${panX} ${panY}) scale(${zoom})`}>
        {#each plan.links as link (link.edge.name + link.edge.source)}
          {@const activa = edgeActive(link.edge.source, link.edge.target)}
          <path
            d={pathOf(link)}
            fill="none"
            stroke={activa ? "var(--erd-accent)" : "var(--erd-edge)"}
            stroke-width={activa ? 1.6 : 1}
            stroke-dasharray={link.external ? "4 3" : undefined}
            marker-end={activa ? "url(#erd-arrow-activa)" : "url(#erd-arrow)"}
            opacity={tab.selected !== null && !activa ? 0.35 : 1}
          >
            <title>
              {link.edge.name} · {link.edge.sourceColumns.join(", ")} → {link.edge.targetColumns.join(
                ", ",
              )}
            </title>
          </path>

          {#if link.external}
            <text
              x={link.points[1].x + 4}
              y={link.points[1].y}
              font-size="10"
              dominant-baseline="middle"
              fill="var(--erd-muted)"
            >
              {link.edge.targetLabel}
            </text>
          {/if}
        {/each}

        {#each plan.boxes as box (box.oid)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <g
            class="cursor-move"
            opacity={dimmed(box.oid) ? 0.35 : 1}
            onpointerdown={(event) => startBox(event, box)}
            onpointermove={move}
            onpointerup={stop}
            onpointercancel={stop}
          >
            <rect
              x={box.x}
              y={box.y}
              width={box.width}
              height={box.height}
              rx="4"
              fill="var(--erd-box)"
              stroke={tab.selected === box.oid ? "var(--erd-accent)" : "var(--erd-border)"}
              stroke-width={tab.selected === box.oid ? 2 : 1}
            />
            <path
              d={`M ${box.x} ${box.y + METRICS.headerHeight} h ${box.width}`}
              stroke="var(--erd-border)"
              stroke-width="1"
            />
            <rect
              x={box.x}
              y={box.y}
              width={box.width}
              height={METRICS.headerHeight}
              rx="4"
              fill="var(--erd-header)"
            />
            <text
              x={box.x + METRICS.padding}
              y={box.y + METRICS.headerHeight / 2}
              font-size="12"
              font-weight="600"
              dominant-baseline="middle"
              fill="var(--erd-text)"
            >
              {box.table.name}
            </text>
            {#if box.table.kind !== "table"}
              <text
                x={box.x + box.width - METRICS.padding}
                y={box.y + METRICS.headerHeight / 2}
                font-size="9"
                text-anchor="end"
                dominant-baseline="middle"
                fill="var(--erd-muted)"
              >
                {box.table.kind === "partitionedTable" ? "particionada" : "foránea"}
              </text>
            {/if}

            {#each box.columns as column, index (column.position)}
              <text
                x={box.x + METRICS.padding}
                y={rowY(box, index)}
                font-size="11"
                dominant-baseline="middle"
                fill="var(--erd-text)"
              >
                {column.name}
              </text>
              <text
                x={box.x + box.width - METRICS.padding}
                y={rowY(box, index)}
                font-size="10"
                text-anchor="end"
                dominant-baseline="middle"
                fill="var(--erd-muted)"
              >
                {[column.primaryKey ? "PK" : "", column.foreignKey ? "FK" : "", column.typeName]
                  .filter(Boolean)
                  .join(" ")}
              </text>
            {/each}

            {#if box.hidden > 0}
              <text
                x={box.x + METRICS.padding}
                y={rowY(box, box.columns.length)}
                font-size="10"
                dominant-baseline="middle"
                fill="var(--erd-muted)"
              >
                +{box.hidden} columnas
              </text>
            {/if}
          </g>
        {/each}
      </g>
    </svg>
  {:else}
    <Empty icon="schema" title="Cargando el diagrama…" />
  {/if}
</div>
