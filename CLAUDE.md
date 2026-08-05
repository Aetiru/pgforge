# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Idioma

Todo el proyecto está escrito en español: comentarios, doc-comments, nombres de tests, mensajes de
error, textos de la interfaz y mensajes de commit. Mantenerlo así. Los identificadores de código
(tipos, funciones, campos) van en inglés, salvo los nombres de los tests, que son frases en español
que describen lo que se verifica (`crea_cambia_y_borra_una_tabla_contra_servidores_reales`).

Los comentarios de este repositorio explican **por qué**, no qué. Antes de escribir uno, mirar los
que ya hay: casi todos justifican una decisión (por qué un cast explícito, por qué el pool tiene ese
tamaño, por qué la posición interna de un error no se muestra). Comentarios que repiten el código no
encajan con el estilo existente.

## Comandos

```bash
pnpm install                    # el CLI de Tauri vive en la raíz del workspace
pnpm dev                        # levantar la aplicación (tauri dev)
pnpm build                      # tauri build
pnpm ui:check                   # svelte-check sobre ui/
pnpm ui:test                    # vitest sobre ui/ (lógica pura, sin DOM)
pnpm ui:build                   # vite build de ui/

cargo build --workspace
cargo fmt --all --check
cargo clippy -p pgforge-core -p pgforge-cli --all-targets   # CI usa RUSTFLAGS="-D warnings"
cargo test -p pgforge-core -p pgforge-cli
cargo clippy -p pgforge-app --all-targets && cargo test -p pgforge-app   # necesita ui/dist
```

De `src-tauri` no se prueba lo que toca la red —eso vive en el core y se prueba ahí—, pero sí sus
comandos de **vista previa**, que son puros: `src-tauri/tests/preview.rs` los llama con la carga
JSON tal como la manda `ipc.ts`. Compilarlo exige que exista `ui/dist` (`generate_context!` lo
verifica), así que hay que correr `pnpm ui:build` antes; en CI eso ya lo dejó hecho el
`pnpm tauri build --no-bundle` del mismo job, que además cubre las tres plataformas. El job del core
corre contra PostgreSQL **13 y 17** — los dos extremos del rango soportado— porque es ahí donde se
nota un gating por versión mal puesto.

La CLI es la forma rápida de ejercitar el core sin levantar la ventana, y sirve para comprobar a
mano lo que un test todavía no cubre:

```bash
cargo run -p pgforge-cli -- tree  --url postgres://postgres@localhost:5432/postgres --depth 4
cargo run -p pgforge-cli -- ddl   --url postgres://postgres@localhost:5432/postgres public.clientes
cargo run -p pgforge-cli -- query --url postgres://postgres@localhost:5432/postgres --sql "SELECT 1"
```

### Tests de integración

Los tests de `crates/pgforge-core/tests/` necesitan servidores reales. Se ejecutan contra **todas**
las URLs de `PGFORGE_TEST_URLS` (separadas por coma), y la gracia es apuntar a dos versiones
distintas de PostgreSQL a la vez: eso es lo que valida que el gating por versión esté bien. Sin esa
variable **no verifican nada** y lo avisan por stderr — un test "verde" sin la variable no prueba
nada.

```powershell
$env:PGFORGE_TEST_URLS = ((Get-Content .env.local) -match 'PGFORGE_TEST_URLS=') -replace '^[^=]*='
cargo test -p pgforge-core
cargo test -p pgforge-core --test ddl_table                 # un archivo
cargo test -p pgforge-core --test ddl_table crea_cambia     # un test suelto
cargo test -p pgforge-core -- --nocapture                   # ver el "ok contra PostgreSQL X" de cada URL
```

Cada archivo de test crea su propio esquema (`pgforge_ddl_<pid>`, etc.), lo borra al final, y los
tests son `#[tokio::test]` que iteran las URLs en un bucle. Al agregar uno, seguir ese patrón:
`test_urls()` → `connect()` → `setup()` → cuerpo dentro de un `tokio::spawn` → `teardown()` →
`resume_unwind` si falló, para que el esquema se limpie aunque el test panickee.

## Arquitectura

```
crates/pgforge-core/   Núcleo: conexiones, introspección, SQL, datos, DDL, monitoreo, backups.
crates/pgforge-cli/    Binario de línea de comandos sobre el core.
src-tauri/             Aplicación de escritorio: comandos Tauri y estado.
ui/                    Svelte 5 (runes) + TypeScript + Vite + Tailwind 4.
```

**La regla que ordena todo:** la lógica vive en `pgforge-core`. Los comandos de `src-tauri` solo
traducen argumentos y delegan; `pgforge-cli` existe justamente para probar que el núcleo se puede
usar sin ventana. Si algo solo se puede hacer desde la interfaz, está en el lugar equivocado. Los
tests ejercitan el core directamente, nunca a través de Tauri.

`clippy.toml` prohíbe `print!`/`println!` en el core: quien lo consume decide cómo presentar. La CLI
los habilita con un `allow` explícito.

### Compatibilidad entre versiones de PostgreSQL

Soporte desde PG 13. **Nunca escribir una consulta atada a una versión.** `ServerCaps`
(`pgforge-core::caps`) se calcula una vez al conectar y expone predicados (`has_pg_stat_io()`,
`has_query_id()`, `has_reindex_concurrently()`, …) que cada módulo consulta para armar su SQL. Al
usar una vista o columna que no existe en todas las versiones soportadas, agregar el predicado ahí
en vez de resolverlo en el sitio de uso.

`ServerCaps` también lleva los permisos del rol conectado (`is_superuser`, `can_signal_backends`,
`can_read_all_stats`), para poder avisar antes de intentar algo que va a fallar.

Lo que depende de una **extensión** instalada (no de la versión) se gatea distinto: un `has_*`
consulta `pg_extension` y la operación devuelve un `Error::Config` claro si falta —así lo hacen
`pg_stat_statements` para las consultas costosas y `pgstattuple` para la estimación de bloat
(`monitor::stats::bloat`, que usa `pgstattuple_approx` sobre las tablas más grandes en vez de
recorrerlas enteras)—.

### Conexiones

`ConnectionManager` mantiene, por servidor conectado, un pool por cada base abierta (el árbol salta
entre bases y reconectar en cada salto sería inutilizable). Aparte del pool hay **sesiones
dedicadas**: cada pestaña de consulta tiene la suya, que es lo que hace que un `BEGIN`, un `SET` o
una tabla temporal sigan valiendo en la consulta siguiente. Cada una guarda su `CancelToken`.

El `statement_timeout` se pasa por caso de uso, no globalmente: el explorador usa el del perfil, el
monitoreo uno corto, y una tarea de mantenimiento ninguno (o el servidor mataría el `VACUUM`).

Las contraseñas nunca van a los archivos de la aplicación: el perfil guarda solo los datos del
servidor y la contraseña va al almacén del sistema operativo vía `keyring`, solo si se pide
recordarla.

**Túnel SSH** (`conn::tunnel`, con `russh`): cuando el perfil trae `tunnel`, `connect_with_ssh`
levanta un **forward local** (`TcpListener` en `127.0.0.1:<efímero>` empalmado a un canal
`direct-tcpip`) *antes* de armar el pool, y el resto del núcleo conecta a ese puerto local sin
enterarse del túnel —el TLS de PostgreSQL sigue siendo de extremo a extremo—. El `LocalForward` vive
dentro del `ServerHandle`, así que un túnel por servidor se comparte entre los pools de sus bases y
se cierra cuando se cierra el servidor. `russh` usa el backend **`ring`** a propósito (no
`aws-lc-rs`, que pediría NASM/cmake en el CI). Dos consecuencias: por un túnel `verify-full` degrada
a validar solo la cadena (la conexión termina en `127.0.0.1`, el nombre nunca coincide), lo resuelve
`tls::connector(profile, verify_hostname)`; y la clave del host se verifica contra `known_hosts` con
`HostKeyPolicy` —un host desconocido o con clave cambiada devuelve `Error::SshHostKey` con la huella
para que la interfaz confirme, nunca se acepta a ciegas—. El secreto SSH va al `keyring` bajo una
clave aparte (`{id}:ssh`), separado del de la base. El extremo a extremo con un `sshd` real es prueba
manual (`pgforge tunnel …`), no entra en `PGFORGE_TEST_URLS`.

Los servidores se agrupan en **carpetas de conexiones**: el campo `group` del perfil. No hay una
lista de carpetas guardada aparte —una carpeta es el nombre que comparten unos perfiles—, así que
`ProfileStore::groups()` las deriva, `rename_group` mueve a todos sus miembros de una sola escritura
y la carpeta desaparece sola cuando sale de ella el último servidor. El nombre pasa siempre por
`normalize_group` al guardar, porque es también la clave por la que se agrupa.

### Errores

`pgforge_core::Error` traduce los errores del servidor a variantes con significado: `Canceled` (el
usuario apretó cancelar, no es una falla), `Permission`, `Conflict` (los datos cambiaron entre la
lectura y la escritura), `Database` (con `code`, `detail`, `hint` y `position` para resaltar en el
editor). Cruza el IPC como `ErrorPayload`, un enum etiquetado con `kind`, y la interfaz lo consume
con el tipo `CoreError` de `ui/src/lib/ipc.ts` (`describeError`, `isCanceled`).

### La frontera del IPC

`ui/src/lib/ipc.ts` es el **único** lugar de la interfaz que habla con Rust: declara los tipos
espejo de los del core y envuelve cada `invoke`. Ningún componente llama a `invoke` por su cuenta.
Al agregar un comando hay que tocar tres puntos: el módulo del core, el comando en
`src-tauri/src/commands/` **más su registro en el `generate_handler!` de `src-tauri/src/lib.rs`**, y
la función y los tipos en `ipc.ts`.

Convenciones de serde que la interfaz da por hechas: `#[serde(rename_all = "camelCase")]` en todo, y
los enums como uniones etiquetadas — `kind` para las variantes de datos (`Change`, `TableChange`,
`Outcome`, `NodeKind`) y `type` para los eventos de canal (`QueryEvent`, `MonitorEvent`,
`MaintenanceEvent`). Los flujos largos (ejecución de consultas, sondeo del dashboard, mantenimiento)
no devuelven el resultado: mandan eventos por un `Channel` de Tauri.

**Un enum etiquetado necesita las dos:** `rename_all` renombra los nombres de variante y
`rename_all_fields` los campos de adentro de cada variante. Solo con la primera, un `new_name` o un
`type_name` sigue esperándose en `snake_case` mientras la interfaz manda `newName`: nada falla al
compilar y el `invoke` se cae recién en tiempo de ejecución, con un «missing field» del lado de
Rust. Por eso los enums de este proyecto llevan siempre
`#[serde(tag = "…", rename_all = "camelCase", rename_all_fields = "camelCase")]`, aunque hoy no
tengan ningún campo de dos palabras. Lo que atrapa el olvido es `src-tauri/tests/preview.rs`, que
arma la carga como JSON en vez de construir los tipos de Rust.

Los valores de las celdas viajan como `string | null` (`null` es un NULL de la base, distinto de la
cadena vacía), no como tipos nativos de JavaScript.

### Vista previa antes de aplicar

Toda mutación tiene dos comandos: uno que **genera el SQL** y otro que lo ejecuta — `ddl_preview` /
`ddl_apply`, `data_preview` / `data_apply`, `data_export_preview` / `data_export_run`,
`data_import_preview` / `data_import_run` (que generan el `COPY … TO/FROM STDIN` exacto, ver
`data::io`), `index_preview` / `index_create`, `view_preview`, `trigger_preview`, `role_preview`,
`privilege_preview`, `maintenance_plan` / `maintenance_run`, `backup_plan` / `backup_run` y
`restore_plan` / `restore_run` (que en vez de SQL generan la línea de comando de `pg_dump` y
`pg_restore`).

El import/export mueve datos en streaming por trozos (`copy_out`/`copy_in`), no acumula el archivo
en memoria, y reporta el avance por un `Channel`; el import corre en una sola transacción para no
dejar la tabla a medias. El formato binario no es portable entre versiones distintas —la interfaz lo
advierte—.

La función generadora es **pura** a propósito: es lo único verificable sin servidor, y garantiza que
lo que la interfaz muestra es exactamente lo que se va a ejecutar, no una reconstrucción parecida.
Al agregar una operación que modifica el servidor, mantener esa separación.

Reglas que sostienen la edición de datos (`data::edit`): sin clave primaria/única la grilla se abre
en solo lectura; cada `UPDATE` lleva los valores originales de las columnas que cambia y, si afecta
cero filas, se reporta `Conflict` en vez de pisar; el lote entero va en una transacción.

### Identificadores y SQL crudo

`ddl::quote_ident` cita solo cuando hace falta (mayúsculas, símbolos, palabras reservadas). Usarlo
para todo identificador que se interpole. En cambio, hay texto que va **crudo** a propósito y está
documentado como tal: el `DEFAULT` de una columna, el `USING` de un cambio de tipo, el predicado de
un índice parcial, la expresión de un `CHECK`, el nombre de tipo de una columna. No se pueden
parametrizar en DDL, los valida el servidor al ejecutar, y es la misma frontera de confianza que el
editor de consultas: lo ejecuta el propio usuario con sus propios privilegios.

Para el DDL de lectura se prefiere siempre la función del servidor (`pg_get_viewdef`,
`pg_get_indexdef`, `pg_get_functiondef`, `pg_get_constraintdef`, `pg_get_triggerdef`); las tablas,
que no tienen equivalente, se delegan a `pg_dump`. El campo `DdlSource` dice de cuál de los dos
salió, porque no es lo mismo lo que afirma el servidor que lo que reconstruyó una herramienta
externa.

Los binarios externos (`pg_dump`, `pg_restore`) se ubican en un solo lugar, `backup::tools`:
variable de entorno por herramienta → `PATH` → rutas típicas de cada sistema, quedándose con la
versión más alta. Antes de usarlos hay que comparar su versión con la del servidor — pueden leer
servidores más viejos que ellos, nunca más nuevos — y eso conviene detectarlo antes de empezar, no
a los diez minutos.

### Interfaz

Svelte 5 con runes. El estado compartido son clases con campos `$state` exportadas como instancia
única desde archivos `*.svelte.ts` (`explorer`, `tabs`, `theme`, y las clases `QueryTab`/`DataTab`
que extienden `Tab`). No hay stores de Svelte 4 ni librería de estado.

Las pestañas de consulta y de datos conviven en una sola barra (`tabs.svelte.ts`); cerrar una llama
a su `dispose()`, que es donde se suelta la sesión del lado de Rust. El tema resuelto se escribe en
`document.documentElement.dataset.theme` para que Tailwind, CodeMirror y uPlot lean un solo atributo.

Editor SQL: CodeMirror 6 (`@codemirror/lang-sql`). Gráficos: uPlot.

#### Estilos

`ui/src/app.css` no es solo el import de Tailwind: tiene una **capa de componentes** con las piezas
que se repiten en toda la aplicación — `.card`, `.panel`, `.toolbar`, `.btn` (`btn-primary`,
`btn-danger`, `btn-ghost`, `btn-icon`, `btn-sm`), `.field`, `.label`, `.check`, `.seg`, `.tag-*`,
`.alert-*`, `.list-table`, `.row-actions`, `.spinner`, `.muted`, `.divider-*`. Antes de escribir una
cadena de clases de Tailwind para un botón, un campo o una tarjeta, usar la clase que ya existe; si
hace falta una variante nueva, va ahí y no repetida en cada componente.

Dos consecuencias de ese archivo que sorprenden si no se leyó: el `<body>` tiene `user-select: none`
porque la ventana es una aplicación y no una página —lo que se debe poder copiar (SQL, DDL, valores)
pide `select-text` explícitamente—, y CodeMirror y uPlot no reciben clases sino variables CSS
(`--cm-*`, `--plot-*`) declaradas por tema, para que el color lo siga decidiendo un solo lugar.

Los íconos son SVG dibujados a mano en `Icon.svelte` (una librería completa pesa más que toda la
interfaz); el ícono y el color de cada tipo de nodo salen de `lookOf` en `badges.ts`, que agrupa por
familia. Un tipo de objeto nuevo se agrega en esos dos lugares, no en el componente que lo muestra.

Los rasgos que el árbol muestra como pastillas de color (que un rol pueda iniciar sesión, que una
tabla tenga RLS activa) son el `NodeTag` del núcleo, un vocabulario cerrado: el `TreeNode` los trae
en `tags` y `tagLook` en `badges.ts` les pone texto y tono. Van separados de `detail`, que es texto
libre para leer. Un rasgo nuevo se agrega en esos dos lugares.

#### Diálogos

`Modal.svelte` es el diálogo de toda la aplicación y ya resuelve Escape, clic afuera, trampa de foco
y devolución del foco al cerrar; `busy` impide que se cierre a mitad de un guardado y `data-autofocus`
elige qué campo recibe el foco. Ningún diálogo vuelve a implementar eso por su cuenta. Al lado están
`Confirm.svelte` (acciones que no se deshacen), `Alert.svelte` (`tone` + `box`), `SqlPreview.svelte`
y `Empty.svelte`.

Los formularios de mutación (`PolicyDialog`, `RoleDialog`, `TableDialog`, …) siguen todos la misma
forma, que es la cara visible de la regla de vista previa: copia editable de los props tomada una
sola vez con `untrack`, una función `changes()` que arma el cambio, `validate()` que devuelve el
problema o `null`, un botón «Ver SQL» que llama al `*_preview` y un `submit()` que llama al
`*_apply`. Al agregar un objeto nuevo, copiar esa estructura en vez de inventar otra.

`changes()` y `validate()` son funciones puras de lo que hay escrito en pantalla, y montar el
componente para probarlas no aporta nada. Donde la lógica es de verdad —un diff contra lo que hay en
el servidor, un campo que solo vale para algunos casos— se saca a un `<objeto>-form.ts` al lado del
diálogo (`role-form.ts`, `policy-form.ts`): el componente queda con el `$state` y las bindings, y el
archivo suelto se prueba con Vitest. Los diálogos que solo juntan campos y los mandan no necesitan
esa separación.

## Nota sobre `plan-proyecto-pgtool-rust.md`

Es el documento de visión original y su tabla de stack está **desactualizada**: el proyecto usa
`tokio-postgres` + `deadpool` (no `sqlx`), CodeMirror (no Monaco), Svelte 5 con grillas propias (no
TanStack Table) y las instancias locales de PostgreSQL vía `PGFORGE_TEST_URLS` (no
`testcontainers`). Sirve para el alcance por fases; para el stack, mandan `Cargo.toml` y
`ui/package.json`.
