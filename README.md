# pgforge

Cliente de administración de PostgreSQL, open source, escrito en Rust.

Arranque instantáneo y bajo consumo de RAM: el núcleo es un crate de Rust y la interfaz corre
sobre el WebView nativo del sistema (Tauri), no sobre Electron.

**Estado: funcionalmente completo para las tareas comunes de administración.** Ya hay releases
publicadas (ver [Instalación](#instalación)), pero todavía sin rodaje en producción: úsalo con
cuidado contra servidores reales, y con la marca de entorno `prod` y el modo solo lectura de cada
perfil si quieres una red de seguridad extra.

*English: pgforge is an open-source PostgreSQL administration client written in Rust (Tauri +
Svelte). The UI is available in Spanish and English; see [English summary](#english-summary).*

## Qué es

Un cliente de escritorio para administrar PostgreSQL: explorar esquemas, editar datos, escribir y
planificar consultas, gestionar objetos, roles y permisos, hacer backup y restore, comparar
esquemas y datos entre servidores, y monitorear. Toda la lógica vive en un núcleo reutilizable
(`pgforge-core`) que también expone una CLI. Antes de aplicar cualquier cambio muestra el SQL
exacto que se va a ejecutar.

## Idioma de la interfaz

La interfaz está en **español e inglés**. Se elige en **Preferencias → Idioma**. El código, los
comentarios y los mensajes de commit del proyecto están en español.

## Requisitos

- Un servidor PostgreSQL **13 a 17** (el CI prueba el núcleo contra las cinco versiones).
- `pg_dump` y `pg_restore` instalados en la máquina **solo** para backup, restore y ver el DDL de
  tablas; el resto funciona sin ellos. Deben ser de una versión igual o mayor que la del servidor.
- Windows 10/11 (usa WebView2), macOS o Linux (WebKitGTK 4.1).

## Alcance

El objetivo es cubrir el ciclo completo de administración, no solo consultar datos. Se construye
de forma incremental y cada etapa entrega algo usable por sí sola:

| # | Etapa | Estado |
|---|---|---|
| 0 | Setup del proyecto | listo |
| 1 | Conexión y exploración de esquema | listo |
| 2 | Monitoreo y mantenimiento | listo |
| 3 | Editor SQL + EXPLAIN | listo |
| 4 | Grilla de datos editable | listo |
| 5 | Gestión de objetos DDL | listo |
| 6 | Roles y permisos | listo |
| 7 | Backup/restore | listo |
| 8 | Extensiones y datos externos (FDW) | listo |
| 9 | Configuración del servidor (`pg_settings`) | listo |
| 10 | Empaquetado y primera release | en curso |
| 11 | Import/export de datos (`COPY`) | listo |
| 12 | Túnel SSH (bastión) | listo |
| 13 | Monitoreo: bloqueos, índices sin uso y bloat | listo |
| 14 | Diagrama ERD del esquema | listo |
| 15 | Red de seguridad: tests de vista previa e IPC | listo |
| 16 | Comparación de esquemas entre servidores | listo |
| 17 | Procesos en segundo plano | listo |
| 18 | Plan de ejecución: copiar y sugerencias | listo |
| 19 | Índices que sobran (duplicados y cubiertos) | listo |
| 20 | Paleta de comandos, historial de lo aplicado e importar servidores | listo |

Las etapas 11 en adelante son la capa de mejoras posterior a la primera release; su plan está en
[`plan-mejoras.md`](plan-mejoras.md). La replicación (física y lógica) queda fuera del alcance inicial.

La etapa 6 cubre roles y membresías, GRANT/REVOKE sobre tablas, vistas, secuencias, funciones,
esquemas, bases y columnas, privilegios por omisión (`ALTER DEFAULT PRIVILEGES`) y Row-Level
Security.

La visión completa está en [`plan-proyecto-pgtool-rust.md`](plan-proyecto-pgtool-rust.md).

## Versiones de PostgreSQL soportadas

PostgreSQL **13 o superior**. Las diferencias de catálogo entre versiones se resuelven en tiempo
de ejecución (`pgforge-core::caps`), no con consultas escritas contra una versión fija.

## Instalación

Los instaladores de cada plataforma están en la página de
[Releases](https://github.com/Aetiru/pgforge/releases):

- **Windows**: `pgforge_x.y.z_x64-setup.exe` (NSIS) o `pgforge_x.y.z_x64_en-US.msi`. Usa el WebView2
  del sistema, que ya viene con Windows 10/11 al día.
- **macOS**: `pgforge_x.y.z_*.dmg`.
- **Linux**: `pgforge_x.y.z_amd64.deb` o `pgforge_x.y.z_amd64.AppImage`.

Los instaladores todavía no están firmados, así que Windows SmartScreen o Gatekeeper en macOS
pueden advertirlo al abrirlos. Para la última versión del código se compila desde el fuente (ver
[Desarrollo](#desarrollo)): `pnpm install && pnpm build` deja los instaladores en
`target/release/bundle/`.

## Arquitectura

```
crates/pgforge-core/   Núcleo. Sin ninguna dependencia de Tauri ni de la UI.
crates/pgforge-cli/    Binario de línea de comandos sobre el core.
src-tauri/             Aplicación de escritorio: comandos Tauri y estado.
ui/                    Interfaz: Svelte 5 + TypeScript + Vite + Tailwind.
```

`pgforge-cli` existe para garantizar que el core sea usable sin interfaz gráfica. Si una
funcionalidad solo se puede ejecutar desde la ventana, el core está mal diseñado.

```bash
pgforge info
pgforge server --url postgres://postgres@localhost:5432/postgres
pgforge tree   --url postgres://postgres@localhost:5432/postgres --depth 4
pgforge ddl    --url postgres://postgres@localhost:5432/postgres public.clientes
pgforge data   --url postgres://postgres@localhost:5432/postgres public.clientes --limit 20
pgforge query  --url postgres://postgres@localhost:5432/postgres --sql "SELECT * FROM clientes"
pgforge query  --url postgres://postgres@localhost:5432/postgres --sql "SELECT …" --explain --analyze
pgforge backup  --url postgres://postgres@localhost:5432/postgres --out copia.dump --format custom
pgforge restore --url postgres://postgres@localhost:5432/postgres --source copia.dump --clean
```

El backup necesita `pg_dump` y el restore `pg_restore`. Se buscan en el `PATH` y en las rutas
habituales de cada sistema; si hay varias instalaciones, `PGFORGE_PG_DUMP` y `PGFORGE_PG_RESTORE`
mandan. El restore no lee el formato plano: ese es un script SQL que se aplica con `psql`.

## Contraseñas

Nunca se guardan en los archivos de la aplicación. El archivo de conexiones solo tiene los datos
del servidor; la contraseña va al almacén de credenciales del sistema operativo (Credential
Manager, Keychain o Secret Service) y solo si se pide recordarla.

El secreto de un túnel SSH (la contraseña del bastión o la frase de la clave privada) va al mismo
almacén, bajo una clave aparte de la de la base. La clave del host del bastión se verifica contra el
`known_hosts` del usuario: un host desconocido —o uno cuya clave cambió— pide confirmación mostrando
la huella, nunca se acepta a ciegas.

## Desarrollo

Requisitos: Rust estable, Node 20+, pnpm y — en Windows — Visual Studio Build Tools con el
workload de C++ para el linker de MSVC.

```bash
pnpm install                # el CLI de Tauri vive en la raíz del workspace
cargo build --workspace     # compilar el core y la CLI
pnpm dev                    # levantar la aplicación
```

Los tests de integración necesitan al menos una instancia real de PostgreSQL. Se ejecutan contra
todas las URLs indicadas, lo que permite validar de una sola pasada que las consultas al catálogo
funcionan igual en distintas versiones del servidor:

```bash
export PGFORGE_TEST_URLS="postgres://postgres@localhost:5432/postgres,postgres://postgres@localhost:5433/postgres"
cargo test --workspace
```

Sin esa variable, los tests de integración no verifican nada y lo avisan por consola.

No todo necesita un servidor. La lógica pura —lo que la interfaz muestra antes de aplicar un cambio,
y la traducción de argumentos en la frontera con Rust— se prueba sola:

```bash
pnpm ui:test                # Vitest sobre ui/
pnpm ui:build               # `cargo test -p pgforge-app` necesita ui/dist
cargo test -p pgforge-app   # comandos de vista previa, sin red ni ventana
```

Para no dejar credenciales en el historial del intérprete de comandos, conviene ponerlas en un
archivo `.env.local` en la raíz del repositorio (ya está ignorado por git):

```powershell
# .env.local
PGFORGE_TEST_URLS=postgres://postgres:clave@localhost:5432/postgres,postgres://postgres:clave@localhost:5433/postgres
```

```powershell
$env:PGFORGE_TEST_URLS = ((Get-Content .env.local) -match 'PGFORGE_TEST_URLS=') -replace '^[^=]*='
cargo test --workspace
```

## Contribuir

Los issues y pull requests son bienvenidos en
[GitHub](https://github.com/Aetiru/pgforge). Antes de abrir un PR conviene correr
`cargo fmt --all --check`, `cargo clippy -p pgforge-core -p pgforge-cli --all-targets`,
`cargo test -p pgforge-core -p pgforge-cli` (con `PGFORGE_TEST_URLS`, ver arriba) y
`pnpm ui:check && pnpm ui:test`. Reglas de diseño y convenciones en [`CLAUDE.md`](CLAUDE.md) y
[`ui/CLAUDE.md`](ui/CLAUDE.md): la lógica va en el núcleo, cada mutación tiene vista previa, y
los textos del proyecto están en español.

## Licencia

Bajo cualquiera de estas dos licencias, a elección:

- Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT ([LICENSE-MIT](LICENSE-MIT))

Salvo indicación explícita en contrario, toda contribución enviada a este proyecto queda
licenciada de la misma forma dual, sin condiciones adicionales.

## English summary

pgforge is an open-source PostgreSQL administration client written in Rust: instant startup and
low memory use, with the UI running on the system's native WebView (Tauri) instead of Electron.

- **What it does:** schema explorer, editable data grid, SQL editor with EXPLAIN, DDL for tables,
  views, indexes, roles and privileges, backup/restore, `COPY` import/export, SSH tunnel,
  monitoring and maintenance, schema and data comparison, and an ERD diagram. Every change shows
  the exact SQL before it is applied.
- **Requirements:** PostgreSQL 13 to 17. `pg_dump` / `pg_restore` are needed only for backup,
  restore and table DDL.
- **Install:** download the installer for your platform (Windows `.exe`/`.msi`, macOS `.dmg`,
  Linux `.deb`/`.AppImage`) from [Releases](https://github.com/Aetiru/pgforge/releases), or build
  from source (see [Desarrollo](#desarrollo)).
- **Language:** the interface is available in English and Spanish; pick it in
  Preferences > Language. Project docs and code comments are in Spanish.
- **Contributing:** issues and PRs welcome; see [Contribuir](#contribuir).
- **License:** Apache-2.0 or MIT, at your option.
