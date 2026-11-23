<script lang="ts">
  import { t } from "./i18n.svelte";
  import DataGrid, { type Column } from "./DataGrid.svelte";
  import { boolText, isBoolType } from "./format";
  import { geometryText, geometryTextOrNull, isGeometryType } from "./geometry";
  import { columnWidth, gutterWidth } from "./grid-width";
  import { gridZoom } from "./grid.svelte";
  import { timeZone } from "./tz.svelte";
  import { numberFormat } from "./numfmt.svelte";

  let {
    columns,
    rows,
    types = null,
    editing = null,
  }: {
    columns: string[];
    rows: (string | null)[][];
    /** Tipos de cada columna, cuando se pidieron. Van como texto secundario del encabezado, como
     * en la pestaña de datos: al leer un resultado, saber si eso es un `numeric` o un `text` cambia
     * lo que dice. */
    types?: string[] | null;
    /**
     * Modo de edición del resultado: qué columnas se pueden tocar, lo ya cambiado y a quién avisar.
     * Sin esto la grilla es de solo lectura, que es como se usa casi siempre.
     */
    editing?: {
      editable: boolean[];
      edits: ReadonlyMap<number, ReadonlyMap<number, string | null>>;
      onedit: (row: number, column: number, value: string | null) => void;
    } | null;
  } = $props();

  /** Lo que se muestra en lugar de un NULL, que no es lo mismo que una celda vacía. */
  const NULL = "NULL";

  /**
   * `DataGrid` no mide el contenido a propósito: sus columnas son parte de la definición. Acá la
   * definición se arma en el momento, así que el ancho hay que estimarlo, y se estima con una
   * muestra: recorrer cien mil filas para elegir un ancho costaría más que dibujarlas.
   */
  const SAMPLE = 50;
  const MIN_WIDTH = 64;
  const MAX_WIDTH = 340;

  interface Numbered {
    index: number;
    cells: (string | null)[];
  }

  // La fila viaja numerada en vez de buscar su posición al dibujarla: con la ventana deslizante,
  // un `indexOf` por celda visible recorrería la tabla entera en cada cuadro.
  const numbered = $derived(rows.map((cells, index) => ({ index, cells })));

  /** El valor de una celda: lo editado si se tocó, si no lo que trajo el servidor. */
  function shown(row: Numbered, index: number): string | null {
    const cells = editing?.edits.get(row.index);
    return cells?.has(index) ? (cells.get(index) ?? null) : row.cells[index];
  }

  /** La clave de las columnas de datos es `<posición>-<nombre>`; la del «#» no tiene posición. */
  function positionOf(column: Column<Numbered>): number {
    return Number(column.key.split("-")[0]);
  }

  const definitions = $derived.by<Column<Numbered>[]>(() => {
    const sample = rows.slice(0, SAMPLE);

    const cells: Column<Numbered>[] = columns.map((name, index) => {
      // El tipo lo trae el interruptor «Tipos», que viene encendido. Sin él, una columna de textos
      // que dijeran «t» y «f» se traduciría sola, que es peor que no traducir ninguna.
      const bool = isBoolType(types?.[index]);
      // Con el tipo conocido (interruptor «Tipos») se decide en la columna entera; sin él, cada
      // valor se prueba solo — una consulta sin tipos puede mezclar una columna real de geometría
      // con cualquier otro texto que por casualidad decodifique, así que no alcanza con mirar uno.
      const columnType = types?.[index];
      const geometryColumn = isGeometryType(columnType);
      const longest = sample.reduce(
        (max, row) => Math.max(max, (row[index] ?? NULL).length),
        name.length,
      );

      // Una columna de puros números: se alinea a la derecha y admite el separador de miles.
      const numericColumn = numeric(sample, index);

      return {
        key: `${index}-${name}`,
        header: name,
        caption: types?.[index] ?? undefined,
        // La letra de la grilla es una preferencia (`gridZoom`), así que el ancho se calcula con
        // ella: con la letra más grande, la misma cantidad de caracteres ocupa más.
        width: columnWidth({
          longest,
          fontSize: gridZoom.size,
          min: MIN_WIDTH,
          max: MAX_WIDTH,
          padding: 20,
        }),
        align: numericColumn ? "right" : "left",
        value: (row) => {
          const value = shown(row, index);
          if (value === null) return NULL;
          if (bool) return boolText(value);
          if (geometryColumn) return geometryText(value);
          if (columnType === undefined) {
            // Un solo intento de decodificar sirve para las dos preguntas —¿es geometría? ¿qué
            // texto muestro?—; separarlas en `isGeometryText` + `geometryText` decodificaría el
            // mismo hex dos veces por celda dibujada, y `value()` corre en cada cuadro del scroll.
            const decoded = geometryTextOrNull(value);
            if (decoded !== null) return decoded;
          }
          const text = timeZone.show(value);
          return oneLine(numericColumn ? numberFormat.show(text) : text);
        },
        // Lo que se copia y lo que muestra el visor es el valor como vino, no el de una línea.
        raw: (row) => shown(row, index),
        edit: (row) => shown(row, index),
        title: (row) => shown(row, index) ?? undefined,
        tone: (row) =>
          editing?.edits.get(row.index)?.has(index)
            ? "bg-amber-100/70 dark:bg-amber-900/30"
            : shown(row, index) === null
              ? "cell-null"
              : "",
      };
    });

    return [
      {
        key: "#",
        header: "#",
        width: gutterWidth(rows.length, gridZoom.size),
        align: "right",
        value: (row) => String(row.index + 1),
        // Ordenar por esta columna devuelve el resultado al orden en que lo mandó el servidor.
        sort: (row) => row.index,
      },
      ...cells,
    ];
  });

  /** Un valor con saltos de línea rompería la altura fija de la fila. */
  function oneLine(value: string | null): string {
    if (value === null) return NULL;
    // El `includes` antes de la expresión regular no es una manía: `value()` se llama por cada
    // celda dibujada y en cada cuadro del desplazamiento, y la enorme mayoría de los valores no
    // tiene ningún salto de línea que aplastar.
    return value.includes("\n") ? value.replace(/\s*\n\s*/g, " ↵ ") : value;
  }

  /**
   * Si la columna se alinea a la derecha. El resultado de una consulta no trae los tipos —viaja como
   * texto—, así que se decide por lo que se ve: una columna donde todo lo que hay son números se lee
   * en columna, con las unidades una debajo de otra. Alcanza con que la muestra sea uniforme; una
   * fila más abajo con texto solo queda alineada distinto, no rompe nada.
   */
  function numeric(sample: (string | null)[][], index: number): boolean {
    let seen = 0;
    for (const row of sample) {
      const value = row[index];
      if (value === null) continue;
      if (!/^[+-]?\d+(\.\d+)?$/.test(value.trim())) return false;
      seen += 1;
    }
    return seen > 0;
  }
</script>

<!--
  El resultado ya está entero en memoria, así que ordenarlo acá no vuelve a consultar nada: es la
  forma más rápida de mirar lo mismo por otro criterio sin escribir un ORDER BY y volver a esperar.
-->
<DataGrid
  columns={definitions}
  rows={numbered}
  rowKey={(row) => row.index}
  sortable
  editable={editing ? (_row, column) => editing.editable[positionOf(column)] === true : undefined}
  onedit={editing ? (row, column, value) => editing.onedit(row.index, positionOf(column), value) : undefined}
  empty={t("La consulta no devolvió ninguna fila.")}
/>
