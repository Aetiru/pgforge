<script lang="ts" module>
  export interface Column<T> {
    key: string;
    header: string;
    /** Ancho en píxeles. La grilla no mide el contenido: el ancho es parte de la definición. */
    width: number;
    align?: "left" | "right";
    value: (row: T) => string;
    /** Texto del tooltip, para lo que no entra en la celda. */
    title?: (row: T) => string | undefined;
    /** Clases extra según el valor, para resaltar filas problemáticas. */
    tone?: (row: T) => string | undefined;
    /** El texto con el que se abre el editor, que puede no ser el que se muestra. */
    edit?: (row: T) => string | null;
    /**
     * El valor entero, sin recortar ni aplanar: es lo que se copia y lo que muestra el visor de
     * celda. `null` es un NULL de la base, que no es lo mismo que una cadena vacía. Sin esto, copiar
     * una consulta devolvía el texto de la pantalla —con sus «↵» y sus «[null]»— en vez del dato.
     */
    raw?: (row: T) => string | null;
    /**
     * Con qué se ordena la columna. Sin esto se ordena por el texto que se muestra, que alcanza
     * para nombres pero no para números formateados: «1.234» y «999» no se comparan como texto.
     */
    sort?: (row: T) => string | number;
    /**
     * Con qué nombre la ordena el servidor. Solo lo miran las grillas que delegan el orden
     * (`serverSort`): una columna sin esto —el marcador de estado de la fila, por ejemplo— no
     * corresponde a ninguna columna de la tabla y no se puede ordenar allá.
     */
    orderBy?: string;
  }
</script>

<script lang="ts" generics="T">
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import { asJson, delimited, parseDelimited, pretty } from "./grid-copy";
  import { columnRange } from "./grid-window";
  import { gridZoom } from "./grid.svelte";
  import { count } from "./format";

  let {
    columns,
    rows,
    rowKey,
    selectedKey = null,
    onselect,
    empty = "Sin datos.",
    editable,
    onedit,
    onnearend,
    rowClass,
    sortable = false,
    serverSort = false,
    onsort,
  }: {
    columns: Column<T>[];
    rows: T[];
    rowKey: (row: T) => string | number;
    selectedKey?: string | number | null;
    onselect?: (row: T) => void;
    empty?: string;
    /** Sin esto la grilla es de solo lectura, que es como la usan el dashboard y el editor. */
    editable?: (row: T, column: Column<T>) => boolean;
    onedit?: (row: T, column: Column<T>, value: string | null) => void;
    /** Se llama al acercarse al final de lo cargado, para pedir la página siguiente. */
    onnearend?: () => void;
    rowClass?: (row: T) => string | undefined;
    /**
     * Ordenar al hacer clic en el encabezado. Lo piden las grillas que muestran una foto completa
     * —las estadísticas del monitor, el resultado de una consulta—, no las que se paginan o se
     * editan, donde el número de fila tiene que seguir significando su posición real.
     */
    sortable?: boolean;
    /**
     * El orden lo resuelve quien trae los datos, no la grilla. Lo usa la pestaña de datos: ordenar
     * las doscientas filas que se trajeron primero responde otra pregunta que ordenar la tabla.
     */
    serverSort?: boolean;
    onsort?: (column: Column<T> | null, descending: boolean) => void;
  } = $props();

  /**
   * Misma técnica que el árbol: filas de altura fija y ventana calculada por división. El
   * dashboard se refresca cada dos segundos, así que dibujar mil filas en cada muestra sería el
   * mayor costo de toda la aplicación.
   *
   * La ventana se recorta en las dos direcciones. Al desplazar, el compositor mueve el contenido
   * antes de que el hilo principal alcance a dibujar lo que entra por abajo: si el margen es corto,
   * eso se ve como filas que faltan hasta que uno suelta la rueda. El margen de más se paga en cada
   * cuadro, así que lo que lo hace posible es no dibujar las columnas que están fuera de pantalla:
   * una consulta de cuarenta columnas dibujaba dos mil celdas por cuadro para mostrar seis.
   */
  // El alto de fila es la misma preferencia en toda la aplicación (ver `grid.svelte.ts`): agrandar
  // la letra de una grilla agranda las tres.
  const rowHeight = $derived(gridZoom.rowHeight);
  const OVERSCAN = 24;
  const COLUMN_OVERSCAN = 2;
  /** Cuántas filas antes del final disparan la carga de la página siguiente. */
  const NEAR_END = 40;
  const MIN_COLUMN = 48;
  const MAX_FIT = 600;
  /** Cuántas filas mira el ajuste automático de ancho: con cien mil, medirlas todas se nota. */
  const FIT_SAMPLE = 500;

  let scrollTop = $state(0);
  let scrollLeft = $state(0);
  let viewportHeight = $state(400);
  let viewportWidth = $state(800);
  let viewport = $state<HTMLDivElement | null>(null);

  /** Qué celda se está editando: la fila por su clave y la columna por la suya. */
  let editing = $state<{ row: string | number; column: string } | null>(null);
  let draft = $state("");
  /** `true` cuando el editor está por escribir un NULL en vez de un texto. */
  let draftNull = $state(false);

  /** Anchos cambiados a mano. Lo que no está acá conserva el de la definición. */
  let widths = $state<Record<string, number>>({});
  let sort = $state<{ key: string; descending: boolean } | null>(null);

  /**
   * Columnas escondidas y cuántas quedan fijas contra el borde izquierdo. Las dos son de esta
   * grilla y no de la definición: una tabla de treinta columnas se mira de a pedazos, y cuál es el
   * pedazo depende de lo que se esté buscando en ese momento.
   */
  let hidden = $state<Record<string, boolean>>({});
  let frozen = $state(0);
  /** Orden a mano de las columnas visibles. Vacío = el de `columns`, tal como llegó. */
  let order = $state<string[]>([]);
  /** La columna que se está arrastrando y la que está debajo del puntero, para el encabezado. */
  let dragKey = $state<string | null>(null);
  let dragOverKey = $state<string | null>(null);

  /** Filtro rápido sobre lo ya cargado, con su caja escondida hasta que se la pide. */
  let filter = $state("");
  let filterOpen = $state(false);
  let filterBox = $state<HTMLInputElement | null>(null);

  /** Menú del encabezado: lo que se puede hacer sobre una columna. */
  let headerMenu = $state<{ x: number; y: number; index: number } | null>(null);

  /**
   * La celda con el foco del teclado y el extremo fijo del rango, por posición dentro de `ordered`.
   * Se guarda la posición y no la identidad de la fila porque un rango es un rectángulo de la
   * pantalla: al reordenar deja de significar lo mismo, y se descarta.
   */
  let cursor = $state<{ row: number; column: number } | null>(null);
  let anchor = $state<{ row: number; column: number } | null>(null);

  /** El valor abierto en el visor, el menú de copiar y el aviso de que algo se copió. */
  let viewer = $state<{ header: string; text: string | null } | null>(null);
  let menu = $state<{ x: number; y: number } | null>(null);
  let flash = $state<string | null>(null);
  let flashTimer: ReturnType<typeof setTimeout> | null = null;

  function widthOf(column: Column<T>): number {
    return widths[column.key] ?? column.width;
  }

  /**
   * Las columnas que de verdad se dibujan. Todo lo demás cuenta sobre esta lista —el foco de celda,
   * el rango, la ventana horizontal—, así que esconder una columna corre los índices en vez de
   * dejar un hueco al que se pueda llegar con las flechas.
   */
  const active = $derived.by(() => {
    const visible = columns.filter((column) => !hidden[column.key]);
    if (order.length === 0) return visible;

    // Lo que no está en `order` es una columna nueva o una que se acaba de destapar: va al final.
    const byKey = new Map(visible.map((column) => [column.key, column]));
    const result: Column<T>[] = [];
    for (const key of order) {
      const column = byKey.get(key);
      if (column) {
        result.push(column);
        byKey.delete(key);
      }
    }
    result.push(...byKey.values());
    return result;
  });

  /** Cuántas quedan fijas. Fijarlas todas dejaría la grilla sin nada que desplazar. */
  const pinned = $derived(Math.min(frozen, Math.max(0, active.length - 1)));

  const needle = $derived(filter.trim().toLowerCase());

  /**
   * El filtro mira lo que se ve, no el valor entero: es para encontrar la fila que uno tiene en la
   * cabeza entre las que ya están cargadas, no para consultar de nuevo. Lo que no se trajo del
   * servidor no aparece por filtrar.
   */
  const matched = $derived.by(() => {
    if (needle === "") return rows;
    return rows.filter((row) =>
      active.some((column) => column.value(row).toLowerCase().includes(needle)),
    );
  });

  const ordered = $derived.by(() => {
    // Con el orden delegado, las filas ya llegan ordenadas: reordenarlas acá las mezclaría con las
    // de la tanda siguiente.
    if (!sort || serverSort) return matched;
    const column = active.find((item) => item.key === sort!.key);
    if (!column) return matched;

    const value = (row: T) => (column.sort ? column.sort(row) : column.value(row));
    const direction = sort.descending ? -1 : 1;

    return [...matched].sort((left, right) => {
      const a = value(left);
      const b = value(right);
      if (typeof a === "number" && typeof b === "number") return (a - b) * direction;
      return String(a).localeCompare(String(b), "es", { numeric: true }) * direction;
    });
  });

  const start = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN));
  const visible = $derived(
    ordered.slice(start, start + Math.ceil(viewportHeight / rowHeight) + OVERSCAN * 2),
  );
  const totalWidth = $derived(active.reduce((sum, column) => sum + widthOf(column), 0));

  /** Dónde empieza cada columna: lo usan la ventana horizontal, las fijas y el foco de celda. */
  const columnOffsets = $derived.by(() => {
    const out: number[] = [];
    let left = 0;
    for (const column of active) {
      out.push(left);
      left += widthOf(column);
    }
    return out;
  });

  /** Las fijas se dibujan siempre y van pegadas al borde: no entran en la ventana que se desplaza. */
  const frozenColumns = $derived(
    active.slice(0, pinned).map((column, index) => ({ column, index })),
  );

  /** Qué columnas se dibujan y cuánto espacio hay que dejar por las que quedaron a la izquierda. */
  const columnWindow = $derived.by(() => {
    const range = columnRange(
      columnOffsets,
      active.map(widthOf),
      scrollLeft,
      viewportWidth,
      COLUMN_OVERSCAN,
    );
    // Nunca antes de las fijas: esas ya están dibujadas y repetirlas las mostraría dos veces.
    const first = Math.max(range.first, pinned);
    return {
      first,
      last: range.last,
      pad: Math.max(0, (columnOffsets[first] ?? 0) - (columnOffsets[pinned] ?? 0)),
    };
  });

  /** Las columnas dibujadas, cada una con su posición real: el resto del componente usa esa. */
  const shownColumns = $derived(
    active
      .slice(columnWindow.first, columnWindow.last + 1)
      .map((column, index) => ({ column, index: columnWindow.first + index })),
  );

  /** El rectángulo elegido. Sin `anchor` es una sola celda, que es el caso normal. */
  const range = $derived.by(() => {
    if (!cursor) return null;
    const other = anchor ?? cursor;
    return {
      top: Math.min(cursor.row, other.row),
      bottom: Math.max(cursor.row, other.row),
      left: Math.min(cursor.column, other.column),
      right: Math.max(cursor.column, other.column),
    };
  });

  function inRange(row: number, column: number): boolean {
    if (!range) return false;
    return row >= range.top && row <= range.bottom && column >= range.left && column <= range.right;
  }

  /**
   * Cuántas celdas hay elegidas y, si son números, en qué terminan.
   *
   * Solo aparece con más de una celda: para una sola, el valor ya está a la vista. Es la pregunta
   * que uno le hace a una selección —cuántas son, cuánto suman— y que hoy obliga a exportar a una
   * planilla para responderla.
   */
  const summary = $derived.by(() => {
    if (!range) return null;
    const cells = (range.bottom - range.top + 1) * (range.right - range.left + 1);
    if (cells < 2) return null;

    let count = 0;
    let sum = 0;
    let min = Infinity;
    let max = -Infinity;

    for (let row = range.top; row <= range.bottom; row++) {
      const item = ordered[row];
      if (!item) continue;
      for (let column = range.left; column <= range.right; column++) {
        const text = rawOf(item, active[column])?.trim();
        if (!text) continue;
        const value = Number(text);
        if (!Number.isFinite(value)) continue;
        count += 1;
        sum += value;
        min = Math.min(min, value);
        max = Math.max(max, value);
      }
    }

    return { cells, count, sum, min, max };
  });

  /** Los promedios y las sumas no son enteros: se muestran con lo que haga falta y nada más. */
  const num = (value: number) => value.toLocaleString("es", { maximumFractionDigits: 4 });

  const anyHidden = $derived(columns.length !== active.length);

  /** Fija las columnas hasta la `index` inclusive; repetirlo sobre la misma las suelta. */
  function pinTo(index: number) {
    frozen = pinned === index + 1 ? 0 : index + 1;
    headerMenu = null;
  }

  function hide(column: Column<T>) {
    // Esconder la última dejaría una grilla sin nada, y sin encabezado donde pedir que vuelva.
    if (active.length <= 1) return;
    hidden[column.key] = true;
    headerMenu = null;
    // El foco apuntaba a una posición de una lista que acaba de acortarse.
    cursor = null;
    anchor = null;
  }

  function showAll() {
    hidden = {};
    headerMenu = null;
  }

  /** Mueve una columna a la posición de otra, arrastrando el encabezado. */
  function moveColumn(from: string | null, to: string) {
    if (!from || from === to) return;
    const keys = active.map((column) => column.key);
    const fromIndex = keys.indexOf(from);
    const toIndex = keys.indexOf(to);
    if (fromIndex === -1 || toIndex === -1) return;

    keys.splice(toIndex, 0, keys.splice(fromIndex, 1)[0]);
    order = keys;
    // El rango elegido era un rectángulo sobre el orden anterior: ya no señala esas columnas.
    cursor = null;
    anchor = null;
  }

  function onHeaderMenu(event: MouseEvent, index: number) {
    event.preventDefault();
    menu = null;
    headerMenu = { x: event.clientX, y: event.clientY, index };
  }

  function openFilter() {
    filterOpen = true;
    filterBox?.focus();
  }

  function closeFilter() {
    filterOpen = false;
    filter = "";
    viewport?.focus();
  }

  let frame = 0;

  /**
   * El desplazamiento se lee una vez por cuadro. `scroll` llega varias veces entre dos dibujos —con
   * una rueda de precisión, decenas—, y atender cada uno rehacía la ventana entera para un
   * desplazamiento que la pantalla nunca llegó a mostrar.
   */
  function onScroll(event: Event & { currentTarget: HTMLDivElement }) {
    const element = event.currentTarget;
    if (frame) return;

    frame = requestAnimationFrame(() => {
      frame = 0;
      scrollTop = element.scrollTop;
      scrollLeft = element.scrollLeft;
      // Un menú abierto se queda donde estaba la celda: se cierra en vez de quedar flotando.
      menu = null;

      const lastVisible = Math.ceil((scrollTop + viewportHeight) / rowHeight);
      if (onnearend && ordered.length - lastVisible < NEAR_END) {
        onnearend();
      }
    });
  }

  $effect(() => () => {
    if (frame) cancelAnimationFrame(frame);
  });

  /** Una columna que no existe del lado del servidor no se puede ordenar allá. */
  const canSort = (column: Column<T>) =>
    sortable && (!serverSort || column.orderBy !== undefined);

  function toggleSort(column: Column<T>) {
    if (!canSort(column)) return;
    // Ascendente, descendente y sin orden: el tercer clic devuelve la grilla a como llegó.
    if (sort?.key !== column.key) sort = { key: column.key, descending: false };
    else if (!sort.descending) sort = { key: column.key, descending: true };
    else sort = null;

    onsort?.(sort ? column : null, sort?.descending ?? false);

    // El rango elegido era un rectángulo sobre el orden anterior: ya no señala esos datos.
    cursor = null;
    anchor = null;
  }

  function startResize(event: MouseEvent, column: Column<T>) {
    event.preventDefault();
    event.stopPropagation();
    const origin = event.clientX;
    const initial = widthOf(column);

    const move = (moved: MouseEvent) => {
      widths[column.key] = Math.max(MIN_COLUMN, initial + moved.clientX - origin);
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      document.body.classList.remove("cursor-col-resize");
    };
    document.body.classList.add("cursor-col-resize");
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }

  let ruler: HTMLCanvasElement | null = null;

  /**
   * Ancho que necesita la columna para lo que hoy tiene adentro.
   *
   * Se mide con un canvas y no montando texto en el documento: medir así una columna de mil filas
   * cuesta un cuadro, y hacerlo con el DOM cuesta un reflujo por fila. La fuente se lee de una celda
   * de verdad, porque la grilla no fija la suya: la hereda de la ventana.
   */
  function fitWidth(column: Column<T>): number {
    const context = (ruler ??= document.createElement("canvas")).getContext("2d");
    const cell = viewport?.querySelector<HTMLElement>("[data-cell]");
    if (!context || !cell) return column.width;

    const style = getComputedStyle(cell);
    context.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;

    let longest = context.measureText(column.header).width + 18;
    for (const row of ordered.slice(0, FIT_SAMPLE)) {
      longest = Math.max(longest, context.measureText(column.value(row)).width);
    }
    // El relleno de la celda (8 px de cada lado) más el aire que evita que el texto toque el borde.
    return Math.min(MAX_FIT, Math.max(MIN_COLUMN, Math.ceil(longest) + 20));
  }

  /** Deja la fila `index` a la vista; la usa la navegación con flechas. */
  function reveal(index: number) {
    if (!viewport) return;
    const top = index * rowHeight;
    if (top < scrollTop) viewport.scrollTop = top;
    else if (top + rowHeight > scrollTop + viewportHeight) {
      viewport.scrollTop = top + rowHeight - viewportHeight;
    }
  }

  /** Lo mismo en horizontal: sin esto, moverse a la columna veinte no mueve la pantalla. */
  function revealColumn(index: number) {
    if (!viewport) return;
    const left = columnOffsets[index] ?? 0;
    const width = widthOf(active[index]);
    if (left < viewport.scrollLeft) viewport.scrollLeft = left;
    else if (left + width > viewport.scrollLeft + viewport.clientWidth) {
      viewport.scrollLeft = left + width - viewport.clientWidth;
    }
  }

  function clampIndex(value: number, last: number): number {
    return Math.min(last, Math.max(0, value));
  }

  /** Mueve el foco de celda. `extend` estira el rango en vez de empezar otro. */
  function moveCursor(rowDelta: number, columnDelta: number, extend: boolean) {
    if (ordered.length === 0 || active.length === 0) return;

    const from = cursor ?? { row: 0, column: 0 };
    const next = {
      row: clampIndex(from.row + rowDelta, ordered.length - 1),
      column: clampIndex(from.column + columnDelta, active.length - 1),
    };

    if (extend) anchor ??= from;
    else anchor = null;

    cursor = next;
    reveal(next.row);
    revealColumn(next.column);
    onselect?.(ordered[next.row]);
  }

  /** Elige una celda con el mouse. Con `Shift` estira el rango desde donde estaba el foco. */
  function pick(row: number, column: number, extend: boolean) {
    if (extend) anchor ??= (cursor ?? { row, column });
    else anchor = null;
    cursor = { row, column };
    onselect?.(ordered[row]);
  }

  function onGridKey(event: KeyboardEvent) {
    // Con una celda abierta las flechas son del editor de texto, no de la grilla.
    if (editing) return;

    const extend = event.shiftKey;

    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        moveCursor(1, 0, extend);
        return;
      case "ArrowUp":
        event.preventDefault();
        moveCursor(-1, 0, extend);
        return;
      case "ArrowRight":
        event.preventDefault();
        moveCursor(0, 1, extend);
        return;
      case "ArrowLeft":
        event.preventDefault();
        moveCursor(0, -1, extend);
        return;
      case "PageDown":
        event.preventDefault();
        moveCursor(Math.floor(viewportHeight / rowHeight), 0, extend);
        return;
      case "PageUp":
        event.preventDefault();
        moveCursor(-Math.floor(viewportHeight / rowHeight), 0, extend);
        return;
      case "Home":
        event.preventDefault();
        moveCursor(-ordered.length, event.ctrlKey ? -active.length : 0, extend);
        return;
      case "End":
        event.preventDefault();
        moveCursor(ordered.length, event.ctrlKey ? active.length : 0, extend);
        return;
      case "Escape":
        if (menu || headerMenu) {
          menu = null;
          headerMenu = null;
        } else if (filterOpen) {
          closeFilter();
        } else {
          cursor = null;
          anchor = null;
        }
        return;
      case "f":
        if (!event.ctrlKey && !event.metaKey) return;
        event.preventDefault();
        openFilter();
        return;
      case " ":
        event.preventDefault();
        openViewer();
        return;
      case "Enter":
      case "F2": {
        if (!cursor) return;
        const row = ordered[cursor.row];
        const column = active[cursor.column];
        if (row && column && editable?.(row, column)) {
          event.preventDefault();
          open(row, column);
        }
        return;
      }
      case "a":
        if (!event.ctrlKey && !event.metaKey) return;
        event.preventDefault();
        if (ordered.length === 0 || active.length === 0) return;
        anchor = { row: 0, column: 0 };
        cursor = { row: ordered.length - 1, column: active.length - 1 };
        return;
      case "c":
        if (!event.ctrlKey && !event.metaKey) return;
        event.preventDefault();
        copySelection(event.shiftKey ? "csv" : "tsv");
        return;
      case "v":
        if (!event.ctrlKey && !event.metaKey) return;
        event.preventDefault();
        void pasteSelection();
        return;
    }
  }

  /**
   * El valor entero de una celda. La grilla muestra texto de una línea y recortado; para copiar y
   * para el visor hace falta el dato como vino.
   */
  function rawOf(row: T, column: Column<T>): string | null {
    if (column.raw) return column.raw(row);
    if (column.edit) return column.edit(row);
    return column.value(row);
  }

  /** Las columnas elegidas y los valores del rango, que es lo que consume `grid-copy`. */
  function selection(): { headers: string[]; cells: (string | null)[][] } {
    if (!range) return { headers: [], cells: [] };
    const chosen = active.slice(range.left, range.right + 1);

    const cells: (string | null)[][] = [];
    for (let index = range.top; index <= range.bottom; index++) {
      const row = ordered[index];
      if (row) cells.push(chosen.map((column) => rawOf(row, column)));
    }
    return { headers: chosen.map((column) => column.header), cells };
  }

  async function toClipboard(text: string, message: string) {
    menu = null;
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      notify(message);
    } catch {
      // El portapapeles lo puede negar el sistema; decirlo es mejor que no hacer nada visible.
      notify("No se pudo copiar");
    }
  }

  function copySelection(format: "tsv" | "csv") {
    const { headers, cells } = selection();
    // El CSV lleva encabezados porque termina en un archivo; el TSV se pega en una planilla que ya
    // los tiene.
    const text =
      format === "csv" ? delimited(headers, cells, ",") : delimited(null, cells, "\t");
    return toClipboard(text, format === "csv" ? "Copiado con encabezados" : "Copiado");
  }

  function copyJson() {
    const { headers, cells } = selection();
    return toClipboard(asJson(headers, cells), "Copiado como JSON");
  }

  /**
   * Pega un bloque del portapapeles a partir de la celda con foco, como cualquier planilla. Una
   * celda vacía escribe NULL y no cadena vacía: mismo criterio que ya usa `grid-copy.ts` al copiar,
   * para que copiar y pegar adentro de la aplicación redondeen sin inventar un valor.
   */
  async function pasteSelection() {
    menu = null;
    if (!onedit || !cursor) return;

    let text: string;
    try {
      text = await navigator.clipboard.readText();
    } catch {
      notify("No se pudo leer el portapapeles");
      return;
    }
    if (!text) return;

    const grid = parseDelimited(text, "\t");
    const top = cursor.row;
    const left = cursor.column;
    let rowsApplied = 0;

    for (let r = 0; r < grid.length; r++) {
      const row = ordered[top + r];
      if (!row) break;
      rowsApplied++;
      for (let c = 0; c < grid[r].length; c++) {
        const column = active[left + c];
        if (!column || !editable?.(row, column)) continue;
        const value = grid[r][c];
        onedit(row, column, value === "" ? null : value);
      }
    }
    if (rowsApplied === 0) return;

    // Deja ver exactamente qué rango se pegó, igual que hace cualquier planilla.
    const cols = Math.min(Math.max(...grid.map((item) => item.length)), active.length - left);
    cursor = { row: top, column: left };
    anchor =
      rowsApplied > 1 || cols > 1 ? { row: top + rowsApplied - 1, column: left + cols - 1 } : null;
    notify(`Pegado: ${rowsApplied} ${rowsApplied === 1 ? "fila" : "filas"}`);
  }

  function notify(message: string) {
    flash = message;
    if (flashTimer) clearTimeout(flashTimer);
    flashTimer = setTimeout(() => (flash = null), 1500);
  }

  function openViewer() {
    if (!cursor) return;
    const row = ordered[cursor.row];
    const column = active[cursor.column];
    if (!row || !column) return;
    menu = null;
    viewer = { header: column.header, text: rawOf(row, column) };
  }

  const viewerText = $derived(viewer?.text ? pretty(viewer.text) : "");

  function onCellMenu(event: MouseEvent, row: number, column: number) {
    event.preventDefault();
    // Con el clic derecho afuera del rango, lo que se quiere copiar es la celda apuntada.
    if (!inRange(row, column)) pick(row, column, false);
    menu = { x: event.clientX, y: event.clientY };
  }

  function open(row: T, column: Column<T>) {
    if (!editable?.(row, column)) return;

    const value = column.edit ? column.edit(row) : column.value(row);
    editing = { row: rowKey(row), column: column.key };
    draftNull = value === null;
    draft = value ?? "";
  }

  function commit(row: T, column: Column<T>) {
    if (!editing) return;
    onedit?.(row, column, draftNull ? null : draft);
    editing = null;
  }

  function onKey(event: KeyboardEvent, row: T, column: Column<T>) {
    if (event.key === "Enter") {
      event.preventDefault();
      commit(row, column);
    } else if (event.key === "Escape") {
      event.preventDefault();
      editing = null;
    }
  }
</script>

<svelte:window
  onclick={() => {
    menu = null;
    headerMenu = null;
  }}
/>

<!--
  Una columna del encabezado. Va en un fragmento porque se dibuja en dos lugares: las fijas quedan
  pegadas al borde izquierdo y las demás entran y salen con la ventana que se desplaza.
-->
{#snippet headerCell(column: Column<T>, index: number, sticky: boolean)}
  {@const sorted = sort?.key === column.key}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="group relative shrink-0 border-r border-zinc-200/70 dark:border-zinc-800/70
           {sticky ? 'panel sticky z-20' : ''} {column.align === 'right' ? 'text-right' : ''}"
    style="width: {widthOf(column)}px; left: {sticky ? columnOffsets[index] : 0}px"
    oncontextmenu={(event) => onHeaderMenu(event, index)}
  >
    <button
      class="w-full truncate px-2 py-1 text-left {column.align === 'right'
        ? 'text-right'
        : ''} {canSort(column) ? 'hover:text-zinc-900 dark:hover:text-zinc-100' : 'cursor-default'}
        {sorted ? 'text-zinc-900 dark:text-zinc-100' : ''}
        {dragKey === column.key ? 'opacity-50' : ''}
        {dragOverKey === column.key ? 'bg-blue-100 dark:bg-blue-900/40' : ''}"
      title="{column.header}{canSort(column) ? ' — clic para ordenar' : ''} — clic derecho para
        fijar u ocultar{sticky ? '' : ' — arrastrá para reordenar'}"
      tabindex="-1"
      draggable={!sticky}
      onclick={() => toggleSort(column)}
      ondragstart={() => (dragKey = column.key)}
      ondragover={(event) => {
        if (!dragKey || dragKey === column.key) return;
        event.preventDefault();
        dragOverKey = column.key;
      }}
      ondragleave={() => {
        if (dragOverKey === column.key) dragOverKey = null;
      }}
      ondrop={(event) => {
        event.preventDefault();
        moveColumn(dragKey, column.key);
        dragKey = null;
        dragOverKey = null;
      }}
      ondragend={() => {
        dragKey = null;
        dragOverKey = null;
      }}
    >
      {#if sticky}
        <Icon name="lock" size={9} class="mr-0.5 inline-block opacity-60" />
      {/if}
      {column.header}
      {#if sorted}
        <Icon
          name="chevron"
          size={9}
          class="ml-0.5 inline-block {sort?.descending ? '-rotate-90' : 'rotate-90'}"
        />
      {/if}
    </button>

    <!--
      El tirador del ancho: dos píxeles a la vista, seis para el mouse. Es lo primero que se
      busca cuando una columna corta justo el valor que hay que leer. El doble clic ajusta al
      contenido, que es lo que hace cualquier planilla; con `Alt`, vuelve al ancho de origen.
    -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="absolute inset-y-0 -right-[3px] z-10 w-[6px] cursor-col-resize hover:bg-blue-500/40"
      title="Arrastrá para cambiar el ancho; doble clic lo ajusta al contenido"
      onmousedown={(event) => startResize(event, column)}
      ondblclick={(event) => {
        if (event.altKey) delete widths[column.key];
        else widths[column.key] = fitWidth(column);
      }}
    ></div>
  </div>
{/snippet}

<!-- Una celda del cuerpo, con la misma división entre fijas y desplazables. -->
{#snippet bodyCell(
  row: T,
  at: number,
  key: string | number,
  column: Column<T>,
  columnIndex: number,
  sticky: boolean,
)}
  {#if editing && editing.row === key && editing.column === column.key}
    <!-- svelte-ignore a11y_autofocus -->
    <div
      class="flex shrink-0 items-center gap-1 bg-white px-1 ring-2 ring-blue-500 dark:bg-zinc-900
             {sticky ? 'sticky z-10' : ''}"
      style="width: {widthOf(column)}px; height: {rowHeight}px;
             left: {sticky ? columnOffsets[columnIndex] : 0}px"
    >
      <input
        class="w-full min-w-0 bg-transparent outline-none
               {draftNull ? 'italic text-zinc-400' : ''}"
        style="font-size: var(--grid-font-size, 0.875rem)"
        value={draftNull ? "[null]" : draft}
        autofocus
        oninput={(event) => {
          draft = event.currentTarget.value;
          draftNull = false;
        }}
        onkeydown={(event) => onKey(event, row, column)}
        onblur={() => commit(row, column)}
      />
      <!--
        Sin un botón, un NULL sería imposible de escribir: cualquier texto que uno teclee es una
        cadena, y la vacía no es lo mismo que la ausencia de valor.
      -->
      <button
        class="shrink-0 rounded px-1 text-[10px] text-zinc-400 hover:bg-zinc-200
               hover:text-zinc-700 dark:hover:bg-zinc-700 dark:hover:text-zinc-100"
        title="Poner NULL"
        tabindex="-1"
        onmousedown={(event) => {
          // `mousedown` y no `click`: el `blur` del input confirmaría antes.
          event.preventDefault();
          draftNull = true;
          draft = "";
        }}
      >
        ∅
      </button>
    </div>
  {:else}
    {@const focused = cursor?.row === at && cursor?.column === columnIndex}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      data-cell
      class="shrink-0 truncate border-r border-zinc-200/70 px-2 py-0.5 dark:border-zinc-800/70
        {column.align === 'right' ? 'text-right tabular-nums' : ''} {column.tone?.(row) ?? ''}
        {editable?.(row, column) ? 'cursor-text' : ''}
        {sticky ? 'sticky z-10 bg-inherit' : ''}
        {inRange(at, columnIndex) && !focused ? 'bg-blue-100/60 dark:bg-blue-900/30' : ''}
        {focused ? 'bg-blue-100/60 ring-1 ring-blue-500 ring-inset dark:bg-blue-900/30' : ''}"
      style="width: {widthOf(column)}px; left: {sticky ? columnOffsets[columnIndex] : 0}px"
      title={column.title?.(row)}
      onclick={(event) => pick(at, columnIndex, event.shiftKey)}
      ondblclick={() => open(row, column)}
      oncontextmenu={(event) => onCellMenu(event, at, columnIndex)}
    >
      {column.value(row)}
    </div>
  {/if}
{/snippet}

<div class="relative flex h-full flex-col">
  <!--
    El filtro se abre con Ctrl+F y solo alcanza a lo que ya está cargado. Que aparezca en vez de
    estar siempre es a propósito: una caja de texto arriba de la grilla se lee como «buscar en la
    tabla», que es otra cosa —eso se hace con un WHERE—.
  -->
  {#if filterOpen}
    <div class="divider-b flex items-center gap-2 px-2 py-1">
      <Icon name="search" size={12} class="muted" />
      <!-- svelte-ignore a11y_autofocus -->
      <input
        bind:this={filterBox}
        bind:value={filter}
        class="field min-w-0 flex-1 py-0.5 text-xs"
        placeholder="Filtrar entre las filas cargadas"
        autofocus
        onkeydown={(event) => {
          if (event.key === "Escape") closeFilter();
        }}
      />
      <span class="shrink-0 text-xs tabular-nums muted">
        {count(ordered.length)} de {count(rows.length)}
      </span>
      <button
        class="btn btn-ghost btn-icon size-6"
        aria-label="Cerrar el filtro"
        onclick={closeFilter}
      >
        <Icon name="close" size={11} />
      </button>
    </div>
  {/if}

  <div
    bind:this={viewport}
    class="relative min-h-0 flex-1 overflow-auto outline-none"
    onscroll={onScroll}
    onkeydown={onGridKey}
    onwheel={(event) => {
      // Ctrl + rueda es el gesto que todos prueban antes de buscar el botón (mismo atajo que el
      // tamaño de letra del SQL en `SqlEditor.svelte`).
      if (!event.ctrlKey) return;
      event.preventDefault();
      if (event.deltaY < 0) gridZoom.bigger();
      else gridZoom.smaller();
    }}
    bind:clientHeight={viewportHeight}
    bind:clientWidth={viewportWidth}
    role="grid"
    tabindex="0"
  >
    <div style="width: {totalWidth}px">
      <div
        class="panel divider-b sticky top-0 z-10 flex font-medium muted"
        style="font-size: var(--grid-header-font-size, 0.75rem)"
      >
        {#each frozenColumns as { column, index } (column.key)}
          {@render headerCell(column, index, true)}
        {/each}

        <!-- El hueco de las columnas que quedaron a la izquierda de la ventana. -->
        <div class="shrink-0" style="width: {columnWindow.pad}px"></div>

        {#each shownColumns as { column, index } (column.key)}
          {@render headerCell(column, index, false)}
        {/each}
      </div>

      {#if ordered.length > 0}
        <div class="relative" style="height: {ordered.length * rowHeight}px">
          <!--
            Las filas dibujadas van en un bloque que se corre entero con `transform`, en vez de
            llevar cada una su `top`: mover una capa ya compuesta no cuesta nada, y posicionar
            cuarenta filas a mano rehace la disposición en cada cuadro del desplazamiento.

            La clave es la posición absoluta de la fila, no su identidad. En una lista con ventana
            deslizante los nodos se reciclan por posición de todos modos, y una clave derivada de
            los datos obliga a que sean únicos: pg_stat_statements, por ejemplo, distingue sus filas
            por (usuario, base, queryid) y repite el texto de la consulta, lo que hacía abortar el
            render.
          -->
          <div
            class="absolute inset-x-0 top-0 will-change-transform"
            style="transform: translateY({start * rowHeight}px)"
          >
            {#each visible as row, index (start + index)}
              {@const at = start + index}
              {@const key = rowKey(row)}
              {@const selected = selectedKey === key}
              <!--
                El fondo de la fila es opaco a propósito: las columnas fijas lo heredan con
                `bg-inherit`, y con un fondo a medio pintar se les vería por debajo lo que pasa de
                largo al desplazar.
              -->
              <div
                class="flex
                   {selected
                  ? 'bg-blue-100 text-blue-950 dark:bg-blue-950 dark:text-blue-100'
                  : at % 2 === 1
                    ? 'bg-zinc-50 hover:bg-zinc-100 dark:bg-zinc-900 dark:hover:bg-zinc-800'
                    : 'bg-white hover:bg-zinc-100 dark:bg-zinc-950 dark:hover:bg-zinc-800'}
                   {rowClass?.(row) ?? ''}"
                style="height: {rowHeight}px; width: {totalWidth}px;
                       font-size: var(--grid-font-size, 0.875rem)"
                role="row"
                tabindex="-1"
              >
                {#each frozenColumns as { column, index: columnIndex } (column.key)}
                  {@render bodyCell(row, at, key, column, columnIndex, true)}
                {/each}

                <div class="shrink-0" style="width: {columnWindow.pad}px"></div>

                {#each shownColumns as { column, index: columnIndex } (column.key)}
                  {@render bodyCell(row, at, key, column, columnIndex, false)}
                {/each}
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </div>

    <!--
      El vacío queda fuera del contenedor de ancho fijo y pegado a la izquierda: si estuviera dentro,
      en una grilla de veinte columnas el mensaje aparecería a dos mil píxeles de donde se está
      mirando.
    -->
    {#if ordered.length === 0}
      <div class="sticky left-0 flex w-full justify-center py-6">
        <Empty
          icon={needle === "" ? "table" : "search"}
          title={needle === "" ? empty : "Ninguna fila cargada coincide"}
          hint={needle === ""
            ? undefined
            : "El filtro solo alcanza a lo que ya se trajo del servidor."}
        />
      </div>
    {/if}
  </div>

  <!--
    Lo que se preguntaría de una selección —cuántas celdas son, cuánto suman— sin tener que llevarla
    a una planilla para averiguarlo.
  -->
  {#if summary}
    <div
      class="divider-t flex flex-wrap items-center gap-x-3 px-2 py-0.5 text-[11px] tabular-nums
             muted"
    >
      <span>{count(summary.cells)} celdas</span>
      {#if summary.count > 0}
        <span>suma {num(summary.sum)}</span>
        <span>promedio {num(summary.sum / summary.count)}</span>
        <span>mín {num(summary.min)}</span>
        <span>máx {num(summary.max)}</span>
        <span>({count(summary.count)} numéricas)</span>
      {/if}
    </div>
  {/if}

  {#if flash}
    <div class="pointer-events-none absolute right-3 bottom-3 z-20">
      <span class="tag tag-neutral shadow-sm">{flash}</span>
    </div>
  {/if}
</div>

<!--
  El menú de copiar va fijo a la ventana y no dentro de la grilla: adentro quedaría recortado por el
  desplazamiento en cuanto se abriera cerca del borde.
-->
{#if menu}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="card fixed z-40 min-w-44 p-1 text-sm shadow-lg"
    style="left: {Math.min(menu.x, window.innerWidth - 200)}px;
           top: {Math.min(menu.y, window.innerHeight - 180)}px"
    role="menu"
    tabindex="-1"
    onclick={(event) => event.stopPropagation()}
  >
    <button class="row-menu" onclick={() => copySelection("tsv")}>
      Copiar <span class="muted">Ctrl+C</span>
    </button>
    <button class="row-menu" onclick={() => copySelection("csv")}>
      Copiar con encabezados <span class="muted">Ctrl+Shift+C</span>
    </button>
    <button class="row-menu" onclick={copyJson}>Copiar como JSON</button>
    <button class="row-menu" disabled={!onedit} onclick={pasteSelection}>
      Pegar <span class="muted">Ctrl+V</span>
    </button>
    <button class="row-menu" onclick={openViewer}>
      Ver el valor <span class="muted">Espacio</span>
    </button>
    <button class="row-menu" onclick={openFilter}>
      Filtrar las filas <span class="muted">Ctrl+F</span>
    </button>
  </div>
{/if}

<!-- Lo que se puede hacer sobre una columna. Va en el clic derecho del encabezado y no en un botón
     por columna: son treinta columnas y el botón estaría treinta veces a la vista sin usarse. -->
{#if headerMenu}
  {@const column = active[headerMenu.index]}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="card fixed z-40 min-w-52 p-1 text-sm shadow-lg"
    style="left: {Math.min(headerMenu.x, window.innerWidth - 230)}px;
           top: {Math.min(headerMenu.y, window.innerHeight - 180)}px"
    role="menu"
    tabindex="-1"
    onclick={(event) => event.stopPropagation()}
  >
    <button class="row-menu" onclick={() => pinTo(headerMenu!.index)}>
      {pinned === headerMenu.index + 1 ? "No fijar ninguna" : "Fijar hasta esta columna"}
    </button>
    <button class="row-menu" disabled={active.length <= 1} onclick={() => hide(column)}>
      Ocultar esta columna
    </button>
    <button class="row-menu" disabled={!anyHidden} onclick={showAll}>
      Mostrar todas las columnas
    </button>
    <button
      class="row-menu"
      onclick={() => {
        widths[column.key] = fitWidth(column);
        headerMenu = null;
      }}
    >
      Ajustar el ancho al contenido
    </button>
  </div>
{/if}

{#if viewer}
  <Modal
    title={viewer.header}
    subtitle={viewer.text === null ? "NULL" : `${viewer.text.length} caracteres`}
    size="lg"
    onclose={() => (viewer = null)}
  >
    {#if viewer.text === null}
      <p class="italic muted">NULL — la fila no tiene valor en esta columna.</p>
    {:else}
      <pre
        class="max-h-[60vh] overflow-auto rounded bg-zinc-50 p-3 font-mono text-xs break-words
               whitespace-pre-wrap select-text dark:bg-zinc-900/60">{viewerText}</pre>
    {/if}

    {#snippet footer()}
      <button
        class="btn btn-primary ml-auto"
        disabled={viewer?.text === null}
        onclick={() => toClipboard(viewer?.text ?? "", "Copiado")}
      >
        <Icon name="copy" size={12} />
        Copiar
      </button>
      <button class="btn" onclick={() => (viewer = null)}>Cerrar</button>
    {/snippet}
  </Modal>
{/if}
