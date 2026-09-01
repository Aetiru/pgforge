<script lang="ts">
  import DataGrid, { type Column } from "./DataGrid.svelte";
  import { boolText, isBoolType } from "./format";
  import { columnWidth, gutterWidth } from "./grid-width";
  import { gridZoom } from "./grid.svelte";

  let {
    columns,
    rows,
    types = null,
  }: {
    columns: string[];
    rows: (string | null)[][];
    /** Tipos de cada columna, cuando se pidieron. Van como texto secundario del encabezado, como
     * en la pestaña de datos: al leer un resultado, saber si eso es un `numeric` o un `text` cambia
     * lo que dice. */
    types?: string[] | null;
  } = $props();

  /** Lo que se muestra en lugar de un NULL, que no es lo mismo que una celda vacía. */
  const NULL = "[null]";

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

  const definitions = $derived.by<Column<Numbered>[]>(() => {
    const sample = rows.slice(0, SAMPLE);

    const cells: Column<Numbered>[] = columns.map((name, index) => {
      // El tipo lo trae el interruptor «Tipos», que viene encendido. Sin él, una columna de textos
      // que dijeran «t» y «f» se traduciría sola, que es peor que no traducir ninguna.
      const bool = isBoolType(types?.[index]);
      const longest = sample.reduce(
        (max, row) => Math.max(max, (row[index] ?? NULL).length),
        name.length,
      );

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
        align: numeric(sample, index) ? "right" : "left",
        value: (row) => {
          const value = row.cells[index];
          if (value === null) return NULL;
          return bool ? boolText(value) : oneLine(value);
        },
        // Lo que se copia y lo que muestra el visor es el valor como vino, no el de una línea.
        raw: (row) => row.cells[index],
        title: (row) => row.cells[index] ?? undefined,
        tone: (row) =>
          row.cells[index] === null ? "italic text-zinc-400 dark:text-zinc-600" : "",
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
  empty="La consulta no devolvió ninguna fila."
/>
