# pgforge

Cliente de administración de PostgreSQL, open source, escrito en Rust.

Arranque instantáneo y bajo consumo de RAM: el núcleo es un crate de Rust y la interfaz corre
sobre el WebView nativo del sistema (Tauri), no sobre Electron.

**Estado: funcionalmente completo para las tareas comunes de administración**, en preparación de la
primera release (`v0.1.0`). Usable, pero todavía sin rodaje en producción.

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

La replicación (física y lógica) queda fuera del alcance inicial.

La etapa 6 cubre roles y membresías, GRANT/REVOKE sobre tablas, vistas, secuencias, funciones,
esquemas, bases y columnas, privilegios por omisión (`ALTER DEFAULT PRIVILEGES`) y Row-Level
Security.

La visión completa está en [`plan-proyecto-pgtool-rust.md`](plan-proyecto-pgtool-rust.md).

## Versiones de PostgreSQL soportadas

PostgreSQL **13 o superior**. Las diferencias de catálogo entre versiones se resuelven en tiempo
de ejecución (`pgforge-core::caps`), no con consultas escritas contra una versión fija.

## Instalación

Cuando haya una release publicada, los instaladores de cada plataforma estarán en la página de
[Releases](../../releases):

- **Windows**: `pgforge_x.y.z_x64-setup.exe` (NSIS) o `pgforge_x.y.z_x64_en-US.msi`. Usa el WebView2
  del sistema, que ya viene con Windows 10/11 al día.
- **macOS**: `pgforge_x.y.z_*.dmg`.
- **Linux**: `pgforge_x.y.z_amd64.deb` o `pgforge_x.y.z_amd64.AppImage`.

Para la última versión del código, o mientras no haya release, se compila desde el fuente (ver
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

## Licencia

Bajo cualquiera de estas dos licencias, a elección:

- Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT ([LICENSE-MIT](LICENSE-MIT))

Salvo indicación explícita en contrario, toda contribución enviada a este proyecto queda
licenciada de la misma forma dual, sin condiciones adicionales.
