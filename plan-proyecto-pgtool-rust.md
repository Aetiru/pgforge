# Plan de Trabajo: Cliente PostgreSQL Open Source en Rust
## (Alcance: administración completa, tipo pgAdmin)

## 1. Visión del proyecto

Construir una alternativa moderna, rápida y open source a pgAdmin, usando Rust como backend y Tauri como shell de escritorio, cubriendo **todo el ciclo de administración de PostgreSQL**: no solo consultar datos, sino gestionar objetos, seguridad, replicación, backups y monitoreo.

**Principios de diseño:**
- Arranque instantáneo, bajo consumo de RAM (a diferencia de Electron)
- Multiplataforma (Windows/Mac/Linux) desde el día uno
- Núcleo desacoplado de la UI (el core en Rust debería poder usarse también como CLI o librería)
- Licencia permisiva (MIT o Apache-2.0)

**Aviso de escala honesto:** pgAdmin tiene ~15 años de desarrollo. Este plan asume que el objetivo es llegar a una herramienta *usable en producción para las tareas más comunes* de forma incremental, no clonar el 100% de pgAdmin de entrada. Cada fase entrega algo funcional por sí sola.

**Nombre propuesto:** a definir (candidatos: `pgforge`, `rustgres`, `elephantui`)

---

## 2. Stack tecnológico

| Capa | Tecnología | Notas |
|---|---|---|
| Lenguaje core | Rust (edition 2021) | |
| Shell desktop | Tauri 2.x | |
| Conexión a DB | `sqlx` | Async, pool, soporte nativo Postgres |
| Runtime async | `tokio` | |
| Parsing SQL | `sqlparser-rs` | Autocompletado, validación |
| Backup/restore | wrapper sobre binarios `pg_dump`/`pg_restore`/`pg_basebackup` del sistema | Alternativa: reimplementar el formato custom en Rust a futuro (alto esfuerzo, no prioritario) |
| Credenciales | `keyring` | Integra con Keychain/Credential Manager/libsecret del OS, nunca passwords en texto plano |
| Serialización | `serde` + `serde_json` | |
| Frontend framework | React o Svelte (definir según objetivo de comunidad — ver conversación previa) | |
| Editor de código | Monaco Editor | Con soporte para visualizar planes de EXPLAIN |
| Tablas de datos | TanStack Table (headless) + virtualización | Necesario para tablas grandes y para el dashboard de monitoreo (refresco constante) |
| Gráficos (dashboard monitoreo) | `recharts` o `uPlot` (más liviano, mejor para series en tiempo real) | |
| Estilos | Tailwind CSS | |
| Testing backend | `cargo test` + `testcontainers-rs` | Postgres real en Docker por test |
| CI/CD | GitHub Actions | Build multiplataforma, releases automáticos |

---

## 3. Arquitectura general

```
┌─────────────────────────────────────────┐
│              Frontend                     │
│  Explorador │ Editor SQL │ Dashboard      │
│  de objetos │ + EXPLAIN  │ monitoreo      │
└───────────────────┬───────────────────────┘
                     │ Tauri IPC
┌───────────────────▼───────────────────────┐
│            Core (Rust, crate lib)          │
│  connection_manager  │ schema_introspection │
│  query_executor       │ ddl_generator        │
│  role_manager          │ backup_manager        │
│  monitoring_poller       │ config_manager        │
└───────────────────┬───────────────────────┘
                     │
              ┌──────▼──────┐
              │ PostgreSQL  │
              │ (+ binarios │
              │  pg_dump,   │
              │  pg_restore)│
              └─────────────┘
```

**Decisión de diseño importante:** cada "dominio" de administración (roles, backups, monitoreo, config) se implementa como módulo independiente dentro del crate `core`, con su propio set de comandos Tauri. Esto permite desarrollarlos y testearlos en paralelo sin que se pisen entre sí, y facilita que colaboradores externos tomen un módulo completo.

---

## 4. Catálogo completo de funcionalidades (por dominio)

### A. Objetos de esquema (DDL)
Tablas, vistas, vistas materializadas, secuencias, funciones/procedimientos (PL/pgSQL y otros lenguajes soportados), triggers, event triggers, tipos personalizados (enum/composite/range), dominios, reglas, colaciones, índices (btree/gin/gist/hash/brin, parciales, de expresión), constraints (PK/FK/unique/check/exclusion).
Para cada uno: crear, editar (ALTER), eliminar, ver DDL generado, dependencias.

### B. Seguridad
Roles y usuarios, membresías de grupo, GRANT/REVOKE granular por objeto, Row-Level Security (policies), gestión de passwords y expiración, atributos de rol (LOGIN, SUPERUSER, CREATEDB, etc.).

### C. Datos
Editor SQL con autocompletado y EXPLAIN/EXPLAIN ANALYZE (con visualización gráfica del plan), grilla de datos editable, import/export CSV, historial de queries.

### D. Backup y restore
Integración con `pg_dump`/`pg_restore` (formatos plain/custom/directory), programación de backups, restore selectivo (por tabla/schema).

### E. Monitoreo y mantenimiento
Dashboard de sesiones activas (`pg_stat_activity`), locks y bloqueos, queries lentas, cancelar/matar procesos (`pg_cancel_backend`/`pg_terminate_backend`), estadísticas de uso de índices y bloat, VACUUM/ANALYZE/REINDEX manual desde la UI.

### F. Replicación
Estado de replicación física (streaming, standbys, lag), gestión de publicaciones/suscripciones (replicación lógica).

### G. Extensibilidad e integración
Extensiones (instalar/actualizar/eliminar), Foreign Data Wrappers y foreign tables/servers, tablespaces.

### H. Configuración del servidor
Ver y editar parámetros runtime (`pg_settings`), edición asistida de `postgresql.conf`/`pg_hba.conf` cuando el archivo es accesible.

---

## 5. Roadmap por fases (priorizado por lo que se usa en el día a día)

### Fase 0 — Setup (1 semana)
- [ ] Repo, licencia, scaffold del workspace (Tauri + frontend + crate `core`)
- [ ] CI básico (build + test)

### Fase 1 — Conexión y exploración de esquema (2-3 semanas)
- [ ] Gestor de conexiones (múltiples servidores, grupos, credenciales vía `keyring`)
- [ ] Árbol de objetos: bases → schemas → tablas/vistas/funciones/secuencias/tipos
- [ ] Ver DDL de cualquier objeto seleccionado

### Fase 2 — Editor SQL avanzado (2-3 semanas)
- [ ] Monaco con autocompletado basado en esquema
- [ ] Ejecución de queries, múltiples statements, tabs de resultados
- [ ] EXPLAIN / EXPLAIN ANALYZE con visualización del plan
- [ ] Historial de queries

### Fase 3 — Edición de datos (2 semanas)
- [ ] Grilla editable (INSERT/UPDATE/DELETE), paginación eficiente
- [ ] Preview del SQL generado antes de aplicar cambios

### Fase 4 — Gestión de objetos DDL (3-4 semanas)
- [ ] Crear/editar/eliminar tablas, columnas, índices, constraints desde formularios
- [ ] Crear/editar funciones, triggers, vistas
- [ ] Manejo de dependencias (avisar antes de un DROP que rompe algo)

### Fase 5 — Seguridad (2 semanas)
- [ ] Gestión de roles, membresías, atributos
- [ ] GRANT/REVOKE por objeto vía UI
- [ ] Row-Level Security policies

### Fase 6 — Monitoreo y mantenimiento (2-3 semanas)
- [ ] Dashboard de sesiones activas y locks, con refresco en tiempo real
- [ ] Cancelar/matar procesos desde la UI
- [ ] Estadísticas de tablas/índices, bloat estimado
- [ ] VACUUM/ANALYZE/REINDEX manual

### Fase 7 — Backup/restore (2-3 semanas)
- [ ] Wrapper sobre `pg_dump`/`pg_restore` con UI de opciones
- [ ] Restore selectivo

### Fase 8 — Replicación y extensibilidad (3-4 semanas)
- [ ] Estado de replicación física
- [ ] Publicaciones/suscripciones lógicas
- [ ] Gestión de extensiones y FDWs

### Fase 9 — Configuración del servidor (1-2 semanas)
- [ ] Ver/editar `pg_settings`
- [ ] Edición asistida de archivos de configuración

### Fase 10 — Pulido, empaquetado y v1.0 (2 semanas)
- [ ] Manejo de errores robusto, temas claro/oscuro
- [ ] Empaquetado multiplataforma vía GitHub Actions
- [ ] Documentación de usuario

**Estimación total:** ~24-30 semanas (6-7 meses) trabajando de forma constante part-time; significativamente menos con más de un contribuidor activo por fase, ya que los módulos (B-H) son bastante independientes entre sí una vez que el core de conexión/esquema (Fase 1) está listo.

---

## 6. Riesgos y decisiones abiertas

| Riesgo/Decisión | Impacto | Notas |
|---|---|---|
| Alcance total es un proyecto multi-mes | Alto | Priorizar Fases 1-3 como "release usable" (equivalente a un editor SQL + explorador decente), el resto como releases incrementales |
| Backup/restore dependiendo de binarios externos (`pg_dump`) | Medio | Requiere que el usuario tenga las client tools de Postgres instaladas, o empaquetarlas junto a la app (aumenta tamaño del instalador) |
| Autocompletado SQL "inteligente" | Alto esfuerzo | Empezar simple (nombres de esquema), no full parsing semántico |
| Editar `postgresql.conf`/`pg_hba.conf` remotamente | Complejo/riesgoso | Solo viable si la app corre en el mismo host o vía agente adicional; en conexiones remotas puras esto es limitado por diseño de Postgres |
| Dataset grandes en grillas | Performance | Virtualización obligatoria desde el inicio |
| Contribuidores externos | Depende del stack frontend elegido | Ver discusión React vs Svelte |

---

## 7. Próximo paso inmediato

Recomendación: arrancar por la Fase 0 + Fase 1 (conexión + exploración de esquema) para tener algo ejecutable rápido, y desde ahí ir sumando los módulos de la Fase 4 en adelante según qué te importe más resolver primero (¿lo que más usás vos hoy en pgAdmin es DDL de tablas, monitoreo, o gestión de roles?).
