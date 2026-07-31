# pgforge

Cliente de administración de PostgreSQL, open source, escrito en Rust.

Arranque instantáneo y bajo consumo de RAM: el núcleo es un crate de Rust y la interfaz corre
sobre el WebView nativo del sistema (Tauri), no sobre Electron.

**Estado: en desarrollo temprano.** Todavía no hay releases.

## Alcance

El objetivo es cubrir el ciclo completo de administración, no solo consultar datos. Se construye
de forma incremental y cada etapa entrega algo usable por sí sola:

| # | Etapa | Estado |
|---|---|---|
| 0 | Setup del proyecto | en curso |
| 1 | Conexión y exploración de esquema | pendiente |
| 2 | Monitoreo y mantenimiento | pendiente |
| 3 | Editor SQL + EXPLAIN | pendiente |
| 4 | Grilla de datos editable | pendiente |
| 5 | Gestión de objetos DDL | pendiente |
| 6 | Roles y permisos | pendiente |
| 7+ | Backup/restore, replicación, configuración del servidor | pendiente |

La visión completa está en [`plan-proyecto-pgtool-rust.md`](plan-proyecto-pgtool-rust.md).

## Versiones de PostgreSQL soportadas

PostgreSQL **13 o superior**. Las diferencias de catálogo entre versiones se resuelven en tiempo
de ejecución (`pgforge-core::caps`), no con consultas escritas contra una versión fija.

## Arquitectura

```
crates/pgforge-core/   Núcleo. Sin ninguna dependencia de Tauri ni de la UI.
crates/pgforge-cli/    Binario de línea de comandos sobre el core.
src-tauri/             Aplicación de escritorio: comandos Tauri y estado.
ui/                    Interfaz: Svelte 5 + TypeScript + Vite + Tailwind.
```

`pgforge-cli` existe para garantizar que el core sea usable sin interfaz gráfica. Si una
funcionalidad solo se puede ejecutar desde la ventana, el core está mal diseñado.

## Desarrollo

Requisitos: Rust estable, Node 20+, pnpm y — en Windows — Visual Studio Build Tools con el
workload de C++ para el linker de MSVC.

```bash
pnpm --dir ui install
cargo build --workspace     # compilar el core y la CLI
pnpm --dir ui tauri dev     # levantar la aplicación
```

Los tests de integración necesitan al menos una instancia real de PostgreSQL:

```bash
# Una o varias URLs separadas por coma; los tests corren contra todas.
export PGFORGE_TEST_URLS="postgres://postgres@localhost:5432/postgres,postgres://postgres@localhost:5433/postgres"
cargo test --workspace
```

## Licencia

Bajo cualquiera de estas dos licencias, a elección:

- Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT ([LICENSE-MIT](LICENSE-MIT))

Salvo indicación explícita en contrario, toda contribución enviada a este proyecto queda
licenciada de la misma forma dual, sin condiciones adicionales.
