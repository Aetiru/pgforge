import type { Dictionary } from "../../i18n";

/** Lote 3 de componentes (ver el comentario de `index.ts`). */
export const ui3: Dictionary = {
  // App.svelte
  "{name} (copia)": "{name} (copy)",
  "Abrir una consulta": "Open a query",
  carpeta: "folder",
  Buscar: "Search",
  "Filtra lo que el árbol ya trajo. Enter busca en el servidor.":
    "Filters what the tree has already loaded. Enter searches the server.",
  "Con prefijo se acota al tipo — {help}": "A prefix narrows the search to a type — {help}",
  "Limpiar la búsqueda": "Clear the search",
  "Buscar «{text}» en el catálogo de {database}": 'Search for "{text}" in the catalog of {database}',
  "Elegí una base o un objeto del árbol para buscar en el servidor":
    "Pick a database or an object in the tree to search the server",
  "Buscar en el servidor": "Search the server",
  "Seguir la pestaña activa en el árbol": "Follow the active tab in the tree",
  "El árbol sigue a la pestaña activa (clic para dejar de seguirla)":
    "The tree follows the active tab (click to stop following)",
  "Hacer que el árbol revele la tabla de la pestaña activa":
    "Make the tree reveal the table of the active tab",
  "Nuevo servidor": "New server",
  "Más opciones del árbol": "More tree options",
  "Nueva carpeta": "New folder",
  "Importar servidores…": "Import servers…",
  "Contraer todo": "Collapse all",
  "No hay servidores conectados": "No servers connected",
  "Cierra la conexión de todos los servidores y sus pestañas":
    "Closes the connection of every server and its tabs",
  "Desconectar todos": "Disconnect all",
  "Ventana de workspace…": "Workspace window…",
  "Esconde los servidores sin conectar": "Hides servers that are not connected",
  "Solo servidores conectados": "Connected servers only",
  "Mostrar objetos del sistema": "Show system objects",
  "Arrastrá para cambiar el ancho; doble clic para restablecerlo":
    "Drag to resize; double-click to reset",
  "Cambios sin guardar": "Unsaved changes",
  "Sacar del panel dividido": "Remove from the split pane",
  "Abrir al lado": "Open side by side",
  "Ya es la pestaña principal": "It is already the main tab",
  "Vuelve a mostrar una sola pestaña": "Goes back to showing a single tab",
  "Abre esta pestaña en un panel al lado, sin dejar de ver la actual":
    "Opens this tab in a side pane, keeping the current one in view",
  "Cerrar la pestaña": "Close the tab",
  "Nueva consulta": "New query",
  "Nueva consulta contra {database} ({shortcut})": "New query against {database} ({shortcut})",
  "Elegí una base o un objeto de un servidor conectado":
    "Pick a database or an object of a connected server",
  Consulta: "Query",
  "Abrir un archivo SQL": "Open a SQL file",
  "Abrir un archivo .sql contra {database} ({shortcut})": "Open a .sql file against {database} ({shortcut})",
  Monitoreo: "Monitoring",
  "Monitoreo de {name}": "Monitoring of {name}",
  "Conectá un servidor para ver su monitoreo": "Connect a server to see its monitoring",
  "Configuración del servidor": "Server configuration",
  "Configuración de {name}": "Configuration of {name}",
  "Conectá un servidor para ver su configuración": "Connect a server to see its configuration",
  Inspector: "Inspector",
  "Ocultar el inspector": "Hide the inspector",
  "Mostrar el inspector": "Show the inspector",
  "Empezá con un servidor": "Start with a server",
  "Agregá una conexión a PostgreSQL, o traé las que ya tenés configuradas en pgpass, pg_service.conf o DBeaver. Las contraseñas nunca se importan.":
    "Add a PostgreSQL connection, or bring in the ones you already have in pgpass, pg_service.conf or DBeaver. Passwords are never imported.",
  "Importar servidores existentes…": "Import existing servers…",
  "No hay ninguna pestaña abierta": "No tab is open",
  "Elegí algo en el árbol para verlo en el inspector, o abrí una consulta contra la base con la que estés trabajando.":
    "Pick something in the tree to see it in the inspector, or open a query against the database you are working with.",
  Configuración: "Configuration",
  "Arrastrá para repartir el espacio; doble clic para repartirlo por la mitad":
    "Drag to share the space; double-click to split it in half",
  Detalle: "Details",
  "Elegí un objeto del árbol para ver su detalle.": "Pick an object in the tree to see its details.",
  Ln: "Ln",
  Col: "Col",
  "Sel.": "Sel.",
  "car.": "chars",
  Auto: "Auto",
  Manual: "Manual",
  "Servidores conectados": "Connected servers",
  conectado: "connected",
  conectados: "connected",
  "pgforge {version} está disponible": "pgforge {version} is available",
  "Registro en {path}": "Log at {path}",
  "Buscar una versión nueva": "Check for a new version",
  "Desconectar todos los servidores": "Disconnect all servers",
  "Se cierran las {count} conexiones abiertas y también todas las pestañas de esos servidores. Las consultas sin guardar de esas pestañas se pierden; las transacciones abiertas se revierten.":
    "The {count} open connections will be closed, along with all the tabs of those servers. Unsaved queries in those tabs are lost; open transactions are rolled back.",
  "Contraseña de {name}": "Password for {name}",
  Contraseña: "Password",
  Conectar: "Connect",
  "Clave del bastión SSH sin verificar": "Unverified SSH bastion host key",
  "El host {host} presentó una clave distinta de la registrada, lo que podría indicar un intermediario. Huella SHA256: {fingerprint}. ¿Confiar en esta clave y recordarla?":
    "The host {host} presented a different key from the one on record, which could indicate a man in the middle. SHA256 fingerprint: {fingerprint}. Trust this key and remember it?",
  "El host {host} no está en tu archivo known_hosts. Huella SHA256: {fingerprint}. ¿Confiar en esta clave y recordarla?":
    "The host {host} is not in your known_hosts file. SHA256 fingerprint: {fingerprint}. Trust this key and remember it?",
  "Confiar de todos modos": "Trust anyway",
  "Confiar y conectar": "Trust and connect",
  "Cerrar «{title}»": 'Close "{title}"',
  "La pestaña tiene una transacción abierta. Si la cerrás sin decidir, el servidor revierte todo lo que no esté confirmado. ¿Qué hacemos con ella?":
    "The tab has an open transaction. If you close it without deciding, the server rolls back everything that is not committed. What should we do with it?",
  "Eliminar «{name}»": 'Delete "{name}"',
  "Se borra el servidor de la lista y su contraseña guardada. No se toca nada en la base de datos.":
    "The server is removed from the list along with its saved password. Nothing in the database is touched.",
  Eliminar: "Delete",
  "Se borra la referencia a esta ventana guardada. No se toca ningún servidor ni carpeta de conexiones, y una ventana que la tenga abierta en este momento sigue abierta.":
    "The reference to this saved window is removed. No server or connection folder is touched, and a window that has it open right now stays open.",
  "Operación destructiva en producción": "Destructive operation in production",
  "Modificar producción": "Modify production",
  "«{name}» está marcado como servidor de producción. {action}":
    '"{name}" is marked as a production server. {action}',
  "Ejecutar igual": "Run anyway",
  "Modificar igual": "Modify anyway",

  // CommentDialog
  "Se va a cambiar un comentario.": "A comment is about to be changed.",
  Comentario: "Comment",
  Texto: "Text",
  "Dejalo vacío para borrar el comentario.": "Leave it empty to delete the comment.",
  "Ver SQL": "View SQL",

  // CompareDialog
  "Instantánea de esquema": "Schema snapshot",
  "Instantánea a comparar": "Snapshot to compare",
  "Dónde guardar la instantánea": "Where to save the snapshot",
  servidor: "server",
  "Comparar esquemas": "Compare schemas",
  "Contra otro esquema en vivo": "Against another live schema",
  "Qué cambió desde que se tomó una instantánea —por ejemplo, antes de un deploy—":
    "What changed since a snapshot was taken, for example before a deploy",
  "Contra una instantánea guardada": "Against a saved snapshot",
  "Origen · la instantánea, el estado que se quiere": "Source · the snapshot, the state you want",
  "Ningún archivo elegido": "No file chosen",
  "Elegir…": "Choose…",
  "Destino · el que se llevaría hasta la instantánea": "Target · the one that would be brought up to the snapshot",
  "El script devuelve este esquema a como estaba cuando se tomó la instantánea. Leído al revés, el informe dice qué cambió desde entonces.":
    "The script returns this schema to how it was when the snapshot was taken. Read the other way around, the report tells what changed since then.",
  "Origen · el estado que se quiere": "Source · the state you want",
  "Destino · el que se llevaría hasta el origen": "Target · the one that would be brought up to the source",
  Servidor: "Server",
  Base: "Database",
  Esquema: "Schema",
  "Hay un solo servidor conectado. Se puede comparar contra otro esquema del mismo, o conectar el otro servidor y volver a abrir esta ventana.":
    "Only one server is connected. You can compare against another schema on it, or connect the other server and reopen this window.",
  "Los dos lados son el mismo esquema: no hay nada que comparar.":
    "Both sides are the same schema: there is nothing to compare.",
  "Se comparan {scope}. No se comparan los datos, los dueños ni los permisos por columna.":
    "Compared: {scope}. Data, owners and column-level privileges are not compared.",
  "Una instantánea tomada con una versión anterior de pgforge no trae funciones, disparadores, políticas ni permisos: hay que volver a tomarla.":
    "A snapshot taken with an earlier version of pgforge has no functions, triggers, policies or privileges: it has to be taken again.",
  "Instantánea guardada en {path}": "Snapshot saved to {path}",
  "Guarda {name} tal como está ahora, para compararlo más adelante":
    "Saves {name} as it is now, to compare it later",
  "Guardando…": "Saving…",
  "Guardar instantánea…": "Save snapshot…",
  Comparar: "Compare",

  // Confirm
  "Para continuar, escribí": "To continue, type",
  "Nombre del servidor": "Server name",

  // ConstraintDialog
  "Poné un nombre para la constraint.": "Enter a name for the constraint.",
  "Elegí al menos una columna.": "Pick at least one column.",
  "Elegí al menos una columna local.": "Pick at least one local column.",
  "Poné la tabla referenciada.": "Enter the referenced table.",
  "Poné las columnas referenciadas.": "Enter the referenced columns.",
  "Poné la expresión del CHECK.": "Enter the CHECK expression.",
  "Se va a crear una restricción.": "A constraint is about to be created.",
  "Nueva restricción": "New constraint",
  Nombre: "Name",
  Tipo: "Type",
  "Columnas locales": "Local columns",
  Columnas: "Columns",
  "Esquema referenciado": "Referenced schema",
  "Tabla referenciada": "Referenced table",
  "Columnas referenciadas (separadas por coma)": "Referenced columns (comma-separated)",
  "Sin acción": "No action",
  Expresión: "Expression",
  Agregar: "Add",

  // ForeignServerDialog
  "Poné un nombre para el servidor.": "Enter a name for the server.",
  "Elegí el wrapper del servidor.": "Pick the server's wrapper.",
  "Nada que aplicar.": "Nothing to apply.",
  "Servidor {name}": "Server {name}",
  "Nuevo servidor foráneo": "New foreign server",
  "Tipo (opcional)": "Type (optional)",
  "Versión (opcional)": "Version (optional)",
  Opciones: "Options",
  "Para postgres_fdw: host, port, dbname, …": "For postgres_fdw: host, port, dbname, …",
  Crear: "Create",

  // FunctionDialog
  "Hace falta una sentencia CREATE FUNCTION o CREATE PROCEDURE.":
    "A CREATE FUNCTION or CREATE PROCEDURE statement is required.",
  "Función o procedimiento": "Function or procedure",
  "Se ejecuta tal cual está escrito, contra {database}": "Runs exactly as written, against {database}",
  "Si el texto dice": "If the text says",
  ", reemplaza la que ya existe; si dice": ", it replaces the existing one; if it says",
  "a secas, crea una nueva.": "alone, it creates a new one.",
  Ejecutar: "Run",

  // MigrationsPanel
  "aplicar {version}": "apply {version}",
  "deshacer {version}": "revert {version}",
  Migraciones: "Migrations",
  "Cambiar carpeta…": "Change folder…",
  "Elegir carpeta…": "Choose folder…",
  "Releer la carpeta y el registro": "Reread the folder and the log",
  "Elegí la carpeta de migraciones": "Choose the migrations folder",
  "Son archivos V001__descripcion.sql, con su U001__… opcional para deshacer. Se guardan desde el informe de «Comparar esquemas», o se escriben a mano. Lo aplicado queda anotado en esta máquina, por servidor y base.":
    'They are files named V001__description.sql, with an optional U001__… to revert. They are saved from the "Compare schemas" report, or written by hand. What has been applied is recorded on this machine, per server and database.',
  "{reason}: se pueden ver, no aplicar.": "{reason}: they can be viewed, not applied.",
  "Leyendo la carpeta…": "Reading the folder…",
  "El registro dice que se aplicó {versions}, pero ya no está en la carpeta.":
    "The log says {versions} was applied, but it is no longer in the folder.",
  "Hay scripts de reversa sin su migración: {list}.": "There are revert scripts without their migration: {list}.",
  "La carpeta no tiene migraciones": "The folder has no migrations",
  "Guardá el script de una comparación desde «Comparar esquemas» → «Guardar como migración».":
    'Save the script of a comparison from "Compare schemas" → "Save as migration".',
  "No falta aplicar nada.": "Nothing left to apply.",
  "{count} pendiente de {total}.": "{count} pending of {total}.",
  "{count} pendientes de {total}.": "{count} pending of {total}.",
  Versión: "Version",
  Descripción: "Description",
  Estado: "Status",
  Aplicada: "Applied",
  Acciones: "Actions",
  "Hay migraciones con número mayor ya aplicadas en este servidor":
    "Migrations with a higher number have already been applied on this server",
  "fuera de orden": "out of order",
  "Huella {sum}": "Checksum {sum}",
  "Ver el SQL y aplicarla": "View the SQL and apply it",
  "Reintentar…": "Retry…",
  "Aplicar…": "Apply…",
  "Ver el script de reversa y deshacerla": "View the revert script and undo it",
  "Deshacer…": "Revert…",
  "Registro de intentos ({count})": "Attempt log ({count})",
  Cuándo: "When",
  Sentido: "Direction",
  Resultado: "Result",
  Huella: "Checksum",
  aplicar: "apply",
  deshacer: "revert",
  ok: "ok",
  falló: "failed",
  "Aplicar {version}": "Apply {version}",
  "Deshacer {version}": "Revert {version}",
  "Esto es lo que se va a ejecutar, tal como está en el archivo ahora.":
    "This is what will run, as it is in the file right now.",
  "Va dentro de una transacción: o queda entera o no queda nada.":
    "It runs inside a transaction: either all of it is applied or none of it.",
  "El script maneja su propia transacción.": "The script manages its own transaction.",
  "Corre sentencia por sentencia, sin transacción.": "It runs statement by statement, without a transaction.",
  "El archivo cambió desde que se aplicó: lo que se deshace es lo que dice el script de reversa de hoy.":
    "The file changed since it was applied: what gets reverted is whatever today's revert script says.",
  "-- vacío": "-- empty",
  Aplicar: "Apply",
  Deshacer: "Revert",

  // OptionsEditor
  clave: "key",
  valor: "value",
  "Quitar la opción": "Remove the option",
  "Agregar opción": "Add option",

  // OutlinePanel
  "Esquema del script": "Script outline",
  "Cerrar el esquema del script": "Close the script outline",
  "Filtrar sentencias…": "Filter statements…",
  "Filtrar las sentencias del script": "Filter the script's statements",
  "El script está vacío": "The script is empty",
  "Ninguna sentencia coincide": "No statement matches",

  // PartitionDialog
  "Poné un nombre para la partición.": "Enter a name for the partition.",
  "Poné el extremo inicial del rango.": "Enter the start of the range.",
  "Poné el extremo final del rango.": "Enter the end of the range.",
  "Poné al menos un valor.": "Enter at least one value.",
  "El módulo tiene que ser un entero de al menos 1.": "The modulus must be an integer of at least 1.",
  "El resto tiene que ser un entero menor que el módulo.": "The remainder must be an integer smaller than the modulus.",
  "Se va a modificar una tabla particionada.": "A partitioned table is about to be modified.",
  "Nueva partición": "New partition",
  "Crear tabla nueva": "Create new table",
  "Enganchar una existente": "Attach an existing one",
  "Nombre de la partición": "Partition name",
  "Partición por omisión: se lleva todo lo que no entra en ninguna otra":
    "Default partition: takes everything that does not fit in any other",
  "Desde (incluido)": "From (inclusive)",
  "Hasta (excluido)": "To (exclusive)",
  "Con varias columnas de partición, separá los extremos con coma. Se admiten":
    "With several partition columns, separate the bounds with a comma. Accepted:",
  y: "and",
  "Valores (separados por coma)": "Values (comma-separated)",
  Módulo: "Modulus",
  Resto: "Remainder",
  "El servidor revisa que ninguna fila de la tabla se salga del límite antes de engancharla, y la bloquea mientras lo hace.":
    "The server checks that no row of the table falls outside the bound before attaching it, and locks the table while it does so.",
  Enganchar: "Attach",

  // PolicyDialog
  "Se va a modificar una política de seguridad.": "A security policy is about to be modified.",
  "Editar la política {name}": "Edit policy {name}",
  "Nueva política": "New policy",
  "PostgreSQL no deja cambiar el comando ni el carácter restrictivo de una política que ya existe: se borra y se vuelve a crear con lo que quede acá, todo dentro de la misma transacción.":
    "PostgreSQL does not allow changing the command or the restrictive nature of an existing policy: it is dropped and created again with what is left here, all within the same transaction.",
  Comando: "Command",
  Roles: "Roles",
  "ana, beto — vacío es PUBLIC": "ana, beto — empty means PUBLIC",
  "Separados por coma. Sin roles la política alcanza a todo el mundo.":
    "Comma-separated. With no roles the policy applies to everyone.",
  "Cómo se combina con las demás": "How it combines with the others",
  "Permisiva: suma a lo que dejan pasar las otras": "Permissive: adds to what the others let through",
  "Restrictiva: recorta lo que las permisivas dejaron pasar": "Restrictive: cuts down what the permissive ones let through",
  "USING — qué filas se ven": "USING — which rows are visible",
  "condición SQL, p. ej. dueno = current_user": "SQL condition, e.g. owner = current_user",
  "Una política de INSERT no lleva USING: no hay filas previas que filtrar.":
    "An INSERT policy has no USING: there are no existing rows to filter.",
  "WITH CHECK — qué filas se pueden escribir": "WITH CHECK — which rows can be written",
  "condición SQL; vacío repite la de USING": "SQL condition; empty reuses the USING one",
  "Una política de {command} no lleva WITH CHECK: no escribe ninguna fila que verificar.":
    "A {command} policy has no WITH CHECK: it writes no rows to verify.",
  "Crear política": "Create policy",

  // RehearsalResult
  "Línea {line}:": "Line {line}:",
  Sentencia: "Statement",
  "{count} sin ejecutar.": "{count} not executed.",
  "Locks que tomaría": "Locks it would take",
  "No se pudieron leer: una sentencia falló y la transacción quedó abortada.":
    "They could not be read: a statement failed and the transaction was aborted.",
  "Ninguno que valga la pena mencionar.": "None worth mentioning.",
  "(sin tabla)": "(no table)",
  "en espera": "waiting",
  "en producción, ejecutarlo mantiene esto mientras dure la transacción.":
    "in production, running it holds this for as long as the transaction lasts.",
  "Sentencia {n}:": "Statement {n}:",

  // SnippetVarsDialog
  "elige una opción": "choose an option",
  fecha: "date",
  número: "number",
  "tabla del catálogo": "catalog table",
  "columna del catálogo": "catalog column",
  Insertar: "Insert",

  // ViewDialog
  "Poné un nombre para la vista.": "Enter a name for the view.",
  "Poné la consulta de la vista.": "Enter the view's query.",
  "Se va a modificar una vista.": "A view is about to be modified.",
  "Editar {name}": "Edit {name}",
  "Nueva vista materializada": "New materialized view",
  "Nueva vista": "New view",
  "Columnas (opcional, separadas por coma)": "Columns (optional, comma-separated)",
  "Postgres las infiere solo": "Postgres infers them on its own",
  "Leyendo la definición…": "Reading the definition…",
  "Poblar de inmediato (WITH DATA)": "Populate immediately (WITH DATA)",
};
