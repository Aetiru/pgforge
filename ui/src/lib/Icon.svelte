<script lang="ts" module>
  /**
   * Íconos del árbol y de la barra.
   *
   * Se dibujan a mano en vez de sumar una librería: son unas pocas decenas de formas simples y una
   * dependencia de íconos completa pesa más que toda la interfaz.
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
    | "policy"
    | "role"
    | "extension"
    | "plug"
    | "sliders"
    | "plus"
    | "play"
    | "sql"
    | "search"
    | "refresh"
    | "chevron"
    | "close"
    | "copy"
    | "dots"
    | "sun"
    | "moon"
    | "auto"
    | "trash"
    | "edit"
    | "check"
    | "warn"
    | "info"
    | "clock"
    | "key"
    | "sort"
    | "chart"
    | "compass"
    | "download"
    | "upload"
    | "eye"
    | "eye-off"
    | "diagram";

  const PATHS: Record<IconName, string> = {
    server:
      '<rect x="3" y="4" width="18" height="7" rx="2"/><rect x="3" y="13" width="18" height="7" rx="2"/><path d="M7 7.5h.01M7 16.5h.01"/>',
    database:
      '<ellipse cx="12" cy="5" rx="8" ry="3"/><path d="M4 5v14c0 1.66 3.58 3 8 3s8-1.34 8-3V5"/><path d="M4 12c0 1.66 3.58 3 8 3s8-1.34 8-3"/>',
    schema: '<path d="M3 7l9-4 9 4-9 4-9-4z"/><path d="M3 12l9 4 9-4"/><path d="M3 17l9 4 9-4"/>',
    folder: '<path d="M3 7a2 2 0 0 1 2-2h3.5l2 2H19a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
    table: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 9.5h18M9 9.5V20"/>',
    // Dos cajas unidas por una relación: el diagrama entero en 24 píxeles.
    diagram:
      '<rect x="2.5" y="3.5" width="8" height="7" rx="1.5"/><rect x="13.5" y="13.5" width="8" height="7" rx="1.5"/><path d="M6.5 10.5v4a2 2 0 0 0 2 2h5"/>',
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
    // Escudo con ojo: es un filtro sobre lo que se ve, no una restricción de integridad.
    policy:
      '<path d="M12 3l8 3v6c0 4.8-3.4 8.4-8 9-4.6-.6-8-4.2-8-9V6z"/><circle cx="12" cy="11.5" r="2"/><path d="M7.5 11.5c1.4-2 3-3 4.5-3s3.1 1 4.5 3c-1.4 2-3 3-4.5 3s-3.1-1-4.5-3z"/>',
    role: '<circle cx="12" cy="8" r="3.5"/><path d="M5 20c0-3.9 3.1-7 7-7s7 3.1 7 7"/>',
    plus: '<path d="M12 5v14M5 12h14"/>',
    play: '<path d="M7 4.8l12 7.2-12 7.2z"/>',
    sql: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M6.5 10l2.5 2-2.5 2M12 14.5h5.5"/>',
    search: '<circle cx="11" cy="11" r="7"/><path d="M20 20l-3.5-3.5"/>',
    refresh: '<path d="M20 11a8 8 0 1 0-2.3 5.7"/><path d="M20 5v6h-6"/>',
    chevron: '<path d="M9 6l6 6-6 6"/>',
    close: '<path d="M6 6l12 12M18 6L6 18"/>',
    copy: '<rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/>',
    dots: '<circle cx="12" cy="5" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="12" cy="19" r="1.4"/>',
    sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M2 12h2M20 12h2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M19.1 4.9l-1.4 1.4M6.3 17.7l-1.4 1.4"/>',
    moon: '<path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5z"/>',
    /* El automático es el mismo círculo con una mitad llena: sigue al sistema, sea cual sea. */
    auto: '<circle cx="12" cy="12" r="8.5"/><path d="M12 3.5a8.5 8.5 0 0 1 0 17z" fill="currentColor" stroke="none"/>',
    trash: '<path d="M4 7h16M9.5 7V5h5v2M6.5 7l1 13h9l1-13"/>',
    edit: '<path d="M4 20h4L19 9a2.1 2.1 0 0 0-3-3L5 17z"/><path d="M14.5 6.5l3 3"/>',
    check: '<path d="M5 12.5l4.5 4.5L19 7"/>',
    warn: '<path d="M12 3.5l9 16H3z"/><path d="M12 10v4.5M12 17.2h.01"/>',
    info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v5.5M12 7.8h.01"/>',
    clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5.5l3.5 2"/>',
    key: '<circle cx="8" cy="15" r="4"/><path d="M10.9 12.1L20 3l1.5 1.5-1.5 1.5 1.5 1.5-3 3-1.5-1.5-1.5 1.5"/>',
    sort: '<path d="M8 4v16M8 4L4.5 7.5M8 4l3.5 3.5"/><path d="M16 20V4M16 20l-3.5-3.5M16 20l3.5-3.5"/>',
    chart: '<path d="M4 20V4"/><path d="M4 20h16"/><path d="M8 16.5v-4M12.5 16.5v-8M17 16.5v-5.5"/>',
    compass: '<circle cx="12" cy="12" r="9"/><path d="M15.5 8.5l-2 5-5 2 2-5z"/>',
    download: '<path d="M12 3v12"/><path d="M7.5 10.5L12 15l4.5-4.5"/><path d="M4 20h16"/>',
    upload: '<path d="M12 15V3"/><path d="M7.5 7.5L12 3l4.5 4.5"/><path d="M4 20h16"/>',
    eye: '<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7z"/><circle cx="12" cy="12" r="3"/>',
    "eye-off": '<path d="M9.9 5.2A9.5 9.5 0 0112 5c6.5 0 10 7 10 7a17 17 0 01-3.2 4M6.2 6.2A17 17 0 002 12s3.5 7 10 7a9.5 9.5 0 004.1-.9"/><path d="M9.9 9.9a3 3 0 004.2 4.2"/><path d="M3 3l18 18"/>',
    extension:
      '<path d="M9 4.5a1.6 1.6 0 0 1 3.2 0c0 .45-.15.82-.15 1.2 0 .44.36.8.8.8H16a1 1 0 0 1 1 1v3.15c0 .44.36.8.8.8.38 0 .75-.15 1.2-.15a1.6 1.6 0 0 1 0 3.2c-.45 0-.82-.15-1.2-.15-.44 0-.8.36-.8.8V19a1 1 0 0 1-1 1h-3.15c-.44 0-.8-.36-.8-.8 0-.38.15-.75.15-1.2a1.6 1.6 0 0 0-3.2 0c0 .45.15.82.15 1.2 0 .44-.36.8-.8.8H5a1 1 0 0 1-1-1v-3.15c0-.44-.36-.8-.8-.8-.38 0-.75.15-1.2.15a1.6 1.6 0 0 1 0-3.2c.45 0 .82.15 1.2.15.44 0 .8-.36.8-.8V7.5a1 1 0 0 1 1-1h3.15c.44 0 .8-.36.8-.8 0-.38-.15-.75-.15-1.2z"/>',
    plug: '<path d="M12 22v-5"/><path d="M9 8V2"/><path d="M15 8V2"/><path d="M6 8h12v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4z"/>',
    sliders:
      '<path d="M4 6h10M18 6h2M4 12h2M10 12h10M4 18h6M14 18h6"/><circle cx="16" cy="6" r="2"/><circle cx="8" cy="12" r="2"/><circle cx="12" cy="18" r="2"/>',
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
