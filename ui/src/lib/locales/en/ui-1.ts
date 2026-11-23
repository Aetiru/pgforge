import type { Dictionary } from "../../i18n";

/** Lote 1 de componentes (ver el comentario de `index.ts`). */
export const ui1: Dictionary = {
  // BookmarksList
  "Filtrar marcadores": "Filter bookmarks",
  "Todavía no marcaste nada": "Nothing bookmarked yet",
  "Sin coincidencias": "No matches",
  "Marcá una tabla, vista, función o procedimiento desde el árbol o desde su detalle para tenerla siempre a mano acá.":
    "Bookmark a table, view, function or procedure from the tree or its details to keep it at hand here.",
  "Probá con otra parte del nombre, del esquema o del alias.":
    "Try another part of the name, schema or alias.",
  "Revelar en el árbol": "Reveal in tree",
  "Quitar el marcador": "Remove bookmark",
  "Quitar de marcadores": "Remove from bookmarks",
  "¿Quitar «{name}» de la sección fija? El objeto no se toca en el servidor.":
    "Remove \"{name}\" from the pinned section? The object is not touched on the server.",
  Quitar: "Remove",

  // DatabaseDialog
  "Poné un nombre para la base.": "Enter a name for the database.",
  "El límite de conexiones tiene que ser un entero (-1 es sin límite).":
    "The connection limit must be an integer (-1 means unlimited).",
  "No hay nada que cambiar.": "Nothing to change.",
  "Se va a modificar una base.": "A database is about to be modified.",
  "Editar {name}": "Edit {name}",
  "Nueva base": "New database",
  "Leyendo la definición…": "Reading the definition…",
  Nombre: "Name",
  Dueño: "Owner",
  "el rol conectado": "the connected role",
  Plantilla: "Template",
  Codificación: "Encoding",
  "la de la plantilla": "the template's",
  "Límite de conexiones": "Connection limit",
  "Impide que se conecte nadie más": "Prevents anyone else from connecting",
  "Admite conexiones": "Allow connections",
  "Crear o borrar una base no corre adentro de una transacción, así que los cambios se mandan uno por uno y lo que ya se hizo queda hecho aunque el siguiente falle.":
    "Creating or dropping a database does not run inside a transaction, so changes are sent one by one and whatever was already done stays done even if the next one fails.",
  "Ver SQL": "View SQL",
  Crear: "Create",

  // FontSize
  "Tamaño de letra del SQL — Ctrl +, Ctrl - y Ctrl 0": "SQL font size — Ctrl +, Ctrl - and Ctrl 0",
  "Achicar la letra del SQL": "Decrease SQL font size",
  "Achicar (Ctrl -)": "Decrease (Ctrl -)",
  "Volver al tamaño normal": "Reset to normal size",
  "Volver al tamaño normal (Ctrl 0)": "Reset to normal size (Ctrl 0)",
  "Agrandar la letra del SQL": "Increase SQL font size",
  "Agrandar (Ctrl +)": "Increase (Ctrl +)",
  "Interlineado del SQL": "SQL line spacing",
  "Interlineado compacto: apretá para volver al cómodo":
    "Compact line spacing: click to go back to comfortable",
  "Interlineado cómodo: apretá para ver más líneas (compacto)":
    "Comfortable line spacing: click to see more lines (compact)",

  // GridSize
  "Tamaño de la grilla — Ctrl + rueda": "Grid size — Ctrl + wheel",
  "Achicar la grilla": "Shrink the grid",
  Achicar: "Shrink",
  "Agrandar la grilla": "Enlarge the grid",
  Agrandar: "Enlarge",

  // GenerateSqlDialog
  "Generar SQL": "Generate SQL",
  "Lo que falta va como": "Whatever is missing goes in as",
  ": al ejecutar, la consulta pregunta cada valor.": ": when run, the query asks for each value.",
  "Copiado ✓": "Copied ✓",
  Copiar: "Copy",
  "Abrir en una consulta": "Open in a query",
  SELECT: "SELECT",
  INSERT: "INSERT",
  UPDATE: "UPDATE",
  DELETE: "DELETE",

  // NewGroupDialog
  "Poné un nombre para la carpeta.": "Enter a name for the folder.",
  "Ya existe una carpeta \"{name}\".": "A folder named \"{name}\" already exists.",
  "Nueva carpeta": "New folder",
  "La carpeta agrupa conexiones guardadas: no toca nada en los servidores. Arrastrá servidores del árbol para meterlos. Una barra anida —\"Clientes/ACME\" se dibuja adentro de \"Clientes\"—. Una carpeta que quede vacía no se guarda: desaparece al cerrar la aplicación.":
    "A folder groups saved connections: it does not touch anything on the servers. Drag servers from the tree to put them in. A slash nests folders —\"Clients/ACME\" is drawn inside \"Clients\"—. A folder left empty is not saved: it disappears when the application is closed.",
  "Esto no va a verse en esta ventana: queda fuera de \"{root}\".":
    "This will not be visible in this window: it falls outside \"{root}\".",

  // PlanAdvice
  Copiado: "Copied",
  "Crear el índice…": "Create the index…",
  "\"Explicar y medir\" ejecuta la consulta y compara lo estimado con lo real; con eso se pueden señalar los recorridos completos que sobran y los índices que faltan.":
    "\"Explain and measure\" runs the query and compares the estimate with the actual numbers; that makes it possible to point out unnecessary full scans and missing indexes.",

  // RestoreDialog
  Custom: "Custom",
  "el formato propio de pg_dump, comprimido": "pg_dump's own format, compressed",
  Directorio: "Directory",
  "un directorio con un archivo por tabla; el único que se puede paralelizar":
    "a directory with one file per table; the only one that can be parallelized",
  Tar: "Tar",
  "un tar sin comprimir": "an uncompressed tar",
  "El directorio del backup a restaurar": "The backup directory to restore",
  "El archivo del backup a restaurar": "The backup file to restore",
  "Se va a restaurar un respaldo sobre esta base.": "A backup is about to be restored onto this database.",
  Restaurar: "Restore",
  "base {name}": "database {name}",
  "Directorio del backup": "Backup directory",
  "Archivo del backup": "Backup file",
  "La ruta se elige con el botón: solo se aceptan rutas elegidas en el diálogo del sistema":
    "The path is chosen with the button: only paths picked in the system dialog are accepted",
  "Elegir…": "Choose…",
  "Base destino": "Target database",
  "Con \"crear la base\" el destino lo define el propio backup.":
    "With \"create the database\" the target is defined by the backup itself.",
  "Qué restaurar": "What to restore",
  "El esquema y los datos": "Schema and data",
  "Solo el esquema": "Schema only",
  "Solo los datos": "Data only",
  "Limpiar (eliminar cada objeto antes de recrearlo)": "Clean (drop each object before recreating it)",
  "…y no fallar si todavía no existe": "…and do not fail if it does not exist yet",
  "Crear la base destino": "Create the target database",
  "Una sola transacción (revierte si algo falla)": "Single transaction (rolls back if anything fails)",
  "Sin los dueños": "No owners",
  "Sin los privilegios": "No privileges",
  "Trabajos en paralelo": "Parallel jobs",
  "Corre en segundo plano: se sigue y se cancela desde la vista de procesos.":
    "Runs in the background: follow it and cancel it from the Processes view.",

  // ResultFileDialog
  "Exportar el resultado": "Export the result",
  "un arreglo de objetos, uno por fila": "an array of objects, one per row",
  "una tabla lista para pegar": "a table ready to paste",
  Excel: "Excel",
  "un libro de una hoja; los números como número y los textos, como el 00123, intactos":
    "a one-sheet workbook; numbers as numbers and texts, such as 00123, left intact",
  "Se escriben las {count} filas que están cargadas. Para CSV, texto o binario —que sí vuelven al servidor y traen todas— usá \"Exportar\" de la barra.":
    "The {count} rows that are loaded are written. For CSV, text or binary —which go back to the server and bring all of them— use \"Export\" in the toolbar.",
  "El resultado quedó cortado por el techo de filas: el archivo no tiene todas. Subí el máximo y volvé a ejecutar, o exportá en CSV.":
    "The result was cut off by the row limit: the file does not have all of them. Raise the maximum and run again, or export as CSV.",
  "Guardado en {path}.": "Saved to {path}.",
  "Guardar…": "Save…",

  // RoleCapabilitiesPanel
  "— sin permisos —": "— no permissions —",
  "Otorgado directo": "Granted directly",
  "Heredado por membresía": "Inherited through membership",
  heredado: "inherited",
  Esquema: "Schema",
  "Volver a leer": "Reload",
  "Sin objetos": "No objects",
  "Este esquema no tiene nada que {role} pueda tocar.": "This schema has nothing that {role} can touch.",
  Tablas: "Tables",
  Secuencias: "Sequences",
  Funciones: "Functions",

  // SizeDecision
  "Tabla muy grande en producción": "Very large table in production",
  "tiene unas {rows} filas y la consulta no tiene": "has about {rows} rows and the query has no",
  ". La aplicación solo trae las primeras que entran en el máximo elegido, pero el servidor puede tardar mucho en producirlas.":
    ". The application only fetches the first ones that fit in the chosen maximum, but the server may take a long time to produce them.",
  "Ejecutar igual": "Run anyway",
  "Ejecutar con LIMIT {limit}": "Run with LIMIT {limit}",

  // SnippetLibraryDialog
  "Biblioteca de plantillas": "Template library",
  "Elegí una y agregala a tus abreviaturas: después se edita como cualquier otra.":
    "Pick one and add it to your abbreviations: afterwards it is edited like any other.",
  "Buscar plantillas…": "Search templates…",
  "Buscar plantillas": "Search templates",
  Todas: "All",
  Agregada: "Added",
  "Ninguna plantilla coincide": "No template matches",
  "Abreviatura con la que se agrega": "Abbreviation to add it with",
  Abreviatura: "Abbreviation",
  "Agregada como \"{name}\". Escribila en el editor y apretá Tab.":
    "Added as \"{name}\". Type it in the editor and press Tab.",
  "Elegí una plantilla": "Pick a template",
  "Ya está agregada: cambiale la abreviatura para agregarla otra vez":
    "Already added: change its abbreviation to add it again",
  Agregar: "Add",

  // TypeDialog
  "Poné un nombre para el tipo.": "Enter a name for the type.",
  "Un tipo compuesto necesita al menos un campo.": "A composite type needs at least one field.",
  "Hay campos con el mismo nombre.": "Some fields have the same name.",
  "Una enumeración necesita al menos un valor.": "An enum needs at least one value.",
  "Hay valores repetidos.": "There are duplicate values.",
  "Se va a modificar un tipo.": "A type is about to be modified.",
  "Nuevo tipo compuesto": "New composite type",
  "Nueva enumeración": "New enum",
  Campos: "Fields",
  nombre: "name",
  tipo: "type",
  "Quitar campo": "Remove field",
  "Agregar campo": "Add field",
  "El nombre de un campo que ya existe no se puede editar acá: cambiarlo y cambiarle el tipo a la vez haría imposible distinguir un renombre de un borrado más un alta.":
    "The name of an existing field cannot be edited here: changing it and its type at the same time would make it impossible to tell a rename from a drop plus an add.",
  Valores: "Values",
  valor: "value",
  "Quitar valor": "Remove value",
  "Agregar valor": "Add value",
  "PostgreSQL no puede quitar un valor de una enumeración: {values} se va a conservar. Para sacarlo hay que recrear el tipo y todas las columnas que lo usan.":
    "PostgreSQL cannot remove a value from an enum: {values} will be kept. To remove it you have to recreate the type and every column that uses it.",
  "PostgreSQL no puede quitar un valor de una enumeración: {values} se van a conservar. Para sacarlo hay que recrear el tipo y todas las columnas que lo usan.":
    "PostgreSQL cannot remove a value from an enum: {values} will be kept. To remove them you have to recreate the type and every column that uses it.",

  // detail/Privileges
  Privilegios: "Privileges",
  "Leyendo privilegios…": "Reading privileges…",
  "Nadie tiene privilegios propios: rige el default (el dueño puede todo).":
    "Nobody has privileges of their own: the default applies (the owner can do everything).",
  Privilegio: "Privilege",
  Rol: "Role",
  "con GRANT OPTION": "with GRANT OPTION",
  "Editar los privilegios": "Edit privileges",
  "Revocar todo": "Revoke all",
  "Acotados a columnas": "Column-level",
  Columna: "Column",
  "Por omisión": "Default",
  "lo que van a recibir los objetos que se creen acá": "what objects created here will receive",
  "Cuando crea": "When it creates",
  Sobre: "On",

  // detail/Security
  "Seguridad por fila": "Row-level security",
  "Leyendo las políticas…": "Reading policies…",
  Política: "Policy",
  "Filtrar las filas según las políticas": "Filter rows according to the policies",
  "Aplicarlo también al dueño de la tabla": "Apply it to the table owner as well",
  "El filtro está activo y no hay ninguna política: la tabla no devuelve ninguna fila.":
    "The filter is on and there are no policies: the table returns no rows.",
  "Hay políticas definidas pero el filtro está apagado: no se aplica ninguna.":
    "Policies are defined but the filter is off: none of them applies.",
  "El dueño de la tabla se saltea el filtro. Para probar las políticas hay que conectarse con otro rol, o marcar la segunda casilla.":
    "The table owner bypasses the filter. To test the policies, connect with another role or check the second box.",
  "No tiene políticas.": "It has no policies.",
  Comando: "Command",
  Roles: "Roles",
  Condición: "Condition",
  restrictiva: "restrictive",
  "Editar la política": "Edit policy",
  "Eliminar la política": "Drop policy",

  // detail/Statistics
  Estadísticas: "Statistics",
  "Leyendo estadísticas…": "Reading statistics…",
  "Una tabla particionada no guarda filas propias: mirá las estadísticas de cada partición.":
    "A partitioned table does not store rows of its own: look at the statistics of each partition.",
  "El servidor no lleva estadísticas de esta tabla.": "The server keeps no statistics for this table.",
  "VACUUM, ANALYZE o REINDEX sobre esta tabla": "VACUUM, ANALYZE or REINDEX on this table",
  "Mantenimiento…": "Maintenance…",
  "Tamaño total": "Total size",
  "Tabla, índices y TOAST": "Table, indexes and TOAST",
  "Solo la tabla": "Table only",
  "Incluye el TOAST": "Includes TOAST",
  Índices: "Indexes",
  "Filas vivas": "Live rows",
  "Estimación de las estadísticas, no un recuento": "Estimate from the statistics, not an actual count",
  "Filas muertas": "Dead rows",
  "Una estimación sobre los contadores de estadísticas, no una medición del espacio desperdiciado.":
    "An estimate based on the statistics counters, not a measurement of wasted space.",
  "Último VACUUM": "Last VACUUM",
  "Último ANALYZE": "Last ANALYZE",
  "Lecturas secuenciales": "Sequential scans",
  "Lecturas por índice": "Index scans",

  // TreePanel
  "Se va a correr VACUUM ANALYZE sobre {count} tablas.": "VACUUM ANALYZE is about to run on {count} tables.",
  Consulta: "Query",
  "Servidores y objetos": "Servers and objects",
  "No se pudo abrir este workspace": "Could not open this workspace",
  "Buscando en el servidor…": "Searching on the server…",
  "{count} coincidencia en el servidor": "{count} match on the server",
  "{count} coincidencias en el servidor": "{count} matches on the server",
  "Volver al árbol": "Back to the tree",
  "Sin coincidencias en el servidor": "No matches on the server",
  "No hay ninguna tabla, vista, secuencia, función ni tipo cuyo nombre contenga \"{search}\" en esa base.":
    "There is no table, view, sequence, function or type whose name contains \"{search}\" in that database.",
  "Todavía no hay servidores": "No servers yet",
  "Agregá una conexión para empezar a explorar sus bases, esquemas y tablas.":
    "Add a connection to start exploring its databases, schemas and tables.",
  "Nuevo servidor": "New server",
  "Ningún servidor conectado": "No server connected",
  "El árbol está mostrando solo los servidores conectados y todavía no hay ninguno.":
    "The tree is showing only connected servers and there are none yet.",
  "Mostrar todos los servidores": "Show all servers",
  "La búsqueda solo alcanza a lo que ya se cargó del árbol: abrí los nodos donde puede estar \"{search}\".":
    "The search only covers what the tree has already loaded: open the nodes where \"{search}\" might be.",
  "Buscar también en los servidores sin conectar": "Also search disconnected servers",
  Fijados: "Pinned",
  "Ir al servidor": "Go to server",
  Conectar: "Connect",
  Recientes: "Recent",
  "ver menos": "show less",
  "ver {count} más": "show {count} more",
  "Quitar de recientes": "Remove from recent",
  "{count} coincidencia entre lo ya cargado": "{count} match among what is already loaded",
  "{count} coincidencias entre lo ya cargado": "{count} matches among what is already loaded",
  "Ir a {name}": "Go to {name}",
  "El servidor dejó de responder": "The server stopped responding",
  Conectado: "Connected",
  "Sin conectar": "Not connected",
  "Ida y vuelta contra el servidor: {ms} ms": "Round trip to the server: {ms} ms",
  "Cancelar la lectura": "Cancel the read",
  Contraer: "Collapse",
  Expandir: "Expand",
  "Renombrar o deshacer la carpeta": "Rename or dissolve the folder",
  "Editar la carpeta": "Edit folder",
  "Volver a leer este nodo del servidor": "Reload this node from the server",
  Recargar: "Reload",
  "Nueva consulta": "New query",
  Desconectar: "Disconnect",
  "El servidor dejó de responder. Reconectar.": "The server stopped responding. Reconnect.",
  Reconectar: "Reconnect",
  "Ver los datos": "View data",
  "Consulta nueva con un SELECT de esta tabla": "New query with a SELECT from this table",
  "Consulta nueva": "New query",
  "Copiar el nombre: {name}": "Copy the name: {name}",
  "Copiar el nombre": "Copy the name",
  "tabla marcada": "table marked",
  "tablas marcadas": "tables marked",
  "Copiar los nombres, uno por línea": "Copy the names, one per line",
  "Copiar nombres": "Copy names",
  "Hasta {count} tablas": "Up to {count} tables",
  "Una consulta con el SELECT de cada una": "A query with the SELECT of each one",
  "SELECT de todas": "SELECT from all",
  "VACUUM ANALYZE de cada una, en segundo plano": "VACUUM ANALYZE on each one, in the background",
  Mantenimiento: "Maintenance",
  "Quitar las marcas": "Clear the marks",
  marcado: "marked",
  marcados: "marked",
  "Desconectar los {count}": "Disconnect all {count}",
  "Desconectar los marcados": "Disconnect the marked ones",
  "Mover a una carpeta": "Move to a folder",
  "Sin carpeta": "No folder",
  "Arrastralos a una carpeta para moverlos todos juntos.": "Drag them to a folder to move them all together.",
  "Abrir la base": "Open the database",
  "Por nombre": "By name",
  "Por tamaño en disco": "By size on disk",
  "Por filas": "By rows",
  "Agrupar por prefijo (fact_, dim_…)": "Group by prefix (fact_, dim_…)",
  "Elegir esquemas visibles…": "Choose visible schemas…",
  "Generar SQL…": "Generate SQL…",
  "Comparar datos con otra tabla": "Compare data with another table",
  "Diagrama del esquema": "Schema diagram",
  "Documentar el esquema…": "Document the schema…",
  "Comparar con otro servidor": "Compare with another server",
  "Matriz de permisos": "Permissions matrix",
  "Qué puede hacer": "What it can do",
  "Agregar a marcadores": "Add to bookmarks",
  "Copiar la cadena de conexión": "Copy the connection string",
  Refrescar: "Refresh",
  "Quitar de fijados": "Unpin",
  "Fijar arriba": "Pin to top",
  Editar: "Edit",
  Duplicar: "Duplicate",
  Eliminar: "Delete",
};
