# Hoja de ruta

Ideas para después de la v0.13.0, ordenadas por área. No es una promesa de fechas: es lo que
conviene hacer y por qué. El esfuerzo es una estimación gruesa: **S** horas, **M** días, **L** semanas.
Lo que dice «verificado» se comprobó en el código o con mediciones; lo demás es propuesta.

## Funciones nuevas

### Guardar y sincronizar los scripts con Git (GitHub o GitLab) — L
Que los scripts de cada conexión, las consultas guardadas y las abreviaturas vivan en un repositorio
propio del usuario: historial de cambios, revisión por pares y la misma carpeta en varios equipos.

- **Cualquier remoto, no una API por proveedor.** GitHub y GitLab hablan Git; con eso basta para
  clonar, hacer pull, commit y push. Abrir un *pull request* o *merge request* con la API de cada uno
  sería un paso posterior y opcional.
- **Las credenciales no son de la aplicación.** Se usa el agente SSH o el asistente de credenciales
  de Git del sistema; si hace falta un token, va al almacén del sistema (`keyring`), nunca a un archivo
  de configuración. Es la misma regla que ya siguen las contraseñas de los servidores.
- **Qué entra al repositorio:** los archivos de scripts, las consultas guardadas con nombre y las
  abreviaturas (que ya se exportan sin ids ni perfiles). **No** entran contraseñas, el historial de
  ejecución ni los resultados.
- **Aviso antes de subir:** un script puede traer datos sensibles (un `INSERT` con un correo real, una
  contraseña en un `CREATE ROLE`). Antes del primer push se muestra qué se sube y se avisa si el
  remoto es público.
- **Conflictos:** si el archivo cambió en ambos lados, se muestra la diferencia y el usuario decide;
  nunca se pisa en silencio.
- **Reparto de siempre:** la lógica de Git vive en `pgforge-core` (pura donde se pueda, probada con un
  repositorio temporal), los comandos de Tauri solo delegan, y todo push pasa por `confirmMutation`.

### SQL por lenguaje natural, con Claude — L
Describir lo que se quiere («pedidos pagados por ciudad el último mes») y obtener la consulta en el
editor para revisarla. **Nunca se ejecuta sola.**

- **Qué se envía.** Solo la estructura (nombres de esquemas, tablas, columnas, tipos y claves),
  nunca filas. Antes de enviar se muestra exactamente qué sale de la máquina, igual que la vista previa
  del SQL. Desactivado por omisión y activable por servidor; apagado para `prod` salvo que se pida.
- **Dos formas de conectarlo, para elegir al implementarlo:**
  1. **La API de Anthropic con la clave del usuario** (guardada en el almacén del sistema). Es la
     más directa y la que da más control sobre qué se envía.
  2. **La herramienta `claude` (Claude Code) instalada en el equipo**, invocada sin interfaz
     (`claude -p`). No hay clave que guardar en pgforge: usa la sesión del propio usuario.
  Modelos locales (Ollama) quedan como tercera opción para quien no pueda enviar nada fuera.
- **Salvaguardas.** El texto generado pasa por `statement-risk` (detecta `DROP`, `TRUNCATE`,
  `DELETE`/`UPDATE` sin `WHERE`), por la confirmación escalonada en producción y por el modo solo
  lectura del perfil. Se puede validar con `QuerySession::check` antes de ejecutar. Con «Ensayar»
  (`BEGIN … ROLLBACK`) se ve el efecto sin dejar nada aplicado.
- **Explicar y arreglar:** además de generar, «explicar esta consulta» y «arreglar este error»
  usando el mensaje de PostgreSQL y la posición del error que ya llegan al editor.
- **Qué no hacer:** mandar datos de las tablas, ejecutar sin revisión, ni guardar la clave en un
  archivo de la aplicación.

### Otras funciones
| # | Mejora | Esfuerzo |
|---|---|---|
| 1 | **Impacto antes de borrar o alterar** («esto rompe 3 vistas y 2 funciones»), cruzando Dependencias con los diálogos | M |
| 2 | **Visor de celdas** largas, JSON o XML con formato, búsqueda y copia (ya existe `JsonTree`) | S |
| 3 | **Graficar un resultado** de consulta (barras, líneas, torta); ya existe `Chart.svelte` en monitoreo | M |
| 4 | **Roles, fase 2 y 3:** editar permisos desde la matriz y «Copiar los accesos de otro rol…» | M |
| 5 | **Consultas programadas y alertas** (avisar si cambia un resultado) | L |
| 6 | **Exportar resultados a Excel (.xlsx)** | S |
| 7 | **Comparar dos resultados** de consultas, no solo tablas | M |
| 8 | **Compartir configuración de equipo:** exportar e importar perfiles y carpetas sin contraseñas | M |
| 9 | **Plantillas de consulta con parámetros** y un formulario para rellenarlos (hoy hay `{{nombre}}`) | S |
| 10 | **De la comparación de esquemas a la migración con un clic** (hoy el flujo existe, pero hay que guardar el script a mano) | M |

## Pulido pendiente
- «Eliminar…» baja a otra línea en el inspector angosto.
- Los títulos de Usuarios, Grupos y Sistema y los de las carpetas del árbol no cambian de idioma
  hasta releer el árbol.
- Queda una traducción sin uso («Qué puede hacer»).
- Editar o Eliminar desde el menú reabre el inspector y recuerda esa preferencia.
- El mock de pruebas (`ui/scripts/mock-tauri.js`) devuelve `null` en `server_config` y `table_data`.
- Antes de la v0.13.0 final hay que ajustar `bundle.windows.wix.version` en `tauri.conf.json`: WiX no
  acepta `-rc.N`, así que el MSI lleva una versión numérica propia.

## Rendimiento
Verificado: el proceso de Rust ocupa menos de 50 MB; con la ventana (WebKit) el total ronda entre
135 y 280 MB según lo que haya abierto. Las filas de una pestaña pesan poco (unos 4-5 MB de JavaScript
por 5000×11 celdas) y no hay fuga al cerrar pestañas; el resto es el motor web.

- Medir un **build de release** con carga real: todo lo medido hasta ahora fue debug.
- Perfilar el **costo de dibujar la grilla en WebKit** (capas y DOM).
- Una prueba de **estrés**: varios servidores conectados, consultas en paralelo y pestañas pesadas.
- Cargar bajo demanda los módulos pesados (monitoreo, diagrama, comparar).
- Soltar los datos de las pestañas inactivas: ahorra poco (unos 6 MB por pestaña) y cambia el
  comportamiento visible (scroll, recarga, estado de edición); va al final.
- No se recomienda reescribir la interfaz en Rust nativo solo para ahorrar memoria.

## Distribución
- **Firmar los instaladores** (Windows y macOS) para evitar los avisos de SmartScreen y Gatekeeper.
- Actualizador integrado; hoy solo avisa de una versión nueva, y descarta los prelanzamientos.
- Paquetes de Flatpak, Homebrew o winget además del AppImage y el `.deb`.
- Un `CHANGELOG` y notas de versión automáticas.
- Probar los instaladores en Windows y macOS: hasta ahora solo se comprobó que compilan.

## Proyecto abierto
- `CONTRIBUTING.md`, plantillas de issue y de PR, y etiquetas `good first issue`.
- Un `SECURITY.md` con cómo reportar vulnerabilidades.
- README completo en inglés y un GIF corto de uso.
- Pruebas de interfaz de extremo a extremo (Playwright) en CI y un job que avise si sube la memoria.
- Que cada comando nuevo del IPC tenga su prueba con la carga JSON tal como la manda la interfaz
  (como `src-tauri/tests/preview.rs`).
- Pruebas de la interfaz con roles de permisos distintos (por ejemplo, solo lectura).

## Descartado por ahora
- **Reescribir la interfaz en Rust nativo** (`egui`, `iced`, `slint`) para ahorrar memoria: son meses
  de trabajo para unos 100 a 200 MB, con riesgo en accesibilidad y texto en Windows y macOS.
- **Edición colaborativa o sesiones compartidas:** exige un servidor propio y cambia el modelo de
  seguridad de una aplicación que hoy no tiene cuentas.
- **SQL por lenguaje natural que envíe datos de las tablas:** solo se contempla con la estructura (ver
  arriba), nunca con filas.

## Decisiones abiertas
- Cómo conectar el SQL por lenguaje natural: la API de Anthropic con la clave del usuario, la
  herramienta `claude` instalada en el equipo, o un modelo local. La propuesta es empezar por `claude -p`
  porque no obliga a guardar ninguna clave en pgforge.
- Si la sincronización con Git guarda también las abreviaturas y consultas guardadas o solo los
  archivos de scripts.
- Si se publican estas ideas como issues de GitHub para que quien pruebe la rc.2 vote lo que prefiere.
