# CLAUDE.md

Guía para Claude Code (claude.ai/code) en este repositorio.

## Idioma

Todo proyecto en español: comentarios, doc-comments, nombres de tests, mensajes de error, textos de interfaz, mensajes de commit. Mantener así. Identificadores de código (tipos, funciones, campos) en inglés — salvo nombres de tests, que son frases en español describiendo qué se verifica (`crea_cambia_y_borra_una_tabla_contra_servidores_reales`).

Comentarios explican **por qué**, no qué. Antes de escribir uno, mirar los existentes: casi todos justifican decisión (por qué cast explícito, por qué pool tiene ese tamaño, por qué posición interna de error no se muestra). Comentario que repite código no encaja con estilo.

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

De `src-tauri` no se prueba lo que toca red —eso vive en core y se prueba ahí—, pero sí sus comandos de **vista previa**, que son puros: `src-tauri/tests/preview.rs` los llama con la carga JSON tal como manda `ipc.ts`. Compilarlo exige que exista `ui/dist` (`generate_context!` lo verifica): correr `pnpm ui:build` antes. En CI ya lo dejó hecho el `pnpm tauri build --no-bundle` del mismo job, que además cubre las tres plataformas. Job del core corre contra PostgreSQL **13 y 17** —extremos del rango soportado— porque ahí se nota gating por versión mal puesto.

CLI = forma rápida de ejercitar core sin levantar ventana; sirve para comprobar a mano lo que test todavía no cubre:

```bash
cargo run -p pgforge-cli -- tree  --url postgres://postgres@localhost:5432/postgres --depth 4
cargo run -p pgforge-cli -- ddl   --url postgres://postgres@localhost:5432/postgres public.clientes
cargo run -p pgforge-cli -- query --url postgres://postgres@localhost:5432/postgres --sql "SELECT 1"
```

### Tests de integración

Tests de `crates/pgforge-core/tests/` necesitan servidores reales. Se ejecutan contra **todas** las URLs de `PGFORGE_TEST_URLS` (separadas por coma). Gracia: apuntar a dos versiones distintas de PostgreSQL a la vez — eso valida que gating por versión esté bien. Sin esa variable **no verifican nada** y avisan por stderr: test "verde" sin la variable no prueba nada.

```powershell
$env:PGFORGE_TEST_URLS = ((Get-Content .env.local) -match 'PGFORGE_TEST_URLS=') -replace '^[^=]*='
cargo test -p pgforge-core
cargo test -p pgforge-core --test ddl_table                 # un archivo
cargo test -p pgforge-core --test ddl_table crea_cambia     # un test suelto
cargo test -p pgforge-core -- --nocapture                   # ver el "ok contra PostgreSQL X" de cada URL
```

Cada archivo de test crea su propio esquema (`pgforge_ddl_<pid>`, etc.), lo borra al final; tests son `#[tokio::test]` que iteran URLs en bucle. Al agregar uno, seguir ese patrón: `test_urls()` → `connect()` → `setup()` → cuerpo dentro de `tokio::spawn` → `teardown()` → `resume_unwind` si falló, para que esquema se limpie aunque test panickee.

## Arquitectura

```
crates/pgforge-core/   Núcleo: conexiones, introspección, SQL, datos, DDL, monitoreo, backups.
crates/pgforge-cli/    Binario de línea de comandos sobre el core.
src-tauri/             Aplicación de escritorio: comandos Tauri y estado.
ui/                    Svelte 5 (runes) + TypeScript + Vite + Tailwind 4.
```

**Regla que ordena todo:** lógica vive en `pgforge-core`. Comandos de `src-tauri` solo traducen argumentos y delegan; `pgforge-cli` existe justamente para probar que núcleo se puede usar sin ventana. Si algo solo se puede hacer desde interfaz, está en lugar equivocado. Tests ejercitan core directamente, nunca vía Tauri.

`clippy.toml` prohíbe `print!`/`println!` en core: quien consume decide cómo presentar. CLI los habilita con `allow` explícito.

### Compatibilidad entre versiones de PostgreSQL

Soporte desde PG 13. **Nunca escribir consulta atada a una versión.** `ServerCaps` (`pgforge-core::caps`) se calcula una vez al conectar y expone predicados (`has_pg_stat_io()`, `has_query_id()`, `has_reindex_concurrently()`, …) que cada módulo consulta para armar su SQL. Al usar vista o columna que no existe en todas las versiones soportadas, agregar predicado ahí, no resolverlo en sitio de uso.

`ServerCaps` también lleva permisos del rol conectado (`is_superuser`, `can_signal_backends`, `can_read_all_stats`), para avisar antes de intentar algo que va a fallar.

Lo que depende de **extensión** instalada (no de versión) se gatea distinto: un `has_*` consulta `pg_extension` y la operación devuelve `Error::Config` claro si falta. Así lo hacen `pg_stat_statements` para consultas costosas y `pgstattuple` para estimación de bloat (`monitor::stats::bloat`, que usa `pgstattuple_approx` sobre tablas más grandes en vez de recorrerlas enteras).

### Conexiones

`ConnectionManager` mantiene, por servidor conectado, un pool por cada base abierta (árbol salta entre bases; reconectar en cada salto sería inutilizable). Aparte del pool hay **sesiones dedicadas**: cada pestaña de consulta tiene la suya — eso hace que `BEGIN`, `SET` o tabla temporal sigan valiendo en la consulta siguiente. Cada una guarda su `CancelToken`.

`statement_timeout` se pasa por caso de uso, no globalmente: explorador usa el del perfil, monitoreo uno corto, tarea de mantenimiento ninguno (o servidor mataría el `VACUUM`).

Contraseñas nunca van a archivos de la aplicación: perfil guarda solo datos del servidor y contraseña va al almacén del sistema operativo vía `keyring`, solo si se pide recordarla.

**Túnel SSH** (`conn::tunnel`, con `russh`): cuando perfil trae `tunnel`, `connect_with_ssh` levanta **forward local** (`TcpListener` en `127.0.0.1:<efímero>` empalmado a canal `direct-tcpip`) *antes* de armar el pool, y resto del núcleo conecta a ese puerto local sin enterarse del túnel —TLS de PostgreSQL sigue siendo extremo a extremo—. `LocalForward` vive dentro del `ServerHandle`: un túnel por servidor se comparte entre pools de sus bases y se cierra al cerrar servidor. `russh` usa backend **`ring`** a propósito (no `aws-lc-rs`, que pediría NASM/cmake en CI). Dos consecuencias: por túnel, `verify-full` degrada a validar solo la cadena (conexión termina en `127.0.0.1`, nombre nunca coincide) — lo resuelve `tls::connector(profile, verify_hostname)`; y clave del host se verifica contra `known_hosts` con `HostKeyPolicy` —host desconocido o con clave cambiada devuelve `Error::SshHostKey` con la huella para que interfaz confirme, nunca se acepta a ciegas—. Secreto SSH va al `keyring` bajo clave aparte (`{id}:ssh`), separado del de la base. Extremo a extremo con `sshd` real es prueba manual (`pgforge tunnel …`), no entra en `PGFORGE_TEST_URLS`.

Servidores se agrupan en **carpetas de conexiones**: campo `group` del perfil. No hay lista de carpetas guardada aparte —carpeta = nombre que comparten unos perfiles—, así que `ProfileStore::groups()` las deriva, `rename_group` mueve a todos sus miembros en una sola escritura, y carpeta desaparece sola cuando sale el último servidor. Nombre pasa siempre por `normalize_group` al guardar, porque es también la clave por la que se agrupa.

### Errores

`pgforge_core::Error` traduce errores del servidor a variantes con significado: `Canceled` (usuario apretó cancelar, no es falla), `Permission`, `Conflict` (datos cambiaron entre lectura y escritura), `Database` (con `code`, `detail`, `hint` y `position` para resaltar en editor). Cruza el IPC como `ErrorPayload`, enum etiquetado con `kind`; interfaz lo consume con tipo `CoreError` de `ui/src/lib/ipc.ts` (`describeError`, `isCanceled`).

### La frontera del IPC

`ui/src/lib/ipc.ts` = **único** lugar de la interfaz que habla con Rust: declara tipos espejo de los del core y envuelve cada `invoke`. Ningún componente llama a `invoke` por su cuenta. Al agregar comando hay que tocar tres puntos: módulo del core, comando en `src-tauri/src/commands/` **más su registro en el `generate_handler!` de `src-tauri/src/lib.rs`**, y la función y tipos en `ipc.ts`.

Convenciones de serde que interfaz da por hechas: `#[serde(rename_all = "camelCase")]` en todo, y enums como uniones etiquetadas — `kind` para variantes de datos (`Change`, `TableChange`, `Outcome`, `NodeKind`) y `type` para eventos de canal (`QueryEvent`, `MonitorEvent`, `MaintenanceEvent`). Flujos largos (ejecución de consultas, sondeo del dashboard, mantenimiento) no devuelven resultado: mandan eventos por `Channel` de Tauri.

**Enum etiquetado necesita las dos:** `rename_all` renombra nombres de variante, `rename_all_fields` los campos de adentro de cada variante. Solo con la primera, un `new_name` o `type_name` sigue esperándose en `snake_case` mientras interfaz manda `newName`: nada falla al compilar y el `invoke` se cae recién en tiempo de ejecución, con «missing field» del lado de Rust. Por eso los enums de este proyecto llevan siempre `#[serde(tag = "…", rename_all = "camelCase", rename_all_fields = "camelCase")]`, aunque hoy no tengan campo de dos palabras. Lo que atrapa el olvido: `src-tauri/tests/preview.rs`, que arma la carga como JSON en vez de construir tipos de Rust.

Valores de celdas viajan como `string | null` (`null` = NULL de la base, distinto de cadena vacía), no como tipos nativos de JavaScript.

### Vista previa antes de aplicar

Toda mutación tiene dos comandos: uno que **genera el SQL** y otro que lo ejecuta — `ddl_preview` / `ddl_apply`, `data_preview` / `data_apply`, `data_export_preview` / `data_export_run`, `data_import_preview` / `data_import_run` (generan el `COPY … TO/FROM STDIN` exacto, ver `data::io`), `index_preview` / `index_create`, `view_preview`, `trigger_preview`, `role_preview`, `privilege_preview`, `maintenance_plan` / `maintenance_run`, `backup_plan` / `backup_run` y `restore_plan` / `restore_run` (que en vez de SQL generan la línea de comando de `pg_dump` y `pg_restore`).

Import/export mueve datos en streaming por trozos (`copy_out`/`copy_in`), no acumula archivo en memoria, y reporta avance por `Channel`; import corre en una sola transacción para no dejar tabla a medias. Formato binario no es portable entre versiones distintas —interfaz lo advierte—.

Función generadora es **pura** a propósito: único verificable sin servidor, y garantiza que lo que interfaz muestra es exactamente lo que se va a ejecutar, no reconstrucción parecida. Al agregar operación que modifica servidor, mantener esa separación.

Reglas que sostienen edición de datos (`data::edit`): sin clave primaria/única, grilla se abre en solo lectura; cada `UPDATE` lleva valores originales de las columnas que cambia y, si afecta cero filas, reporta `Conflict` en vez de pisar; lote entero va en una transacción.

### Identificadores y SQL crudo

`ddl::quote_ident` cita solo cuando hace falta (mayúsculas, símbolos, palabras reservadas). Usarlo para todo identificador que se interpole. En cambio, hay texto que va **crudo** a propósito y está documentado como tal: `DEFAULT` de columna, `USING` de cambio de tipo, predicado de índice parcial, expresión de `CHECK`, nombre de tipo de columna. No se pueden parametrizar en DDL, los valida el servidor al ejecutar, y es la misma frontera de confianza que el editor de consultas: lo ejecuta el propio usuario con sus propios privilegios.

Para DDL de lectura se prefiere siempre función del servidor (`pg_get_viewdef`, `pg_get_indexdef`, `pg_get_functiondef`, `pg_get_constraintdef`, `pg_get_triggerdef`); tablas, sin equivalente, se delegan a `pg_dump`. Campo `DdlSource` dice de cuál de los dos salió, porque no es lo mismo lo que afirma el servidor que lo que reconstruyó herramienta externa.

Binarios externos (`pg_dump`, `pg_restore`) se ubican en un solo lugar, `backup::tools`: variable de entorno por herramienta → `PATH` → rutas típicas de cada sistema, quedándose con la versión más alta. Antes de usarlos hay que comparar su versión con la del servidor —pueden leer servidores más viejos que ellos, nunca más nuevos— y conviene detectarlo antes de empezar, no a los diez minutos.

### Interfaz

Svelte 5 con runes. Estado compartido = clases con campos `$state` exportadas como instancia única desde archivos `*.svelte.ts` (`explorer`, `tabs`, `theme`, y clases `QueryTab`/`DataTab` que extienden `Tab`). No hay stores de Svelte 4 ni librería de estado.

Pestañas de consulta y de datos conviven en una sola barra (`tabs.svelte.ts`); cerrar una llama a su `dispose()`, donde se suelta la sesión del lado de Rust. Tema resuelto se escribe en `document.documentElement.dataset.theme` para que Tailwind, CodeMirror y uPlot lean un solo atributo.

Editor SQL: CodeMirror 6 (`@codemirror/lang-sql`). Gráficos: uPlot.

#### Estilos

`ui/src/app.css` no es solo el import de Tailwind: tiene **capa de componentes** con las piezas repetidas en toda la aplicación — `.card`, `.panel`, `.toolbar`, `.btn` (`btn-primary`, `btn-danger`, `btn-ghost`, `btn-icon`, `btn-sm`), `.field`, `.label`, `.check`, `.seg`, `.tag-*`, `.alert-*`, `.list-table`, `.row-actions`, `.spinner`, `.muted`, `.divider-*`. Antes de escribir cadena de clases Tailwind para botón, campo o tarjeta, usar la clase que ya existe; si hace falta variante nueva, va ahí, no repetida en cada componente.

Dos consecuencias de ese archivo que sorprenden si no se leyó: `<body>` tiene `user-select: none` porque la ventana es aplicación, no página —lo que se debe poder copiar (SQL, DDL, valores) pide `select-text` explícitamente—; y CodeMirror y uPlot no reciben clases sino variables CSS (`--cm-*`, `--plot-*`) declaradas por tema, para que color lo siga decidiendo un solo lugar.

Íconos = SVG dibujados a mano en `Icon.svelte` (librería completa pesa más que toda la interfaz); ícono y color de cada tipo de nodo salen de `lookOf` en `badges.ts`, que agrupa por familia. Tipo de objeto nuevo se agrega en esos dos lugares, no en el componente que lo muestra.

Rasgos que árbol muestra como pastillas de color (rol puede iniciar sesión, tabla tiene RLS activa) son el `NodeTag` del núcleo, vocabulario cerrado: `TreeNode` los trae en `tags` y `tagLook` en `badges.ts` les pone texto y tono. Van separados de `detail`, texto libre para leer. Rasgo nuevo se agrega en esos dos lugares.

#### Diálogos

`Modal.svelte` es el diálogo de toda la aplicación y ya resuelve Escape, clic afuera, trampa de foco y devolución del foco al cerrar; `busy` impide que se cierre a mitad de un guardado, `data-autofocus` elige qué campo recibe foco. Ningún diálogo reimplementa eso por su cuenta. Al lado: `Confirm.svelte` (acciones que no se deshacen), `Alert.svelte` (`tone` + `box`), `SqlPreview.svelte` y `Empty.svelte`.

Formularios de mutación (`PolicyDialog`, `RoleDialog`, `TableDialog`, …) siguen todos la misma forma, cara visible de la regla de vista previa: copia editable de props tomada una sola vez con `untrack`, función `changes()` que arma el cambio, `validate()` que devuelve el problema o `null`, botón «Ver SQL» que llama al `*_preview`, y `submit()` que llama al `*_apply`. Al agregar objeto nuevo, copiar esa estructura en vez de inventar otra.

`changes()` y `validate()` son funciones puras de lo escrito en pantalla; montar el componente para probarlas no aporta nada. Donde lógica es de verdad —diff contra lo que hay en servidor, campo que solo vale para algunos casos— se saca a `<objeto>-form.ts` al lado del diálogo (`role-form.ts`, `policy-form.ts`): componente queda con `$state` y bindings, archivo suelto se prueba con Vitest. Diálogos que solo juntan campos y los mandan no necesitan esa separación.

## Nota sobre `plan-proyecto-pgtool-rust.md`

Documento de visión original; su tabla de stack está **desactualizada**: proyecto usa `tokio-postgres` + `deadpool` (no `sqlx`), CodeMirror (no Monaco), Svelte 5 con grillas propias (no TanStack Table) e instancias locales de PostgreSQL vía `PGFORGE_TEST_URLS` (no `testcontainers`). Sirve para alcance por fases; para stack mandan `Cargo.toml` y `ui/package.json`.