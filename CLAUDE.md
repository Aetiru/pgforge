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
cargo deny check                # vulnerabilidades, licencias y registros de las dependencias
```

`cargo deny` corre en CI en su propio job (no compila nada: lee `Cargo.lock`). Reglas en `deny.toml`: una vulnerabilidad publicada frena el build, y las excepciones van con el motivo escrito —hoy una sola, el `rsa` que arrastra `russh`, que no tiene versión corregida—. «Sin mantenimiento» solo frena si la caja la pide este workspace: lo que llega por abajo de Tauri no se puede accionar desde acá. Todos los `Cargo.toml` llevan `publish = false`: nada de esto va a crates.io.

De `src-tauri` no se prueba lo que toca red —eso vive en core y se prueba ahí—, pero sí sus comandos de **vista previa**, que son puros: `src-tauri/tests/preview.rs` los llama con la carga JSON tal como manda `ipc.ts`. Compilarlo exige que exista `ui/dist` (`generate_context!` lo verifica): correr `pnpm ui:build` antes. En CI ya lo dejó hecho el `pnpm tauri build --no-bundle` del mismo job, que además cubre las tres plataformas. Job del core corre contra **todo el rango soportado, PostgreSQL 13 a 17**, una instancia por versión, porque ahí se nota gating por versión mal puesto —los extremos atrapan lo que se sale del rango; las del medio, el salto de catálogo atado a una versión intermedia—.

CLI = forma rápida de ejercitar core sin levantar ventana; sirve para comprobar a mano lo que test todavía no cubre:

```bash
cargo run -p pgforge-cli -- tree  --url postgres://postgres@localhost:5432/postgres --depth 4
cargo run -p pgforge-cli -- graph --url postgres://postgres@localhost:5432/postgres public
cargo run -p pgforge-cli -- search --url postgres://postgres@localhost:5432/postgres cliente
cargo run -p pgforge-cli -- ddl   --url postgres://postgres@localhost:5432/postgres public.clientes
cargo run -p pgforge-cli -- query --url postgres://postgres@localhost:5432/postgres --sql "SELECT 1"
cargo run -p pgforge-cli -- update   # sin servidor: pregunta a GitHub si hay una versión más nueva
cargo run -p pgforge-cli -- compare --url postgres://postgres@localhost:5434/postgres \
    --target-url postgres://postgres@localhost:5438/postgres --schema public
cargo run -p pgforge-cli -- data  --url postgres://postgres@localhost:5432/postgres public.clientes \
    --order creado --desc --where "estado = 'activo'" --limit 20
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

**Los permisos no solo cambian qué se puede hacer: cambian qué columnas llegan.** Un rol sin `pg_read_all_stats` ve las filas de `pg_stat_activity` de sesiones ajenas con casi todo en NULL (`backend_type`, `state`, `datname`; `query` como `<insufficient privilege>`), y uno sin `pg_read_all_settings` ve `pg_settings.setting` en NULL para los parámetros marcados `GUC_SUPERUSER_ONLY`. `Row::get` sobre un tipo que no es `Option` **hace panic** ahí. Al mapear cualquier vista del catálogo con visibilidad por rol, la columna va como `Option` aunque contra un superusuario nunca venga vacía; hay un test contra servidores reales (`tests/monitor_permisos.rs`) que hace `SET ROLE` a un rol sin privilegios justo para eso.

Lo que depende de **extensión** instalada (no de versión) se gatea distinto: un `has_*` consulta `pg_extension` y la operación devuelve `Error::Config` claro si falta. Así lo hacen `pg_stat_statements` para consultas costosas y `pgstattuple` para estimación de bloat (`monitor::stats::bloat`, que usa `pgstattuple_approx` sobre tablas más grandes en vez de recorrerlas enteras).

### Conexiones

`ConnectionManager` mantiene, por servidor conectado, un pool por cada base abierta (árbol salta entre bases; reconectar en cada salto sería inutilizable). Aparte del pool hay **sesiones dedicadas**: cada pestaña de consulta tiene la suya — eso hace que `BEGIN`, `SET` o tabla temporal sigan valiendo en la consulta siguiente. Cada una guarda su `CancelToken`.

`statement_timeout` se pasa por caso de uso, no globalmente: explorador usa el del perfil, monitoreo uno corto, tarea de mantenimiento ninguno (o servidor mataría el `VACUUM`). Va como opción de arranque (`options`) y no como `SET` posterior, para que también valga en conexiones recicladas del pool; como `Config::options` reemplaza en vez de acumular, todas las opciones se arman en una lista y se pasan de una sola vez.

Tres campos del perfil que no cambian cómo se conecta sino qué se permite: `environment` (`dev`/`test`/`prod`, `None` = sin marcar) pinta el servidor en árbol, detalle, barra de consulta y pestañas, y hace que toda mutación pase por una confirmación extra (`ui/src/lib/access.svelte.ts`); `read_only` agrega `-c default_transaction_read_only=on` a esas mismas opciones de arranque, así **rechaza el servidor** y no una lista de operaciones que hay que acordarse de tapar —vale igual para explorador, editor de SQL, importación y mantenimiento—; `autocommit` es el valor inicial de cada pestaña de consulta. Los tres llevan `#[serde(default)]`: `connections.json` no tiene versión ni migraciones, y un perfil viejo tiene que seguir abriendo.

**Transacciones de la pestaña de consulta** (`sql::exec`): `TxStatus` (`Idle`/`Active`/`Failed`) no se deduce del SQL escrito, se le pregunta al servidor con `SELECT now() <> statement_timestamp()` — `tokio-postgres` descarta el byte de estado de `ReadyForQuery` y no lo expone. Adentro de una transacción abortada esa sonda falla con `25P02`, que es exactamente el tercer estado. Con autocommit apagado, `begin_if_needed()` antepone el `BEGIN` antes de la primera sentencia: PostgreSQL no tiene modo autocommit del lado del servidor. Encender autocommit **no confirma** la transacción abierta; solo deja de abrir una nueva.

Contraseñas nunca van a archivos de la aplicación: perfil guarda solo datos del servidor y contraseña va al almacén del sistema operativo vía `keyring`, solo si se pide recordarla.

**Túnel SSH** (`conn::tunnel`, con `russh`): cuando perfil trae `tunnel`, `connect_with_ssh` levanta **forward local** (`TcpListener` en `127.0.0.1:<efímero>` empalmado a canal `direct-tcpip`) *antes* de armar el pool, y resto del núcleo conecta a ese puerto local sin enterarse del túnel —TLS de PostgreSQL sigue siendo extremo a extremo—. `LocalForward` vive dentro del `ServerHandle`: un túnel por servidor se comparte entre pools de sus bases y se cierra al cerrar servidor. `russh` usa backend **`ring`** a propósito (no `aws-lc-rs`, que pediría NASM/cmake en CI). Dos consecuencias: por túnel, el nombre al que conecta el cliente (`127.0.0.1`) no es el del servidor que presenta el certificado, así que `tls::connector(profile, server_name)` envuelve al conector de `rustls` en un `Connector` propio que reemplaza el nombre por el del perfil —es el que rustls valida y el que viaja en el SNI—, y así `verify-full` sigue validando el nombre a través del túnel en vez de degradar a validar solo la cadena; y clave del host se verifica contra `known_hosts` con `HostKeyPolicy` —host desconocido o con clave cambiada devuelve `Error::SshHostKey` con la huella para que interfaz confirme, nunca se acepta a ciegas—. Secreto SSH va al `keyring` bajo clave aparte (`{id}:ssh`), separado del de la base. Extremo a extremo con `sshd` real es prueba manual (`pgforge tunnel …`), no entra en `PGFORGE_TEST_URLS`.

Servidores se agrupan en **carpetas de conexiones**: campo `group` del perfil. No hay lista de carpetas guardada aparte —carpeta = nombre que comparten unos perfiles—, así que `ProfileStore::groups()` las deriva, `rename_group` mueve a todos sus miembros en una sola escritura, y carpeta desaparece sola cuando sale el último servidor. Nombre pasa siempre por `normalize_group` al guardar, porque es también la clave por la que se agrupa. **Carpetas anidan con `/`**: `Clientes/ACME` no es entidad aparte de `Clientes`, es nombre con un tramo más —por eso `normalize_group` normaliza tramo por tramo, `groups()` agrega las intermedias aunque no tengan servidores propios, y `rename_group` arrastra a las de adentro (deshacer sube un escalón en vez de hacerlas desaparecer)—.

### Aviso de versión nueva

`pgforge-core::update` **no** es el actualizador firmado de Tauri: no descarga ni instala nada. Le pregunta a la API de releases de GitHub —repositorio sacado de `CARGO_PKG_REPOSITORY`, no de una constante a mano— cuál es la última publicada y, si supera a la que corre, la interfaz ofrece abrir su página. Elegir esto en vez de `tauri-plugin-updater` evita par de claves de firma, secreto de CI y `latest.json`, y sobre todo no pide confiar en un binario que se reemplaza solo; el día que haya release de verdad contra la cual probarlo, el plugin firmado se enchufa detrás del mismo cartel.

Qué release cuenta como más nueva es **función pura** (`newer_than`, con `Version` propia: comparar tres números y saber que `-rc.1` va antes que la final no justifica traer `semver`), así que se prueba sin red; lo único que necesita internet es traer la lista. Se descartan borradores y prelanzamientos. El HTTP es `reqwest` con `rustls-no-provider` —la variante `rustls` prende `aws-lc-rs`, que pide NASM y cmake en Windows, lo mismo que se evita en `russh`— y el TLS se arma en `conn::tls::web_config`, para que la elección de proveedor criptográfico y de raíces siga siendo una sola en todo el proyecto.

Cada cuánto se pregunta y qué versión se descartó viven en la interfaz (`update.svelte.ts`, `localStorage`): una vez por día, porque la API sin autenticar permite 60 pedidos por hora y por IP, y «Ahora no» silencia esa versión y no el aviso entero. Que la comprobación falle **no se muestra**: un cartel rojo porque no había internet al abrir es peor que no enterarse. `update_open` exige que la dirección empiece por la de las releases del repositorio, así que una respuesta rara de la API no puede abrir cualquier cosa. Desde la línea de comandos, `pgforge update`.

### Historial y consultas guardadas

Las dos cosas viven en SQLite adentro del directorio de configuración, pero en **archivos distintos** (`history.db` y `saved.db`), porque el `user_version` del esquema es del archivo y porque no son lo mismo: el historial (`sql::history`) es lo que pasó —se anota solo y se vacía entero sin que duela—, y `sql::saved` es lo que el usuario decidió conservar, con nombre. De ahí que ahí el nombre sea obligatorio y único sin distinguir mayúsculas, y que guardar con un nombre ocupado devuelva `Conflict` en vez de pisar: perder en silencio lo único que se pidió conservar es el peor final posible. Reescribir se pide con el `id`, y entonces la fecha de creación no se toca.

Contra qué servidor y qué base se escribió se guarda como dato, no como atadura: correr el mismo `SELECT` contra desarrollo y contra producción es justo lo que se hace, así que ni se exige que el perfil siga existiendo ni se filtra por él. El historial busca en el servidor —crece sin techo— y las guardadas se filtran en la interfaz: son decenas, puestas a mano de a una. La pestaña recuerda de cuál salió (`QueryTab.savedId`), que es lo que hace que volver a guardar reescriba esa y no deje una copia; es independiente de `filePath`, que es un archivo del disco y otra cosa.

### Comparar esquemas

`pgforge-core::compare` responde qué tiene el esquema de un servidor que no tiene el de otro, y con qué SQL se los iguala. Tres piezas: `snapshot` es lo único que habla con el servidor, `diff` compara dos instantáneas y `sync` arma el script. Las dos últimas son **puras**, que es donde vive el riesgo: un `ALTER` mal armado se ve leyendo el texto, no conectando. `render` escribe el texto de cada objeto y lo usan las dos —lo que la interfaz muestra lado a lado es exactamente lo que el script ejecutaría, no una reconstrucción parecida—.

**No hay `compare_apply`**, y es la regla de vista previa llevada hasta el final: la comparación termina en un informe y un script que se copia o se abre en una pestaña de consulta *contra el destino*. Cada sentencia viaja con su `Risk` (`safe` / `review` / `destructive`) y el orden de salida es el orden en que se puede correr —tipos, secuencias, tablas, restricciones, índices, vistas, y lo destructivo al final, para poder cortar el script ahí—. Lo que PostgreSQL no puede hacer con un `ALTER` no se inventa: va a `SyncPlan::warnings` con el motivo (el tipo base de un dominio, el orden de una enumeración, una columna generada).

Se comparan tablas con sus columnas, restricciones e índices; vistas y materializadas; secuencias; y tipos —enumeraciones, compuestos, dominios y rangos—. Quedan afuera funciones, disparadores, políticas y permisos: el cuerpo de una función difiere por un espacio y el ruido tapa lo que importa. También quedan afuera las particiones (cuelgan de su madre: una diferencia aparecería repetida una vez por partición), las secuencias de una columna `serial`/`identity` (son parte de esa columna) y los datos —`last_value` es estado, no estructura—.

Dos normalizaciones evitan diferencias que no lo son. `retarget` reescribe las referencias al esquema origen con el nombre del destino, que es lo mismo que hace falta para poder ejecutar el script del otro lado; sin eso, comparar `dev.pedidos` con `prod.pedidos` marcaría cada índice como distinto. Y el cuerpo de una vista se compara **también sin las calificaciones de columna, pero solo entre versiones mayores distintas**: PG 16 dejó de escribir `clientes.id` y pasó a escribir `id` en `pg_get_viewdef`, así que sin esa segunda pasada toda vista aparecería cambiada justo al comparar antes de una actualización. Entre servidores de la misma versión la comparación queda exacta, porque ahí una calificación de más la escribió alguien. Para no confundir `esquema.tabla` con `tabla.columna` sin analizar SQL, la instantánea trae los nombres de todos los esquemas de su base.

De la interfaz: la pestaña es `CompareTab` (`compare.svelte.ts`), se abre desde el clic derecho de un esquema o el botón del panel de detalle, y `CompareDialog` elige el otro lado entre los servidores **conectados** —leer en vivo de los dos lados es la premisa, y conectar desde ahí pediría contraseña fuera del único lugar donde se piden—. El filtro por riesgo y el armado del texto son de la interfaz (`compare-script.ts`, puro y con Vitest) porque destildar «lo destructivo» tiene que rearmar el script en el acto; el equivalente para la línea de comandos es `compare::sync::script`. Desde la CLI: `pgforge compare --url … --target-url … --schema public [--sql]`.

### Errores

`pgforge_core::Error` traduce errores del servidor a variantes con significado: `Canceled` (usuario apretó cancelar, no es falla), `Permission`, `Conflict` (datos cambiaron entre lectura y escritura), `Database` (con `code`, `detail`, `hint` y `position` para resaltar en editor). Cruza el IPC como `ErrorPayload`, enum etiquetado con `kind`; interfaz lo consume con tipo `CoreError` de `ui/src/lib/ipc/core.ts` (`describeError`, `isCanceled`).

`Connection` y `ConnectionClosed` cruzan como **`Disconnected`**, aparte de `Other`, porque no son errores de la operación sino del vínculo: cualquier otra cosa contra ese servidor va a fallar igual. El `invoke` de `ipc/core.ts` los detecta al pasar y avisa por `onServerDown` —una sola vez, en un solo lugar— para que `explorer.markDown` pinte el servidor caído en el árbol con su botón de reconectar. `Row.down` es distinto de `connected: false` a propósito: bajar `connected` cierra todas las pestañas de ese servidor (efecto de `App.svelte`), y perder una consulta a medio escribir por un corte de tres segundos es peor que el corte.

### La frontera del IPC

`ui/src/lib/ipc/` = **único** lugar de la interfaz que habla con Rust: declara tipos espejo de los del core y envuelve cada `invoke`. Ningún componente llama a `invoke` por su cuenta. Al agregar comando hay que tocar tres puntos: módulo del core, comando en `src-tauri/src/commands/` **más su registro en el `generate_handler!` de `src-tauri/src/lib.rs`**, y la función y tipos en el módulo de `ipc/` que corresponda al dominio.

Adentro de `ipc/` hay un módulo por dominio —`core` (el error y los ayudantes), `servers`, `schema`, `monitor`, `backup`, `query`, `data`, `ddl`, `objects`, `settings`, `security`— y un `index.ts` que los reexporta, así que quien importa sigue escribiendo `from "./ipc"`. Era un archivo de dos mil líneas: la regla no cambió, cambió que ahora se puede leer el pedazo que a uno le toca.

**Las lecturas largas se pueden abortar.** La pestaña de consulta tiene sesión propia con su `CancelToken`, pero el árbol y el DDL toman una conexión del pool y la devuelven, así que no había a quién cancelarle. `conn::cancelable(sink, future)` corre una lectura adentro de un *task-local* donde `ServerHandle::client` anota el token de cada conexión que entrega —sin cambiarle la firma a ninguna función del núcleo—; `tree_children` y `object_ddl` aceptan un `requestId` opcional, lo registran en `AppState::reads` mientras dura, y `read_cancel` cancela todos sus tokens. Del lado de la interfaz, el chevron de una fila que carga se convierte en cruz, y cambiar de nodo en el panel de detalle aborta el `pg_dump` que ya no se va a mirar. Cancelar deja el nodo cerrado y sin cargar, no vacío: es lo que hace que el próximo intento vuelva a leer.

`describeError` no solo arma el texto: **escribe el error en el registro**, porque es el único embudo por el que pasa todo error que el usuario llega a ver. El registro lo maneja `tauri-plugin-log` (`src-tauri/src/lib.rs`), con archivo en el directorio de registros del sistema, rotación por tamaño y los tres últimos guardados; la ruta viaja en `AppInfo::log_dir` y la interfaz la muestra en el `title` de la versión. Una cancelación no se anota: no es una falla.

Convenciones de serde que interfaz da por hechas: `#[serde(rename_all = "camelCase")]` en todo, y enums como uniones etiquetadas — `kind` para variantes de datos (`Change`, `TableChange`, `Outcome`, `NodeKind`) y `type` para eventos de canal (`QueryEvent`, `MonitorEvent`, `MaintenanceEvent`). Flujos largos (ejecución de consultas, sondeo del dashboard, mantenimiento) no devuelven resultado: mandan eventos por `Channel` de Tauri.

**Enum etiquetado necesita las dos:** `rename_all` renombra nombres de variante, `rename_all_fields` los campos de adentro de cada variante. Solo con la primera, un `new_name` o `type_name` sigue esperándose en `snake_case` mientras interfaz manda `newName`: nada falla al compilar y el `invoke` se cae recién en tiempo de ejecución, con «missing field» del lado de Rust. Por eso los enums de este proyecto llevan siempre `#[serde(tag = "…", rename_all = "camelCase", rename_all_fields = "camelCase")]`, aunque hoy no tengan campo de dos palabras. Lo que atrapa el olvido: `src-tauri/tests/preview.rs`, que arma la carga como JSON en vez de construir tipos de Rust.

Valores de celdas viajan como `string | null` (`null` = NULL de la base, distinto de cadena vacía), no como tipos nativos de JavaScript.

Lo que un perfil habilita o exige no se consulta al `explorer` desde cada diálogo, sino a `ui/src/lib/access.svelte.ts`: `isReadOnly`, `readOnlyReason` (motivo para el `title` de un botón apagado) y `confirmMutation`, que resuelve una promesa contra un único `Confirm.svelte` montado en `App.svelte` — así ningún diálogo de mutación tiene que anidar otro modal adentro del suyo. Todo `submit()` que modifica el servidor arranca con esa línea; en `DetailPanel` los botones que abren esos diálogos llevan `{...blocked}`, que agrega `disabled` **y** el motivo.

### Vista previa antes de aplicar

Toda mutación tiene dos comandos: uno que **genera el SQL** y otro que lo ejecuta — `ddl_preview` / `ddl_apply`, `data_preview` / `data_apply`, `data_export_preview` / `data_export_run`, `data_import_preview` / `data_import_run` (generan el `COPY … TO/FROM STDIN` exacto, ver `data::io`), `index_preview` / `index_create` (que lanza la tarea y devuelve su identificador, ver «Procesos en segundo plano»), `view_preview`, `trigger_preview`, `role_preview`, `privilege_preview`, `maintenance_plan` / `maintenance_run`, `backup_plan` / `backup_run` y `restore_plan` / `restore_run` (que en vez de SQL generan la línea de comando de `pg_dump` y `pg_restore`).

Import/export mueve datos en streaming por trozos (`copy_out`/`copy_in`), no acumula archivo en memoria, y reporta avance por `Channel`; import corre en una sola transacción para no dejar tabla a medias. Formato binario no es portable entre versiones distintas —interfaz lo advierte—.

Función generadora es **pura** a propósito: único verificable sin servidor, y garantiza que lo que interfaz muestra es exactamente lo que se va a ejecutar, no reconstrucción parecida. Al agregar operación que modifica servidor, mantener esa separación.

Reglas que sostienen edición de datos (`data::edit`): sin clave primaria/única, grilla se abre en solo lectura; cada `UPDATE` lleva valores originales de las columnas que cambia y, si afecta cero filas, reporta `Conflict` en vez de pisar; lote entero va en una transacción.

Orden y filtro de la pestaña de datos los resuelve el servidor, no la grilla: `PageView` (`data::page`) lleva la columna a ordenar —validada contra la forma de la tabla, porque llega de la interfaz y se interpola— y el predicado del `WHERE`, que va **crudo**, misma frontera de confianza que el editor. Con un orden elegido, el cursor por clave deja de valer —«lo que sigue de esta clave» no es lo que sigue en pantalla— y se pagina por `OFFSET`; la clave se agrega igual como desempate, o dos filas con el mismo valor podrían aparecer dos veces. Cambiar cualquiera de los dos vuelve a leer desde la primera tanda, así que la interfaz lo bloquea si hay ediciones sin guardar.

Los tipos de las columnas de una consulta se piden aparte (`QuerySession::column_types`, comando `query_column_types`) y solo con el interruptor «Tipos» encendido: el protocolo simple con el que se ejecuta no los trae, y saberlos cuesta **preparar** la sentencia de nuevo —otra vuelta al servidor y otra planificación—. Vale para una sola sentencia; un script lo rechaza el servidor y el encabezado se queda sin tipos, sin molestar con un error.

`ExportDialog` recibe un `ExportSource`, no una tabla: la pestaña de consulta exporta su resultado con `{ kind: "query" }` y el núcleo arma el `COPY (…) TO STDOUT`. Exportar **no** manda las filas de la pantalla: vuelve al servidor, así que el archivo trae todas y no las que entraron en el techo.

### Procesos en segundo plano

Lo que tarda no vive adentro de su diálogo. Mantenimiento (`VACUUM`/`ANALYZE`/`REINDEX`), creación de índices, backup, restore, importación y exportación se **lanzan y el diálogo se cierra**: siguen corriendo mientras se usa el resto de la aplicación, y se ven en la cuarta vista de la barra, «Procesos» (`ProcessPanel.svelte`). Antes cada uno tenía la ventana tomada hasta terminar, así que un `CREATE INDEX CONCURRENTLY` sobre una tabla grande era una tarde sin poder mirar otra cosa que no fuera cancelarlo.

Del lado de Rust, una sentencia larga es siempre lo mismo —una sesión dedicada, sin `statement_timeout`, con su `CancelToken` anotado en `AppState::tasks`—, así que `commands::tasks::spawn_statement` la corre para las dos que la usan (`maintenance_run` e `index_create`) y `task_cancel` le pide al **servidor** que aborte, nunca mata la tarea local: abortarla acá dejaría al servidor terminando el `VACUUM` sin nadie escuchando. `Session::execute_batch` es lo que ejecuta: protocolo simple y sin transacción envolvente, que es justo lo que `CONCURRENTLY` necesita. Backup, restore y las copias de datos ya tenían su propio registro (`backups`, `restores`, `copies`) porque ahí el trabajo lo hace un proceso hijo y no el servidor; lo que cambió es quién los escucha.

Del lado de la interfaz, el dueño del canal es `tasks.svelte.ts` y no el componente: un `Channel` de Tauri sigue recibiendo eventos aunque el diálogo ya no exista, pero sin un lugar donde anotarlos se perderían. Cada `TaskRun` guarda qué corre, contra qué servidor, su registro de avance y con qué terminó; `onDone` es lo que hace que la lista de índices se relea cuando el índice **existe**, y no cuando se apretó el botón. `task-format.ts` —el rótulo, lo que lleva corriendo, con qué terminó— es puro y va con Vitest. Cerrar la ventana de un proceso no lo toca: cancelar es explícito.

### Objetos con molde propio

Casi todo módulo de `ddl/` sigue el mismo molde de `ddl::view`: enum `XChange` etiquetado, función pura `statements()` y `apply()` que la ejecuta en **una** transacción. Tres se salen, y no por gusto:

- **`ddl::database`** no abre transacción: PostgreSQL rechaza `CREATE DATABASE` y `DROP DATABASE` adentro de un bloque transaccional. Consecuencia: una lista a medias deja hecho lo anterior, así que la interfaz manda un cambio por vez. Además no se puede borrar ni renombrar la base a la que uno está conectado — de qué base colgarse lo decide `working_database`, no quien llama.
- **`ddl::partition`** manda las sentencias sueltas solo si hay un `DETACH … CONCURRENTLY`, igual que `ddl::index` con `CREATE INDEX CONCURRENTLY`. Ese modificador existe desde PG 14 y lo gatea `ServerCaps::has_detach_partition_concurrently`; por eso `statements()` recibe las capacidades y `partition_preview` pide el perfil, a diferencia del resto de las vistas previas (mismo caso que `maintenance_plan`).
- **`ddl::comment`** es transversal y no un cambio repetido en cada objeto: la sentencia es siempre `COMMENT ON <clase> <nombre> IS <texto>` y lo único que cambia es cómo se escribe el nombre. Sin texto borra con `IS NULL`, porque una cadena vacía deja un objeto documentado con nada adentro.

Dos cosas que **no** hacen falta gatear, aunque parezca: `ALTER TYPE … ADD VALUE` corre adentro de una transacción en todo el rango soportado (la prohibición se levantó en PG 12, el piso es la 13), y `DROP DATABASE … WITH (FORCE)` existe desde la 13.

Lo que PostgreSQL no puede hacer y la interfaz tiene que explicar en vez de intentar: no hay `DROP VALUE` de una enumeración —sacar uno exigiría recrear el tipo y todas las columnas que lo usan—, así que `TypeDialog` avisa que el valor se conserva. El diff de tipos y compuestos vive en `type-form.ts`, puro y probado.

### Identificadores y SQL crudo

`ddl::quote_ident` cita solo cuando hace falta (mayúsculas, símbolos, palabras reservadas). Usarlo para todo identificador que se interpole. En cambio, hay texto que va **crudo** a propósito y está documentado como tal: `DEFAULT` de columna, `USING` de cambio de tipo, predicado de índice parcial, expresión de `CHECK`, nombre de tipo de columna. No se pueden parametrizar en DDL, los valida el servidor al ejecutar, y es la misma frontera de confianza que el editor de consultas: lo ejecuta el propio usuario con sus propios privilegios.

Para DDL de lectura se prefiere siempre función del servidor (`pg_get_viewdef`, `pg_get_indexdef`, `pg_get_functiondef`, `pg_get_constraintdef`, `pg_get_triggerdef`); tablas, sin equivalente, se delegan a `pg_dump`. Campo `DdlSource` dice de cuál de los dos salió, porque no es lo mismo lo que afirma el servidor que lo que reconstruyó herramienta externa.

Binarios externos (`pg_dump`, `pg_restore`) se ubican en un solo lugar, `backup::tools`: variable de entorno por herramienta → `PATH` → rutas típicas de cada sistema, quedándose con la versión más alta. Antes de usarlos hay que comparar su versión con la del servidor —pueden leer servidores más viejos que ellos, nunca más nuevos— y conviene detectarlo antes de empezar, no a los diez minutos. Todo `Command` que los lance pasa por `backup::tools::hidden`, que en Windows agrega `CREATE_NO_WINDOW`: sin eso, un ejecutable de consola lanzado desde una aplicación gráfica se trae su ventana negra, y con `pg_dump` eso pasaba con solo mirar el detalle de una tabla.

### Interfaz

Svelte 5 con runes. Estado compartido = clases con campos `$state` exportadas como instancia única desde archivos `*.svelte.ts` (`explorer`, `tabs`, `theme`, y clases `QueryTab`/`DataTab`/`ErdTab` que extienden `Tab`). No hay stores de Svelte 4 ni librería de estado.

Pestañas de consulta, de datos y de diagrama conviven en una sola barra (`tabs.svelte.ts`); cerrar una llama a su `dispose()`, donde se suelta la sesión del lado de Rust —el diagrama no toma ninguna, así que hereda el `dispose()` vacío—. Tema resuelto se escribe en `document.documentElement.dataset.theme` para que Tailwind, CodeMirror y uPlot lean un solo atributo.

Editor SQL: CodeMirror 6 (`@codemirror/lang-sql`). Gráficos: uPlot.

Dos cosas del editor que no son configuración sino código propio. **El autocompletado de columnas sin calificar**: `lang-sql` solo las ofrece detrás de `tabla.` o de un alias, así que escrito el `FROM` un `SELECT ` no completaba nada; `ui/src/lib/sql-complete.ts` lee las tablas del `FROM`/`JOIN` de la sentencia del cursor y ofrece sus columnas, y se engancha con `PostgreSQL.language.data.of({ autocomplete })` **sumada** a la del dialecto, no en su lugar. Es puro y con Vitest, y saca las tablas leyendo el texto y no el árbol de sintaxis, que tiene nodos de error justo donde uno está escribiendo. **El keymap va en `Prec.highest`**, así que cada atajo que agregue tiene que devolver `false` cuando lo que corresponde es el manejador de abajo: el `Escape` que cancela la consulta se comía el que cierra el autocompletado y el panel de búsqueda hasta que empezó a consultar `completionStatus` y `searchPanelOpen`.

El **SQL de adentro de un `$$ … $$`** se colorea aparte (`sql-nested.ts`). Para `lang-sql` el cuerpo de una función es una cadena y nada más, así que cien líneas de plpgsql salían todas del mismo verde; un `ViewPlugin` ubica esos cuerpos con un escaneo textual —el árbol de sintaxis tiene nodos de error justo donde uno está escribiendo, igual que en `sql-complete`—, los reparsea con el parser del dialecto y los pinta con el mismo mapeo de colores que el resto (`sql-highlight.ts`, tabla única). Dos cosas que parecen detalles y son la diferencia entre que se vea o no: el plugin va en **`Prec.highest`**, porque con marcas superpuestas CodeMirror dibuja adentro las de mayor precedencia y el color lo decide el `span` de más adentro —con la precedencia por omisión, el de la cadena quedaba último y ganaba siempre—; y el color va como **estilo en línea y no como clase**, porque entre dos clases de igual especificidad gana la que el navegador leyó última, que no es algo que uno elija. La decoración de todo el cuerpo va primero (`--cm-text`), o lo que ningún token pinte seguiría heredando el color de las cadenas. Vale igual para `$$` y para `$function$`, la etiqueta que escribe `pg_get_functiondef`. `dollarBlocks` es puro y con Vitest; lo usan el editor y `Sql.svelte`, o sea también el DDL y toda vista previa.

Cada pestaña muestra **el nombre del servidor** además del suyo: con varias abiertas, «Consulta 1» contra desarrollo y «Consulta 1» contra producción eran la misma pestaña a la vista. Sale del perfil y no de un campo de `Tab`, porque el nombre se puede cambiar sin cerrar nada.

Contra qué base abre una consulta o una grilla cada fila del árbol lo decide `ui/src/lib/tree-actions.ts`, puro y con Vitest. Hay tres puertas a lo mismo —el botón de `DetailPanel`, el menú del clic derecho de `TreePanel` y `Ctrl+Q`—, y la regla tiene que ser una sola. Una pestaña de consulta además puede cambiar de base sin cerrarse (`QueryTab.switchDatabase`): cierra la sesión y abre otra, así que se pregunta antes si hay una transacción abierta.

El tamaño de letra del SQL es una sola preferencia para toda la aplicación (`editor.svelte.ts`), guardada como el tema y aplicada como variable `--sql-font-size` en la raíz: la leen el editor de consultas y `Sql.svelte`, o sea también el DDL y toda vista previa. Se cambia con `Ctrl +`/`Ctrl -`/`Ctrl 0`, `Ctrl` + rueda o los botones de `FontSize.svelte`. Al tocarla hay que llamar a `requestMeasure()` de cada `EditorView`: CodeMirror mide el ancho de un carácter una vez y con la letra nueva el cursor queda corrido.

Cuántas filas se traen de una es una preferencia del usuario (`paging.svelte.ts`, 200 por omisión, guardada) y no una constante del núcleo: la pestaña de datos la usa como tamaño de tanda —el scroll pide la siguiente hasta `AUTO_LIMIT`, y de ahí en más hay que apretar «Cargar N más»— y la pestaña de consulta como `maxRows` del `query_run`, donde no hay scroll que traiga nada y para ver más se sube el número y se vuelve a ejecutar.

La grilla virtualiza en las dos direcciones: filas por división de la altura fija y columnas por `columnRange` (`grid-window.ts`, puro y probado). Las filas dibujadas van en un bloque que se corre con `transform`, no con un `top` por fila, y el desplazamiento se lee una vez por cuadro con `requestAnimationFrame`. Las tres cosas son lo mismo: el compositor desplaza antes de que el hilo principal dibuje, y cada celda de más se paga en ese retraso.

La grilla (`DataGrid.svelte`) tiene foco de celda y rango rectangular: flechas con `Shift`, `Ctrl+C` (TSV), `Ctrl+Shift+C` (CSV con encabezados), clic derecho para el menú, `Espacio` para el visor de la celda y `Ctrl+F` para el filtro sobre lo ya cargado. El clic derecho en el encabezado fija columnas contra el borde izquierdo, las esconde o ajusta el ancho; todo eso pasa por `active` —las columnas visibles—, que es sobre la que cuentan el foco, el rango y la ventana horizontal, así que esconder una columna corre los índices en vez de dejar un hueco. Las fijas son `sticky` con `bg-inherit`, y por eso el fondo de la fila es opaco: con un fondo a medias se les vería por debajo lo que pasa de largo. Lo que se copia sale de `Column.raw` —el valor como vino, con `null` para NULL—, no del texto de pantalla, que va recortado a una línea; el formato lo arma `grid-copy.ts`, que es puro y está probado con Vitest.

**Diagrama ERD**: `introspect::graph` devuelve tablas y aristas, jamás coordenadas — posición depende del ancho del texto en pantalla y de lo que el usuario arrastre, así que layout vive en `ui/src/lib/erd.ts`, puro y con Vitest (rangos por capas, ciclos de FK que no pueden colgar la interfaz, tope de columnas por caja). `ErdPanel.svelte` solo dibuja el SVG y maneja zoom/pan/arrastre. Excepción anotada a la regla de que lógica vive en core: `erd_export_svg` escribe el archivo desde `src-tauri` porque SVG lo arma la interfaz y sumar el plugin de archivos por un caso costaba más que cinco líneas de `std::fs`. `sql_write_file` —guardar una pestaña de consulta como `.sql`— es la misma excepción y por la misma razón; no agregar una tercera sin ese mismo argumento.

#### Estilos

`ui/src/app.css` no es solo el import de Tailwind: tiene **capa de componentes** con las piezas repetidas en toda la aplicación — `.card`, `.panel`, `.toolbar`, `.btn` (`btn-primary`, `btn-danger`, `btn-ghost`, `btn-icon`, `btn-sm`), `.field`, `.label`, `.check`, `.seg`, `.tag-*`, `.alert-*`, `.list-table`, `.row-actions`, `.spinner`, `.muted`, `.divider-*`. Antes de escribir cadena de clases Tailwind para botón, campo o tarjeta, usar la clase que ya existe; si hace falta variante nueva, va ahí, no repetida en cada componente.

Dos consecuencias de ese archivo que sorprenden si no se leyó: `<body>` tiene `user-select: none` porque la ventana es aplicación, no página —lo que se debe poder copiar (SQL, DDL, valores) pide `select-text` explícitamente—; y CodeMirror y uPlot no reciben clases sino variables CSS (`--cm-*`, `--plot-*`) declaradas por tema, para que color lo siga decidiendo un solo lugar.

Íconos = SVG dibujados a mano en `Icon.svelte` (librería completa pesa más que toda la interfaz); ícono y color de cada tipo de nodo salen de `lookOf` en `badges.ts`, que agrupa por familia. Tipo de objeto nuevo se agrega en esos dos lugares, no en el componente que lo muestra.

Rasgos que árbol muestra como pastillas de color (rol puede iniciar sesión, tabla tiene RLS activa) son el `NodeTag` del núcleo, vocabulario cerrado: `TreeNode` los trae en `tags` y `tagLook` en `badges.ts` les pone texto y tono. Van separados de `detail`, texto libre para leer. Rasgo nuevo se agrega en esos dos lugares.

El árbol tiene **tres registros y no uno con quince variantes**, que es lo que lo volvía una mancha: una **sección** —carpeta de conexiones o carpeta del catálogo, las dos con `isSection`— es un rótulo en versalita, sin ícono, con el contador a la derecha; un **servidor** es una ficha de dos líneas; el resto, una fila densa de una sola. Sacarle el ícono a las carpetas es a propósito: la carpeta gris del catálogo y la ámbar de conexiones eran dos formas iguales para dos cosas distintas, y ese ámbar chocaba con el de las secuencias y con el del resaltado de la búsqueda. Agrupar se nota por la tipografía. Los servidores se quedan en el nivel de su carpeta: la sección ya agrupa a la vista, y sangrar además cobra dos veces el ancho que les falta a los nombres del final. Lo que sí sangra es carpeta dentro de carpeta, que es de lo que se trata anidarlas.

En la ficha del servidor, nombre solo arriba —con la pastilla de producción— y abajo `user@host:port` más el candado de solo lectura, sin la palabra. Nada se dice dos veces: el entorno lo pinta la línea del borde izquierdo (`spine` de `envLook`), que baja por el servidor y todo lo que cuelga de él con el mismo ancho, y producción conserva además la pastilla porque el color solo no se le puede confiar. El punto de conexión va **encima** del ícono y no en columna propia, que corría los íconos de las raíces respecto de todo el resto. «Conectar» es un ícono que sale al pasar por encima, no una palabra fija.

Las alturas no son uniformes, así que la ventana visible no sale de una división sino de desplazamientos acumulados (`offsets`) más bisección (`indexAt`); al tocar el alto de una fila hay que pasar por `heightOf`, no por el `style`. El rótulo de la sección de la que cuelga lo que se está viendo queda **anclado arriba** (`tree-sticky.ts`, puro y con Vitest): se dibuja a la altura del desplazamiento y no con `position: sticky`, porque las filas son absolutas y ahí no hay flujo al que pegarse; se ancla uno solo —apilar la cadena entera se come el alto de la ventana, que es justo lo que hace falta para ver filas—. Las guías de sangría se dibujan **solo para la cadena de la fila elegida** (`tree-guides.ts`, puro y con Vitest): una línea por nivel en cada fila son cinco rayas fijas de ruido por una pregunta ocasional. El `detail` de un nodo va en columna a la derecha, alineado, no pegado al nombre. `visibleRows` acepta el filtro «solo conectados», que esconde filas sin descartar lo que tienen cargado.

**Qué recuerda el árbol entre sesiones** vive en `folders.svelte.ts` (`localStorage`, como el tema o el tamaño de tanda): cuáles carpetas quedaron cerradas y cuáles quedaron vacías. Lo segundo no puede vivir en `connections.json` —una carpeta es un nombre compartido entre perfiles, no una entidad guardada—, así que una carpeta vacía no tiene dónde persistir del lado de Rust sin inventarle una lista aparte. Lo que **no** se restaura es hasta dónde estaba expandido un servidor: al arrancar no hay ninguna conexión abierta, y restaurar ese camino sería reconectar y disparar una consulta por nivel sin que nadie lo pidiera. Los servidores sin carpeta salen bajo su propio rótulo «Sin carpeta» —una fila de carpeta sin nombre, que por eso no se puede renombrar— pero solo si hay al menos una carpeta de verdad: es el destino visible para sacar un servidor de la suya.

**Selección múltiple, solo de servidores** (`explorer.marked`): `Ctrl`+clic marca de a uno, `Shift`+clic marca el rango, `Escape` limpia. Solo servidores porque lo que se hace con varios a la vez —moverlos a una carpeta arrastrándolos, desconectarlos desde el menú— es de la conexión y no del catálogo; veinte tablas marcadas no habilitan nada. Conectar no entra al lote: cada servidor puede pedir contraseña y serían diez diálogos encadenados.

Teclado del árbol, además de las flechas: escribir letras salta a la fila que empieza así (`*` abre todos los hermanos, `F2` renombra la carpeta o abre el perfil del servidor). El manejador vive en el contenedor y no en cada fila, porque con la ventana deslizante las filas de arriba y abajo no están en el DOM.

**Buscar es dos cosas distintas.** La caja filtra lo que el árbol ya trajo —que es justo lo que uno ya había abierto—; **Enter** o el botón de brújula preguntan al servidor (`introspect::search` → comando `tree_search`), que es lo único que alcanza los esquemas que nunca se abrieron. La consulta coincide con `strpos(lower(…), lower($2))` y no con `ILIKE '%…%'`: sin escapar, el `_` de cualquier nombre de tabla sería comodín. Busca relaciones, rutinas y tipos —`pg_attribute` tiene un orden de magnitud más filas y doscientas columnas `id` no ayudan—. El resultado **reemplaza** al árbol mientras dura en vez de mezclarse con las filas: lo encontrado puede vivir donde el árbol no tiene rama todavía. Las dos aceptan **prefijo de tipo** (`tree-query.ts`, puro y con Vitest): `t:factura` acota a tablas, y el prefijo solo (`t:`) lista la familia entera. Vocabulario cerrado y de una letra; lo que no está en él es texto, así que un nombre con dos puntos adentro se sigue buscando entero. Al servidor se le manda el texto sin el prefijo y el resultado se acota del lado de la interfaz: mantener la misma tabla de prefijos en los dos lados no compra nada. Elegir una coincidencia la revela: como el `SearchHit` trae el tipo, `folderForKind` baja derecho al cajón que le toca, mientras que revelar desde una grilla —que solo tiene el OID— prueba los cuatro cajones de relaciones y vuelve a cerrar los que no eran.

**El árbol se pone al día solo después de un DDL corrido desde el editor.** `changesCatalog` (`ddl-tags.ts`) mira la primera palabra de la etiqueta que devuelve el servidor —no el SQL escrito, que obliga a pelearse con comentarios y `DO $$`— y `Explorer.refreshOpen` relee lo que esté abierto **conservando la expansión**, al revés de `reload`, que descarta. Con una transacción abierta se espera al `COMMIT`: el árbol lee por otra conexión y ahí el objeto todavía no existe. El mismo aviso invalida el caché de nombres del autocompletado.

#### Diálogos

`Modal.svelte` es el diálogo de toda la aplicación y ya resuelve Escape, trampa de foco y devolución del foco al cerrar; `busy` impide que se cierre a mitad de un guardado, `data-autofocus` elige qué campo recibe foco. Lo que **no** hace es cerrarse al hacer clic en el fondo: son formularios de varios campos con vista previa del SQL, y un clic al costado tirando lo escrito es peor que un clic de más en «Cancelar». Se cierra con sus botones o con Escape, que es intención explícita. Ningún diálogo reimplementa eso por su cuenta. Al lado: `Confirm.svelte` (acciones que no se deshacen), `Alert.svelte` (`tone` + `box`), `SqlPreview.svelte` y `Empty.svelte`.

Formularios de mutación (`PolicyDialog`, `RoleDialog`, `TableDialog`, …) siguen todos la misma forma, cara visible de la regla de vista previa: copia editable de props tomada una sola vez con `untrack`, función `changes()` que arma el cambio, `validate()` que devuelve el problema o `null`, botón «Ver SQL» que llama al `*_preview`, y `submit()` que llama al `*_apply`. Al agregar objeto nuevo, copiar esa estructura en vez de inventar otra.

`changes()` y `validate()` son funciones puras de lo escrito en pantalla; montar el componente para probarlas no aporta nada. Donde lógica es de verdad —diff contra lo que hay en servidor, campo que solo vale para algunos casos, orden en que se mandan las sentencias— se saca a `<objeto>-form.ts` al lado del diálogo (`role-form.ts`, `policy-form.ts`, `type-form.ts`, `column-form.ts`, `table-form.ts`, `index-form.ts`, `trigger-form.ts`): componente queda con `$state` y bindings, archivo suelto se prueba con Vitest. Diálogos que solo juntan campos y los mandan no necesitan esa separación.

Dos que valen como ejemplo de qué se rompe callado: en `column-form.ts`, editar una columna no manda «cómo quedó» sino un cambio por cada cosa tocada, y el renombre va **primero** porque los que siguen corren después en la misma transacción y tienen que hablar del nombre nuevo; en `trigger-form.ts`, editar es `DROP` + `CREATE` —PostgreSQL no altera lo que define a un disparador— y el `DROP` nombra al trigger **como estaba**, no como quedó.

## Nota sobre `plan-proyecto-pgtool-rust.md`

Documento de visión original; su tabla de stack está **desactualizada**: proyecto usa `tokio-postgres` + `deadpool` (no `sqlx`), CodeMirror (no Monaco), Svelte 5 con grillas propias (no TanStack Table) e instancias locales de PostgreSQL vía `PGFORGE_TEST_URLS` (no `testcontainers`). Sirve para alcance por fases; para stack mandan `Cargo.toml` y `ui/package.json`.