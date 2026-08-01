<script lang="ts">
  import DataGrid, { type Column } from "./DataGrid.svelte";

  let {
    columns,
    rows,
  }: {
    columns: string[];
    rows: (string | null)[][];
  } = $props();

  /** Lo que se muestra en lugar de un NULL, que no es lo mismo que una celda vacía. */
  const NULL = "[null]";

  /**
   * `DataGrid` no mide el contenido a propósito: sus columnas son parte de la definición. Acá la
   * definición se arma en el momento, así que el ancho hay que estimarlo, y se estima con una
   * muestra: recorrer cien mil filas para elegir un ancho costaría más que dibujarlas.
   */
  const SAMPLE = 50;
  const CHAR_WIDTH = 7.3;
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
      const longest = sample.reduce(
        (max, row) => Math.max(max, (row[index] ?? NULL).length),
        name.length,
      );

      return {
        key: `${index}-${name}`,
        header: name,
        width: Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, Math.round(longest * CHAR_WIDTH) + 20)),
        value: (row) => oneLine(row.cells[index]),
        title: (row) => row.cells[index] ?? undefined,
        tone: (row) =>
          row.cells[index] === null ? "italic text-zinc-400 dark:text-zinc-600" : "",
      };
    });

    return [
      {
        key: "#",
        header: "#",
        width: 56,
        align: "right",
        value: (row) => String(row.index + 1),
      },
      ...cells,
    ];
  });

  /** Un valor con saltos de línea rompería la altura fija de la fila. */
  function oneLine(value: string | null): string {
    return value === null ? NULL : value.replace(/\s*\n\s*/g, " ↵ ");
  }
</script>

<DataGrid
  columns={definitions}
  rows={numbered}
  rowKey={(row) => row.index}
  empty="La consulta no devolvió ninguna fila."
/>
