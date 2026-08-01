<script lang="ts" module>
  /**
   * Íconos del árbol y de la barra.
   *
   * Se dibujan a mano en vez de sumar una librería: son quince formas simples y una dependencia de
   * íconos completa pesa más que toda la interfaz.
   */
  export type IconName =
    | "server"
    | "database"
    | "schema"
    | "folder"
    | "table"
    | "partitioned"
    | "view"
    | "matview"
    | "sequence"
    | "function"
    | "type"
    | "column"
    | "index"
    | "constraint"
    | "trigger"
    | "plus"
    | "play"
    | "sql"
    | "search"
    | "refresh"
    | "chevron"
    | "close"
    | "copy"
    | "dots";

  const PATHS: Record<IconName, string> = {
    server:
      '<rect x="3" y="4" width="18" height="7" rx="2"/><rect x="3" y="13" width="18" height="7" rx="2"/><path d="M7 7.5h.01M7 16.5h.01"/>',
    database:
      '<ellipse cx="12" cy="5" rx="8" ry="3"/><path d="M4 5v14c0 1.66 3.58 3 8 3s8-1.34 8-3V5"/><path d="M4 12c0 1.66 3.58 3 8 3s8-1.34 8-3"/>',
    schema: '<path d="M3 7l9-4 9 4-9 4-9-4z"/><path d="M3 12l9 4 9-4"/><path d="M3 17l9 4 9-4"/>',
    folder: '<path d="M3 7a2 2 0 0 1 2-2h3.5l2 2H19a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
    table: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 9.5h18M9 9.5V20"/>',
    partitioned:
      '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 9.5h18"/><path d="M9 9.5v3.5M9 16v4"/>',
    view: '<path d="M2 12s3.6-6.5 10-6.5S22 12 22 12s-3.6 6.5-10 6.5S2 12 2 12z"/><circle cx="12" cy="12" r="2.5"/>',
    matview:
      '<path d="M2 12s3.6-6.5 10-6.5S22 12 22 12s-3.6 6.5-10 6.5S2 12 2 12z"/><circle cx="12" cy="12" r="2.5"/><path d="M17 4l3 1-1 3"/>',
    sequence: '<path d="M9 4L7.5 20M17 4l-1.5 16M4 9.5h16M3.5 15h16"/>',
    function:
      '<path d="M8.5 4a3 3 0 0 0-3 3v2a3 3 0 0 1-3 3 3 3 0 0 1 3 3v2a3 3 0 0 0 3 3"/><path d="M15.5 4a3 3 0 0 1 3 3v2a3 3 0 0 0 3 3 3 3 0 0 0-3 3v2a3 3 0 0 1-3 3"/>',
    type: '<path d="M20.6 13.4l-7.2 7.2a2 2 0 0 1-2.8 0l-7.2-7.2A2 2 0 0 1 3 12V4a1 1 0 0 1 1-1h8a2 2 0 0 1 1.4.6l7.2 7.2a2 2 0 0 1 0 2.6z"/><circle cx="7.6" cy="7.6" r="1.1"/>',
    column: '<rect x="4" y="4" width="6" height="16" rx="1"/><rect x="14" y="4" width="6" height="16" rx="1"/>',
    index: '<circle cx="8" cy="15" r="4"/><path d="M10.9 12.1L20 3M17 6l2.5 2.5M14 9l2.5 2.5"/>',
    constraint: '<path d="M12 3l8 3v6c0 4.8-3.4 8.4-8 9-4.6-.6-8-4.2-8-9V6z"/>',
    trigger: '<path d="M13 2L4.5 13.5H11L10 22l8.5-11.5H12z"/>',
    plus: '<path d="M12 5v14M5 12h14"/>',
    play: '<path d="M7 4.8l12 7.2-12 7.2z"/>',
    sql: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M6.5 10l2.5 2-2.5 2M12 14.5h5.5"/>',
    search: '<circle cx="11" cy="11" r="7"/><path d="M20 20l-3.5-3.5"/>',
    refresh: '<path d="M20 11a8 8 0 1 0-2.3 5.7"/><path d="M20 5v6h-6"/>',
    chevron: '<path d="M9 6l6 6-6 6"/>',
    close: '<path d="M6 6l12 12M18 6L6 18"/>',
    copy: '<rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/>',
    dots: '<circle cx="12" cy="5" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="12" cy="19" r="1.4"/>',
  };

  export function iconPath(name: IconName): string {
    return PATHS[name];
  }
</script>

<script lang="ts">
  let {
    name,
    size = 14,
    class: className = "",
  }: {
    name: IconName;
    size?: number;
    class?: string;
  } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.7"
  stroke-linecap="round"
  stroke-linejoin="round"
  class="shrink-0 {className}"
  aria-hidden="true">{@html iconPath(name)}</svg
>
