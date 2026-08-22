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

**Importar servidores** (`conn::import`): lo que ya está configurado en otra herramienta —`~/.pgpass`, `~/.pg_service.conf` y el `data-sources.json` de DBeaver— se lee y se ofrece para agregar. De DBeaver vienen además la carpeta y el tipo de conexión, que son las dos cosas que uno no quiere volver a configurar en veinte servidores. **Nunca se trae una contraseña**, ni siquiera las que `.pgpass` tiene en texto plano: copiarlas convertiría un archivo que el usuario decidió tener así en otra copia que no decidió. Los parsers son puros y con tests —los `\:` escapados, los comodines, el `provider` que no es PostgreSQL—; lo único que toca el disco es `scan`.

Servidores se agrupan en **carpetas de conexiones**: campo `group` del perfil. No hay lista de carpetas guardada aparte —carpeta = nombre que comparten unos perfiles—, así que `ProfileStore::groups()` las deriva, `rename_group` mueve a todos sus miembros en una sola escritura, y carpeta desaparece sola cuando sale el último servidor. Nombre pasa siempre por `normalize_group` al guardar, porque es también la clave por la que se agrupa. **Carpetas anidan con `/`**: `Clientes/ACME` no es entidad aparte de `Clientes`, es nombre con un tramo más —por eso `normalize_group` normaliza tramo por tramo, `groups()` agrega las intermedias aunque no tengan servidores propios, y `rename_group` arrastra a las de adentro (deshacer sube un escalón en vez de hacerlas desaparecer)—.

### Aviso de versión nueva

`pgforge-core::update` **no** es el actualizador firmado de Tauri: no descarga ni instala nada. Le pregunta a la API de releases de GitHub —repositorio sacado de `CARGO_PKG_REPOSITORY`, no de una constante a mano— cuál es la última publicada y, si supera a la que corre, la interfaz ofrece abrir su página. Elegir esto en vez de `tauri-plugin-updater` evita par de claves de firma, secreto de CI y `latest.json`, y sobre todo no pide confiar en un binario que se reemplaza solo; el día que haya release de verdad contra la cual probarlo, el plugin firmado se enchufa detrás del mismo cartel.

Qué release cuenta como más nueva es **función pura** (`newer_than`, con `Version` propia: comparar tres números y saber que `-rc.1` va antes que la final no justifica traer `semver`), así que se prueba sin red; lo único que necesita internet es traer la lista. Se descartan borradores y prelanzamientos. El HTTP es `reqwest` con `rustls-no-provider` —la variante `rustls` prende `aws-lc-rs`, que pide NASM y cmake en Windows, lo mismo que se evita en `russh`— y el TLS se arma en `conn::tls::web_config`, para que la elección de proveedor criptográfico y de raíces siga siendo una sola en todo el proyecto.

Cada cuánto se pregunta y qué versión se descartó viven en la interfaz (`update.svelte.ts`, `localStorage`): una vez por día, porque la API sin autenticar permite 60 pedidos por hora y por IP, y «Ahora no» silencia esa versión y no el aviso entero. Que la comprobación falle **no se muestra**: un cartel rojo porque no había internet al abrir es peor que no enterarse. `update_open` exige que la dirección empiece por la de las releases del repositorio, así que una respuesta rara de la API no puede abrir cualquier cosa. Desde la línea de comandos, `pgforge update`.

### Historial y consultas guardadas

El historial dejó de ser «lo que escribí en el editor» para ser **lo que la aplicación ejecutó contra el servidor**: cada `*_apply` de un diálogo anota su SQL con `source = dialog` (`commands::record_applied`), y la lista lo marca. Es la pregunta del día siguiente —qué cambió, cuándo y contra qué servidor—, que antes no tenía dónde contestarse. Se anota el SQL que se ejecutó, el mismo que muestra la vista previa, y lo que falle al anotar **no** toca el resultado: la operación ya corrió contra el servidor.

Las dos cosas viven en SQLite adentro del directorio de configuración, pero en **archivos distintos** (`history.db` y `saved.db`), porque el `user_version` del esquema es del archivo y porque no son lo mismo: el historial (`sql::history`) es lo que pasó —se anota solo y se vacía entero sin que duela—, y `sql::saved` es lo que el usuario decidió conservar, con nombre. De ahí que ahí el nombre sea obligatorio y único sin distinguir mayúsculas, y que guardar con un nombre ocupado devuelva `Conflict` en vez de pisar: perder en silencio lo único que se pidió conservar es el peor final posible. Reescribir se pide con el `id`, y entonces la fecha de creación no se toca.

Contra qué servidor y qué base se escribió se guarda como dato, no como atadura: correr el mismo `SELECT` contra desarrollo y contra producción es justo lo que se hace, así que ni se exige que el perfil siga existiendo ni se filtra por él. El historial busca en el servidor —crece sin techo— y las guardadas se filtran en la interfaz: son decenas, puestas a mano de a una. La pestaña recuerda de cuál salió (`QueryTab.savedId`), que es lo que hace que volver a guardar reescriba esa y no deje una copia; es independiente de `filePath`, que es un archivo del disco y otra cosa.

### Comparar esquemas

`pgforge-core::compare` responde qué tiene el esquema de un servidor que no tiene el de otro, y con qué SQL se los iguala. Tres piezas: `snapshot` es lo único que habla con el servidor, `diff` compara dos instantáneas y `sync` arma el script. Las dos últimas son **puras**, que es donde vive el riesgo: un `ALTER` mal armado se ve leyendo el texto, no conectando. `render` escribe el texto de cada objeto y lo usan las dos —lo que la interfaz muestra lado a lado es exactamente lo que el script ejecutaría, no una reconstrucción parecida—.

**No hay `compare_apply`**, y es la regla de vista previa llevada hasta el final: la comparación termina en un informe y un script que se copia o se abre en una pestaña de consulta *contra el destino*. Cada sentencia viaja con su `Risk` (`safe` / `review` / `destructive`) y el orden de salida es el orden en que se puede correr —tipos, secuencias, tablas, restricciones, índices, vistas, y lo destructivo al final, para poder cortar el script ahí—. Lo que PostgreSQL no puede hacer con un `ALTER` no se inventa: va a `SyncPlan::warnings` con el motivo (el tipo base de un dominio, el orden de una enumeración, una columna generada).

Se comparan tablas con sus columnas, restricciones e índices; vistas y materializadas; secuencias; y tipos —enumeraciones, compuestos, dominios y rangos—. Quedan afuera funciones, disparadores, políticas y permisos: el cuerpo de una función difiere por un espacio y el ruido tapa lo que importa. También quedan afuera las particiones (cuelgan de su madre: una diferencia aparecería repetida una vez por partición), las secuencias de una columna `serial`/`identity` (son parte de esa columna) y los datos —`last_value` es estado, no estructura—.

Dos normalizaciones evitan diferencias que no lo son. `retarget` reescribe las referencias al esquema origen con el nombre del destino, que es lo mismo que hace falta para poder ejecutar el script del otro lado; sin eso, comparar `dev.pedidos` con `prod.pedidos` marcaría cada índice como distinto. Y el cuerpo de una vista se compara **también sin las calificaciones de columna, pero solo entre versiones mayores distintas**: PG 16 dejó de escribir `clientes.id` y pasó a escribir `id` en `pg_get_viewdef`, así que sin esa segunda pasada toda vista aparecería cambiada justo al comparar antes de una actualización. Entre servidores de la misma versión la comparación queda exacta, porque ahí una calificación de más la escribió alguien. Para no confundir `esquema.tabla` con `tabla.columna` sin analizar SQL, la instantánea trae los nombres de todos los esquemas de su base.

De la interfaz: la pestaña es `CompareTab` (`compare.svelte.ts`), se abre desde el clic derecho de un esquema o el botón del panel de detalle, y `CompareDialog` elige el otro lado entre los servidores **conectados** —leer en vivo de los dos lados es la premisa, y conectar desde ahí pediría contraseña fuera del único lugar donde se piden—. El filtro por riesgo y el armado del texto son de la interfaz (`compare-script.ts`, puro y con Vitest) porque destildar «lo destructivo» tiene que rearmar el script en el acto; el equivalente para la línea de comandos es `compare::sync::script`. Desde la CLI: `pgforge compare --url … --target-url … --schema public [--sql]`.

### Plan de ejecución

El plan se pide **una sola vez y en JSON** (`sql::explain`), y de ahí sale todo lo demás. Eso manda dos decisiones: el `Plan` conserva el `json` crudo del servidor —para pegarlo en un visor de planes hace falta entero, con los campos que la aplicación no dibuja, y reconstruirlo desde el árbol sería inventar la mitad—, y el texto que copia el botón «Copiar texto» lo arma la interfaz (`plan-text.ts`, puro y con Vitest) con la forma de `psql`, en vez de volver a pedirle el plan al servidor en formato texto: con `ANALYZE`, pedirlo de nuevo es ejecutar la consulta otra vez.

`sql::advice` lee ese árbol y dice qué mirar: un `Seq Scan` que descartó casi todo lo que leyó (propone el `CREATE INDEX`), un filtro que el índice no resolvió, una estimación que se fue por diez (propone el `ANALYZE`) y un `Sort` que terminó en disco (propone subir `work_mem` **en la sesión**, que es donde se reserva). Es puro y se prueba con JSON de ejemplo, más un test contra servidores reales (`tests/query.rs`) que verifica la cadena entera: que el filtro llegue como PostgreSQL lo reescribe y que, creado el índice, la sugerencia desaparezca.

La sugerencia de índice viaja además **desarmada** (`Advice::index`: esquema, tabla y columnas), y no solo como texto: con eso el botón «Crear el índice…» abre `IndexDialog` con todo puesto y de ahí sigue el camino de siempre —vista previa, confirmación de producción, tarea en segundo plano—. Copiar y pegar el mismo texto en otra ventana era el paso que sobraba. La tabla se busca por nombre (`data::shape_by_name` → `data_shape_named`), porque una sugerencia dice «esquema.tabla» y nunca un oid.

Tres reglas que lo sostienen. **No aplica nada**: la sugerencia termina en una sentencia para copiar, igual que la comparación de esquemas, porque un índice de más se paga en cada `INSERT` para siempre. **Umbrales altos y pocas reglas**: una lista de veinte avisos tibios se ignora entera. Y el `EXPLAIN` va siempre con `VERBOSE`, que no es una preferencia: es lo único que hace que el plan traiga el **esquema** de cada relación, y sin él un `CREATE INDEX ON trabajos` copiado a otra sesión puede caer sobre otra tabla que se llame igual. La columna que se indexa sale de leer el texto del filtro (`(estado)::text = 'activo'::text`) sin analizar SQL: lo que está adentro de una función se saltea —eso pide un índice por expresión— y la igualdad va antes que el rango, porque en un índice compuesto la primera desigualdad corta el uso de lo que sigue.

### Índices que sobran

`monitor::stats::redundancies` responde la otra mitad de la pregunta: no qué índice falta, sino cuál está de más. Un índice al que otro cubre —las mismas columnas, o las suyas como principio de las del otro— ocupa disco y hace más lenta cada escritura sin acelerar ninguna lectura. Es la única sugerencia de la aplicación que **ahorra** trabajo en vez de costarlo, y por eso no necesita que nadie explique una consulta antes.

Leer el catálogo y decidir están separados a propósito: `index_shapes` trae la forma de cada índice y `redundancies` es **pura**, porque equivocarse acá significa proponer que se borre un índice que hace falta. Con columnas idénticas se conserva uno solo y el desempate está escrito (lo protegido, restricción, `INCLUDE`, uso, nombre): decidirlo dos veces —una por cada orden del par— dejaría la tabla sin ninguno.

**Lo que el catálogo escribe igual y no lo es.** `pg_get_indexdef(indexrelid, n, true)` devuelve la columna pelada —que es lo que hace que un índice por expresión se lea igual que uno por columna—, pero **no** la clase de operadores, la colación ni el orden: `(a)`, `(a DESC)`, `(a NULLS FIRST)`, `(a text_pattern_ops)` y `(a COLLATE "C")` salen los cinco como `a`. Esos tres se leen aparte, de `indclass`, `indcollation` e `indoption`, que se castean directo a `oid[]`/`smallint[]`. Los `INCLUDE` se comparan por nombre y no por si los hay: dos índices que arrastran columnas distintas no se cubren. Y del orden vale también el **inverso exacto** —el btree se recorre para atrás—, pero solo entero: `(a DESC)` sobra frente a `(a)`, y `(a NULLS FIRST)` no, porque un `ORDER BY a NULLS FIRST` no lo resuelve ningún otro.

**Lo que sostiene algo no sobra, aunque esté duplicado.** Es un `Guards` (`constraint`, `referenced_by_fk`, `replica_identity`, `clustered`, `from_extension`) que se lee con el mismo SQL —`GUARD_COLUMNS`— para las dos listas de índices que sobran, la de los que nunca se usaron (`IndexStat::is_unused`) y la de los que otro cubre: con una idea propia de qué está protegido en cada una, una proponía borrar justo lo que la otra conservaba. Los cuatro primeros los rechaza el servidor con un error claro; la identidad de réplica **no**, y ahí está el motivo de que la guarda exista: el `DROP INDEX` funciona, la tabla queda con `relreplident = 'i'` apuntando a nada y la replicación lógica se rompe en el próximo `UPDATE`, lejos del botón que lo causó. Un índice de `EXCLUDE` no es único ni primario, así que sin mirar `pg_constraint` pasaba de largo. Y como lo protegido gana el desempate, la copia inútil que está a su lado se sigue proponiendo en vez de quedarse para siempre.

**Las particiones no entran.** El índice de una partición cuelga del de su madre: el mismo par duplicado aparecería una vez por partición y el servidor rechaza borrarlo por separado. Entra la madre, con el tamaño de todo su árbol (`pg_partition_tree`, porque `pg_relation_size` de un índice particionado da 0) y con un `DROP INDEX` **sin** `CONCURRENTLY`, que sobre un índice particionado también se rechaza. Descartarlas es además lo que achica el `n`: la comparación agrupa primero por esquema, tabla, método y predicado, así que el par solo se busca adentro de su grupo.

El test contra servidores reales (`tests/monitor_indices.rs`) arma el caso completo —los duplicados, el prefijo, el parcial, el gin, el único, las cinco escrituras iguales, cada guarda y la partitionada— y verifica en las cinco versiones que solo aparezcan los que sobran. Lo que la interfaz agrega: la lista se identifica por esquema y nombre —el de un índice solo es único adentro de su esquema— y el error de un borrado va aparte del de la lectura, que reemplaza la grilla entera.

### Errores

`pgforge_core::Error` traduce errores del servidor a variantes con significado: `Canceled` (usuario apretó cancelar, no es falla), `Permission`, `Conflict` (datos cambiaron entre lectura y escritura), `Database` (con `code`, `detail`, `hint` y `position` para resaltar en editor). Cruza el IPC como `ErrorPayload`, enum etiquetado con `kind`; interfaz lo consume con tipo `CoreError` de `ui/src/lib/ipc/core.ts` (`describeError`, `isCanceled`).

`Connection` y `ConnectionClosed` cruzan como **`Disconnected`**, aparte de `Other`, porque no son errores de la operación sino del vínculo: cualquier otra cosa contra ese servidor va a fallar igual. El `invoke` de `ipc/core.ts` los detecta al pasar y avisa por `onServerDown` —una sola vez, en un solo lugar— para que `explorer.markDown` pinte el servidor caído en el árbol con su botón de reconectar. `Row.down` es distinto de `connected: false` a propósito: bajar `connected` cierra todas las pestañas de ese servidor (efecto de `App.svelte`), y perder una consulta a medio escribir por un corte de tres segundos es peor que el corte.

### La frontera del IPC

`ui/src/lib/ipc/` = **único** lugar de la interfaz que habla con Rust: declara tipos espejo de los del core y envuelve cada `invoke`. Ningún componente llama a `invoke` por su cuenta. Al agregar comando hay que tocar tres puntos: módulo del core, comando en `src-tauri/src/commands/` **más su registro en el `generate_handler!` de `src-tauri/src/lib.rs`**, y la función y tipos en el módulo de `ipc/` que corresponda al dominio.

Adentro de `ipc/` hay un módulo por dominio —`core` (el error y los ayudantes), `servers`, `schema`, `monitor`, `backup`, `query`, `data`, `ddl`, `objects`, `settings`, `security`— y un `index.ts` que los reexporta, así que quien importa sigue escribiendo `from "./ipc"`. Era un archivo de dos mil líneas: la regla no cambió, cambió que ahora se puede leer el pedazo que a uno le toca.

**Las lecturas largas se pueden abortar.** La pestaña de consulta tiene sesión propia con su `CancelToken`, pero el árbol y el DDL toman una conexión del pool y la devuelven, así que no había a quién cancelarle. `conn::cancelable(sink, future)` corre una lectura adentro de un *task-local* donde `ServerHandle::client` anota el token de cada conexión que entrega —sin cambiarle la firma a ninguna función del núcleo—; `tree_children` y `object_ddl` aceptan un `requestId` opcional, lo registran en `AppState::reads` mientras dura, y `read_cancel` cancela todos sus tokens. Del lado de la interfaz, el chevron de una fila que carga se convierte en cruz, y cambiar de nodo en el panel de detalle aborta el `pg_dump` que ya no se va a mirar. Cancelar deja el nodo cerrado y sin cargar, no vacío: es lo que hace que el próximo intento vuelva a leer.

`describeError` no solo arma el texto: **escribe el error en el registro**, porque es el único embudo por el que pasa todo error que el usuario llega a ver. El registro lo maneja `tauri-plugin-log` (`src-tauri/src/lib.rs`), con archivo en el directorio de registros del sistema, rotación por tamaño y los tres últimos guardados; la ruta viaja en `AppInfo::log_dir` y la interfaz la muestra en el `title` de la versión. Una cancelación no se anota: no es una falla.

Convenciones de serde que interfaz da por hechas: `#[serde(rename_all = "camelCase")]` en todo, y enums como uniones etiquetadas — `kind` para variantes de datos (`Change`, `TableChange`, `Outcome`, `NodeKind`) y `type` para eventos de canal (`QueryEvent`, `MonitorEvent`, `ProcessEvent`). Flujos largos (ejecución de consultas, sondeo del dashboard, procesos en segundo plano) no devuelven resultado: mandan eventos por `Channel` de Tauri.

**Enum etiquetado necesita las dos:** `rename_all` renombra nombres de variante, `rename_all_fields` los campos de adentro de cada variante. Solo con la primera, un `new_name` o `type_name` sigue esperándose en `snake_case` mientras interfaz manda `newName`: nada falla al compilar y el `invoke` se cae recién en tiempo de ejecución, con «missing field» del lado de Rust. Por eso los enums de este proyecto llevan siempre `#[serde(tag = "…", rename_all = "camelCase", rename_all_fields = "camelCase")]`, aunque hoy no tengan campo de dos palabras. Lo que atrapa el olvido: `src-tauri/tests/preview.rs`, que arma la carga como JSON en vez de construir tipos de Rust.

Valores de celdas viajan como `string | null` (`null` = NULL de la base, distinto de cadena vacía), no como tipos nativos de JavaScript.

Lo que un perfil habilita o exige no se consulta al `explorer` desde cada diálogo, sino a `ui/src/lib/access.svelte.ts`: `isReadOnly`, `readOnlyReason` (motivo para el `title` de un botón apagado) y `confirmMutation`, que resuelve una promesa contra un único `Confirm.svelte` montado en `App.svelte` — así ningún diálogo de mutación tiene que anidar otro modal adentro del suyo. Todo `submit()` que modifica el servidor arranca con esa línea; en `DetailPanel` los botones que abren esos diálogos llevan `{...blocked}`, que agrega `disabled` **y** el motivo.

### Vista previa antes de aplicar

Toda mutación tiene dos comandos: uno que **genera el SQL** y otro que lo ejecuta — `ddl_preview` / `ddl_apply`, `data_preview` / `data_apply`, `data_export_preview` / `data_export_run`, `data_import_preview` / `data_import_run` (generan el `COPY … TO/FROM STDIN` exacto, ver `data::io`), `index_preview` / `index_create` (que lanza la tarea y devuelve su identificador, ver «Procesos en segundo plano»), `view_preview`, `trigger_preview`, `role_preview`, `privilege_preview`, `maintenance_plan` / `maintenance_run`, `backup_plan` / `backup_run` y `restore_plan` / `restore_run` (que en vez de SQL generan la línea de comando de `pg_dump` y `pg_restore`).

Import/export mueve datos en streaming por trozos (`copy_out`/`copy_in`), no acumula archivo en memoria, y reporta avance por el canal de procesos; import corre en una sola transacción para no dejar tabla a medias. Formato binario no es portable entre versiones distintas —interfaz lo advierte—.

Función generadora es **pura** a propósito: único verificable sin servidor, y garantiza que lo que interfaz muestra es exactamente lo que se va a ejecutar, no reconstrucción parecida. Al agregar operación que modifica servidor, mantener esa separación.

Reglas que sostienen edición de datos (`data::edit`): sin clave primaria/única, grilla se abre en solo lectura; cada `UPDATE` lleva valores originales de las columnas que cambia y, si afecta cero filas, reporta `Conflict` en vez de pisar; lote entero va en una transacción.

Orden y filtro de la pestaña de datos los resuelve el servidor, no la grilla: `PageView` (`data::page`) lleva la columna a ordenar —validada contra la forma de la tabla, porque llega de la interfaz y se interpola— y el predicado del `WHERE`, que va **crudo**, misma frontera de confianza que el editor. Con un orden elegido, el cursor por clave deja de valer —«lo que sigue de esta clave» no es lo que sigue en pantalla— y se pagina por `OFFSET`; la clave se agrega igual como desempate, o dos filas con el mismo valor podrían aparecer dos veces. Cambiar cualquiera de los dos vuelve a leer desde la primera tanda, así que la interfaz lo bloquea si hay ediciones sin guardar.

Los tipos de las columnas de una consulta se piden aparte (`QuerySession::column_types`, comando `query_column_types`) y solo con el interruptor «Tipos» encendido: el protocolo simple con el que se ejecuta no los trae, y saberlos cuesta **preparar** la sentencia de nuevo —otra vuelta al servidor y otra planificación—. Vale para una sola sentencia; un script lo rechaza el servidor y el encabezado se queda sin tipos, sin molestar con un error.

`ExportDialog` recibe un `ExportSource`, no una tabla: la pestaña de consulta exporta su resultado con `{ kind: "query" }` y el núcleo arma el `COPY (…) TO STDOUT`. Exportar **no** manda las filas de la pantalla: vuelve al servidor, así que el archivo trae todas y no las que entraron en el techo.

### Procesos en segundo plano

Lo que tarda no vive adentro de su diálogo. Mantenimiento (`VACUUM`/`ANALYZE`/`REINDEX`), creación de índices, backup, restore, importación y exportación se **lanzan y el diálogo se cierra**: siguen corriendo mientras se usa el resto de la aplicación, y se ven en la cuarta vista de la barra, «Procesos» (`ProcessPanel.svelte`). Antes cada uno tenía la ventana tomada hasta terminar, así que un `CREATE INDEX CONCURRENTLY` sobre una tabla grande era una tarde sin poder mirar otra cosa que no fuera cancelarlo.

**El dueño de lo que corre es Rust, no la ventana.** El registro vive en `src-tauri/src/process.rs` (`AppState::processes`) y guarda un `ProcessRecord` por proceso —qué corre, contra qué servidor, el SQL o la línea de comando, lo que fue informando, el avance y con qué terminó—. La interfaz es un espejo: se engancha una sola vez con `process_watch`, cuyo primer mensaje es el estado completo (`ProcessEvent::Snapshot`) y después llegan las novedades. Eso es lo que hace que **recargar la ventana no pierda nada**: los seis procesos siguen corriendo del otro lado y vuelven a aparecer, incluido el resultado de uno que terminó justo mientras no había nadie escuchando —antes ese resultado no quedaba en ningún lado—. Cerrar la ventana de un proceso tampoco lo toca: cancelar es explícito.

Cómo se corta lo sabe el registro (`Cancel`), así que hay **un solo** `process_cancel` y no uno por clase: a una sentencia del servidor se le pide al **servidor** que aborte —nunca se mata la tarea local, que dejaría al servidor terminando el `VACUUM` sin nadie escuchando— y a un proceso hijo se le avisa por su canal para que el núcleo lo mate y limpie lo que dejó a medias. Una sentencia larga es siempre lo mismo —una sesión dedicada, sin `statement_timeout`—, así que `commands::tasks::spawn_statement` la corre para las dos que la usan (`maintenance_run` e `index_create`); `Session::execute_batch` es lo que ejecuta: protocolo simple y sin transacción envolvente, que es justo lo que `CONCURRENTLY` necesita.

Dos techos, para que el registro no crezca sin límite en una sesión larga: 500 líneas de salida por proceso —`pg_restore` imprime miles y los errores están al final— y 50 terminados recordados. Lo que pasó de verdad ya vive en el historial, en SQLite.

Del lado de la interfaz, `tasks.svelte.ts` copia los récords a `TaskRun` y le agrega lo único que no cruza el canal: el `onDone` de quien lanzó —lo que hace que la lista de índices se relea cuando el índice **existe** y no cuando se apretó el botón—, que se anota por identificador y contempla que una tarea corta termine antes de que el comando responda. `task-format.ts` —el rótulo, lo que lleva corriendo, con qué terminó— es puro y va con Vitest.

**Avisa cuando termina** (`notify.svelte.ts`, con `tauri-plugin-notification`): un proceso de la lista y también una ejecución de la pestaña de consulta, que es el mismo caso —se lanza y uno se va a otra pantalla—. Se avisa igual con la ventana adelante, porque puede estar visible en otro monitor o con la vista de procesos cerrada; lo que no avisa es lo que terminó enseguida, con un umbral configurable (10 s por omisión, 0 avisa de todo) que evita convertir cada `SELECT 1` en una notificación. El interruptor y el umbral se configuran en la vista de procesos, que es donde uno está cuando se pregunta por qué no le avisaron. El permiso se pide la primera vez que hay algo para avisar, no al arrancar, y que el sistema lo niegue no muestra ningún error: lo que corría ya terminó y su resultado está en la lista.

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

Para la interfaz (Svelte 5, editor, árbol, grilla, diálogos), ver `ui/CLAUDE.md`.

## Nota sobre `plan-proyecto-pgtool-rust.md`

Documento de visión original; su tabla de stack está **desactualizada**: proyecto usa `tokio-postgres` + `deadpool` (no `sqlx`), CodeMirror (no Monaco), Svelte 5 con grillas propias (no TanStack Table) e instancias locales de PostgreSQL vía `PGFORGE_TEST_URLS` (no `testcontainers`). Sirve para alcance por fases; para stack mandan `Cargo.toml` y `ui/package.json`.