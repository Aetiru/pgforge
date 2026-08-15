# Plan de mejoras — post v0.1.0

Las diez fases del alcance inicial están cerradas y el núcleo cubre la administración común. Este
documento planifica la **siguiente capa de valor**, numerada como continuación (`Fase 11` en
adelante) para que encaje con la tabla del README y los mensajes de commit.

Cada fase respeta las reglas que ya ordenan el proyecto y que **no se renegocian**:

- La lógica vive en `pgforge-core`; `src-tauri` solo traduce y delega; `pgforge-cli` prueba que el
  core sirve sin ventana.
- Toda mutación tiene un par **generar SQL (puro) / ejecutar**, y la interfaz muestra exactamente lo
  que se va a ejecutar.
- Ninguna consulta atada a una versión: los saltos de catálogo se resuelven con predicados de
  `ServerCaps`.
- Tres puntos de IPC por comando: módulo del core, comando en `src-tauri/src/commands/` **con su
  registro en `generate_handler!` de `src-tauri/src/lib.rs`**, y la función y los tipos en `ipc.ts`.
- Tests de integración que crean su propio esquema, iteran `PGFORGE_TEST_URLS` y limpian aunque
  panickeen. Las funciones puras se testean sin servidor.
- Todo en español salvo los identificadores de código.

## Resumen de fases

| #  | Fase | Núcleo del cambio | Riesgo |
|----|------|-------------------|--------|
| 11 | Import/export de datos (`COPY`) | módulo nuevo `data::io` | medio |
| 12 | Túnel SSH | cablear `SshTunnel` (ya modelado) al `manager` | medio-alto |
| 13 | Monitoreo: bloqueos, índices sin uso y bloat | exponer lo que ya existe + `stats::bloat` | bajo |
| 14 | Diagrama ERD del esquema | introspección de FKs → grafo en UI | bajo (aditivo) |
| 15 | Red de seguridad: tests de vista previa e IPC | tests de `src-tauri` puro + Vitest en `ui` | bajo |

Orden recomendado: **11 → 13 → 15 → 12 → 14**. Las 11 y 13 dan valor inmediato con riesgo contenido;
la 15 conviene antes de la 12 (la más delicada) para no cablear SSH sin red; la 14 es la más
independiente y puede intercalarse cuando convenga.

---

## Fase 11 — Import/export de datos (`COPY`)

**Por qué.** Es el mayor hueco funcional: `data/` sabe leer de a páginas y editar, pero no mover
datos en bloque. Exportar una tabla o el resultado de una consulta, e importar un CSV, es tarea
diaria de administración. Encaja con la regla de vista previa: se muestra el `COPY … TO/FROM STDOUT`
exacto antes de ejecutar.

**Núcleo — `crates/pgforge-core/src/data/io.rs`** (nuevo, re-exportado en `data/mod.rs`).

- Función **pura** `export_command(spec) -> Statement` que arma el texto del `COPY`:
  - Origen: una tabla (`COPY <ident> [(cols)] TO STDOUT`) o una consulta cruda del usuario
    (`COPY (<sql>) TO STDOUT`), misma frontera de confianza que el editor.
  - Formato: `csv` (con `HEADER`, `DELIMITER`, `QUOTE`, `NULL`), `text` o `binary`.
  - Identificadores por `quote_ident`; el SQL de la consulta va crudo y documentado como tal, como
    el `USING` o el predicado de un índice parcial.
- Función **pura** `import_command(spec) -> Statement` con `COPY <ident> [(cols)] FROM STDIN`.
- Ejecución en streaming: `export_run` usa `copy_out` de `tokio-postgres` y va emitiendo trozos por
  un `Channel` (como la ejecución de consultas), no acumula el archivo en memoria; `import_run` usa
  `copy_in` leyendo el archivo por trozos. Reportar filas procesadas para una barra de progreso.
- `import_run` corre dentro de una transacción: un CSV a medio cargar no deja la tabla a medias.

**IPC / comandos** (`src-tauri/src/commands/data.rs`, registro en `lib.rs`, tipos en `ipc.ts`):

- `data_export_preview` / `data_export_run` (evento de canal `ExportEvent`: `Progress`, `Done`,
  `Error`).
- `data_import_preview` / `data_import_run` (evento `ImportEvent`).
- El destino/origen es una ruta de archivo elegida con el diálogo de Tauri; el core recibe la ruta,
  no el contenido.

**UI.** `ExportDialog.svelte` / `ImportDialog.svelte` con la forma estándar de los diálogos de
mutación (copia con `untrack`, `changes()`, `validate()`, botón «Ver SQL» → `*_preview`, `submit()`
→ `*_run`). Entradas desde el menú contextual de una tabla en `TreePanel` y desde la barra del
`ResultGrid` (exportar el resultado de la consulta actual). Barra de progreso con las filas
reportadas.

**Tests.** `crates/pgforge-core/tests/data_io.rs`: crea esquema y tabla, exporta a un archivo
temporal, la trunca, reimporta y verifica el conteo y algunas filas contra las URLs reales. Tests
unitarios puros de `export_command`/`import_command` (CSV con y sin header, delimitador custom,
columnas seleccionadas, `COPY (query)`), en la línea de los de `ddl::index`.

**Gating.** `COPY (query) TO` existe desde PG 9.0, dentro del rango; no hace falta predicado nuevo.
El formato binario entre versiones distintas no es portable — advertirlo en la UI.

**Riesgos.** El streaming grande es lo delicado: mantener la memoria plana y que cancelar corte el
`copy_out`/`copy_in` (reutilizar el `CancelToken` de la sesión). Encoding del archivo: fijar UTF-8 y
documentarlo.

---

## Fase 12 — Túnel SSH

**Por qué.** `SshTunnel` ya está en `ConnectionProfile` y en `ipc.ts`, pero el `manager` no lo usa:
el campo se agregó temprano justamente para no migrar los perfiles guardados después. Cablearlo
amplía a cuántos servidores reales se puede conectar la herramienta (bases tras un bastión).

**Núcleo — `crates/pgforge-core/src/conn/tunnel.rs`** (nuevo).

- Dependencia `russh` (SSH puro en Rust, async, sin OpenSSL del sistema; evita el enlace nativo de
  `ssh2`/`libssh2` que complica el build en las tres plataformas del CI).
- `open_tunnel(&SshTunnel, target_host, target_port) -> LocalForward`: abre un forward local
  (`direct-tcpip`) y devuelve el puerto local efímero al que `manager` conecta en vez de al host
  real. `LocalForward` cierra el canal en su `Drop`.
- Autenticación: clave privada (`private_key`, ya en el modelo) o contraseña (del `keyring`, bajo
  una clave derivada del `ProfileId` + `"ssh"`, sin mezclarla con la de la base).
- Verificación del host key contra `known_hosts` del usuario; primera conexión pide confirmación
  (evento hacia la UI), no acepta a ciegas.

**Integración en `manager.rs`.** Al abrir un `ServerHandle`, si el perfil trae `tunnel`, levantar el
forward **antes** de construir el pool y apuntar la cadena de conexión al puerto local. El túnel vive
junto al handle del servidor y se cierra cuando se cierra el servidor. Un solo túnel por servidor,
compartido por todos los pools de sus bases.

**IPC / comandos.** No hay comando nuevo de conexión: el túnel es transparente al conectar. Se suma:

- `ssh_test`: prueba el túnel aislado (útil en el diálogo de conexión antes de guardar).
- Evento/consulta para la confirmación de host key desconocido.
- La contraseña SSH entra por el flujo de credenciales existente (`secret`/`keyring`).

**UI.** Ampliar `ConnectionDialog.svelte` con una sección «Túnel SSH» (host, puerto, usuario,
clave/contraseña), plegada por defecto. Diálogo de confirmación de host key nuevo (usar `Confirm`).

**Tests.** El core: test unitario de la construcción de la config del forward (puro). El extremo a
extremo con SSH real no entra en `PGFORGE_TEST_URLS`; se documenta como manual con un `sshd` local, y
se agrega un comando a `pgforge-cli` (`pgforge tunnel --...`) para ejercitarlo a mano — fiel a que el
core debe usarse sin ventana.

**Gating.** Ninguno por versión de PostgreSQL. Sí verificar disponibilidad de `russh` en las tres
plataformas del CI.

**Riesgos.** Es la fase más delicada: manejo de claves, host keys, y errores de red que ahora tienen
una capa más. Traducir los errores de SSH a variantes de `pgforge_core::Error` con mensaje claro
(«no se pudo abrir el túnel», distinto de «no se pudo conectar a la base»).

---

## Fase 13 — Monitoreo: bloqueos, índices sin uso y bloat

**Por qué.** El core **ya calcula** el árbol de bloqueos (`monitor::activity::blocking_tree`,
`BlockNode`, `Lock`) y los índices sin uso (`monitor::stats::IndexStat::is_unused`). Lo que falta es
**exponerlo en la UI** y sumar la única pieza ausente: estimación de bloat. Es la fase de mayor
relación valor/esfuerzo y menor riesgo.

**Núcleo.**

- Reutilizar `blocking_tree`, `locks`, `IndexStat`/`is_unused`, `TableStat` — ya existen y tienen
  tests.
- **Nuevo** `monitor::stats::bloat`: estimación de bloat de tablas e índices por la consulta clásica
  sobre `pg_class`/`pg_statistic` (la de `check_postgres`/pgAdmin). Es estimación, no exacta —
  documentarlo. Gatear con `ServerCaps` si alguna columna cambió de nombre en el rango 13–17.
- Acciones ya soportadas que se enlazan desde estas vistas: cancelar/terminar backend
  (`can_signal_backends`), y `REINDEX`/`DROP INDEX` (Fase 5) sobre un índice marcado como candidato.

**IPC / comandos.** Sumar a `monitoring.rs`:

- `monitor_blocking_tree` (o incluirlo en el sondeo del dashboard que ya emite por `Channel`).
- `monitor_index_health` (índices sin uso ordenados + su tamaño) y `monitor_bloat`.

**UI.** En `Dashboard.svelte`:

- Pestaña/panel «Bloqueos»: render de `BlockNode` como árbol (reusar `PlanTree.svelte` o
  `BlockTree.svelte` que ya existe), con acción de terminar el backend raíz.
- Panel «Salud de índices»: tabla de índices sin uso y bloat estimado, con botón a `REINDEX` /
  `DROP INDEX CONCURRENTLY` que abre el `SqlPreview` existente. Nada de acción directa sin vista
  previa.

**Tests.** `blocking_tree` y `is_unused` ya tienen tests puros. Sumar test de integración de `bloat`
que solo verifica que la consulta corre y devuelve números razonables en las versiones reales (el
valor exacto no es determinista).

**Riesgos.** Bajo. La consulta de bloat es la única incógnita entre versiones; validarla contra PG 13
y 17.

---

## Fase 14 — Diagrama ERD del esquema

**Por qué.** La introspección de claves foráneas ya existe (la usa el árbol) y la UI ya sabe dibujar
(uPlot, íconos SVG a mano). Un diagrama de relaciones navegable diferencia a pgforge de `psql` y
reutiliza el core casi sin tocarlo. Es aditivo: no cambia nada de lo existente.

**Núcleo — `introspect`.**

- `schema_graph(schema) -> SchemaGraph`: nodos (tablas con sus columnas y clave) y aristas (FKs con
  columnas origen/destino). Una sola consulta sobre `pg_constraint` + `pg_attribute`, reusando lo
  que ya trae el árbol.
- Puro respecto del layout: el core entrega el grafo, **no** posiciona. El layout es cosa de la UI.

**IPC / comandos.** `schema_graph` en `schema.rs` + `ipc.ts`. Solo lectura, sin par preview/apply
(no muta nada).

**UI.** `Erd.svelte`: layout con un algoritmo simple (fuerza dirigida o en capas) en el cliente,
zoom/pan, resaltar una tabla y sus relaciones. Abrir desde el nodo de un esquema en `TreePanel`.
Exportar a SVG/PNG reutiliza el patrón de exportar gráficos. Sin librería pesada de grafos —
mantener la línea de «una librería completa pesa más que toda la interfaz».

**Tests.** Integración de `schema_graph`: crear tablas con FKs y verificar nodos y aristas contra las
URLs reales. El layout no se testea (es visual).

**Riesgos.** Bajo. El único costo real es el algoritmo de layout; empezar con uno básico y mejorar.

---

## Fase 15 — Red de seguridad: tests de vista previa e IPC

**Por qué.** CLAUDE.md dice que `src-tauri` y `ui` se verifican solo compilando. Pero las funciones
puras de vista previa y la frontera `ipc.ts` **son verificables sin servidor y sin ventana**, y son
justo donde un error se propaga silencioso a toda mutación. Es la única zona del proyecto sin red de
seguridad.

**Alcance.**

- **`src-tauri` (puro):** tests de los comandos `*_preview` que solo arman argumentos y delegan en
  las funciones puras del core, verificando que la traducción de argumentos no pierde nada. No
  necesitan servidor. Esto obliga a sacar `src-tauri` del excluido de `cargo test` del CI **solo**
  para estos tests puros (los que tocan servidor siguen en el core).
- **`ui` (Vitest):** tests de `ipc.ts` (`describeError`, `isCanceled`, y el mapeo de los tipos
  espejo) y de la lógica pura de los diálogos (`changes()`, `validate()` de `TableDialog`,
  `PolicyDialog`, etc.). Configurar Vitest en `ui/` y agregar `pnpm ui:test`.

**CI.** Añadir el job de `ui:test` y ampliar el de `src-tauri` para correr sus tests puros. Mantener
la matriz de PostgreSQL 13/17 solo en el core.

**Riesgos.** Bajo. Es infraestructura de calidad; el mayor cuidado es no arrastrar dependencias de
Tauri a los tests puros de `src-tauri`.

---

## Backlog menor (sin fase propia todavía)

- **Snippets/favoritos de consultas** sobre `sql::history` (ya persiste el historial): guardar,
  nombrar y reabrir consultas.
- **Exportar el plan de `EXPLAIN`** a JSON y visualización tipo flamegraph sobre `PlanTree.svelte`.
- **Replicación lógica de solo lectura**: estado de `pg_stat_replication` y slots. Explícitamente
  fuera del alcance inicial; candidata natural a una fase posterior.
- ~~**Comparar esquemas / generar diff DDL** entre dos servidores~~ — hecho en `pgforge-core::compare`
  (etapa 16). Queda pendiente la variante «contra un momento anterior»: exigiría guardar la
  instantánea en un archivo con su formato y su versionado, y hoy los dos lados se leen en vivo.

---

## Cómo se cierra cada fase

Igual que las diez anteriores: un commit `Fase N: <título>`, la tabla del README actualizada, tests
verdes contra PG 13 y 17 con `PGFORGE_TEST_URLS`, `cargo fmt`/`clippy` limpios con
`RUSTFLAGS="-D warnings"`, y `pnpm ui:check`. Nada llega a la UI sin su par de vista previa cuando
muta el servidor.
