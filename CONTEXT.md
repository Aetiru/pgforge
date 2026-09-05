# pgforge

Cliente de administración de PostgreSQL. Este documento fija el vocabulario del dominio —qué
significa cada término dentro del proyecto—, no cómo está implementado.

## Language

**Perfil**:
Datos guardados de un servidor en `connections.json`: host, puerto, `environment`, `read_only`,
`autocommit`, grupo. Nunca contraseña — esa va al `keyring` del sistema.
_Avoid_: conexión guardada, configuración de servidor.

**Servidor**:
Instancia en runtime de un perfil activo, dentro de `ConnectionManager` — su pool por base
abierta y su túnel SSH si tiene uno.
_Avoid_: perfil (eso es antes de conectar), conexión (eso es un cliente concreto salido de acá).

**Conexión**:
Un cliente concreto contra la base: prestado del pool de un servidor (préstamo corto, se
devuelve enseguida) o una sesión dedicada.
_Avoid_: servidor, perfil.

**Sesión**:
Conexión dedicada de una pestaña de consulta, no prestada del pool: tiene su propio `TxStatus` y
`CancelToken`, y sobrevive entre sentencias — por eso `BEGIN`, `SET` o una tabla temporal siguen
valiendo en la siguiente.
_Avoid_: conexión (para un préstamo corto del pool, no para esto).

**Capacidad de versión**:
Qué ofrece el servidor por ser la versión que es (`has_pg_stat_io()`, `has_query_id()`, …). Se
resuelve una vez al conectar y no cambia mientras dure la conexión: se invalida con la versión.
_Avoid_: permiso, capacidad a secas (ambiguo con permiso de rol).

**Permiso de rol**:
Qué puede hacer el rol conectado (`is_superuser`, `can_signal_backends`, `can_read_all_stats`).
Se invalida con la identidad: cambia si se reconecta con otro rol o si alguien toca los `GRANT`
de ese rol, no con la versión del servidor.
_Avoid_: capacidad de versión, capacidad a secas.

> **Deuda conocida**: el struct `ServerCaps` (`pgforge-core::caps`) carga capacidad de versión y
> permiso de rol juntos, en una sola pasada al catálogo al conectar. Es conveniencia de
> implementación, no unidad conceptual — los dos términos de arriba siguen siendo distintos en
> el dominio aunque compartan struct.

## Cuatro formas de guardar algo

Los cuatro guardan algo, pero ninguno es sinónimo de otro. Cada entrada fija quién lo crea, a qué
está atado y dónde vive.

**Historial**:
Lo que la aplicación ejecutó de verdad contra un servidor. Se crea solo: cada `*_apply` de un
diálogo lo anota sin que el usuario pida nada (`source = dialog`). Queda atado a qué servidor y
qué base como dato, no como restricción — no exige que el perfil siga existiendo, ni filtra por
él. Vive en `history.db` (SQLite).
_No confundir con_: Consulta guardada — el historial es lo que pasó, crece sin techo y se vacía
entero sin que duela; una guardada es lo que el usuario decidió conservar, con nombre.

**Consulta guardada**:
SQL que el usuario decidió conservar a mano, con nombre obligatorio y único (sin distinguir
mayúsculas). No está atada a nada para correr: el mismo texto vale contra cualquier base;
servidor y base de origen se guardan como dato, igual que en el historial. Vive en `saved.db`
(SQLite), archivo aparte del historial.
_No confundir con_: Marcador — la guardada es texto portable entre bases; el marcador apunta a
un objeto concreto de un servidor concreto y no tiene sentido en otro. Ver [ADR-0004](docs/adr/0004-clave-del-marcador-ata-a-servidor.md).

**Marcador**:
Objeto del catálogo que el usuario marcó con la estrella del árbol. Se crea a mano; `add` es
idempotente — apretar la estrella de nuevo no duplica ni falla. Está atado a un objeto concreto:
`(profile_id, database, schema, name, kind)`, porque `public.clientes` de dev y de prod son
objetos distintos. Vive en su propio archivo SQLite, aparte de historial y guardadas. Ver
[ADR-0004](docs/adr/0004-clave-del-marcador-ata-a-servidor.md).
_No confundir con_: Consulta guardada — esa es texto portable; el marcador no.

**Script**:
Archivo `.sql` que el usuario escribe o importa, guardado en una carpeta por conexión. Se crea a
mano. Está atado a una conexión por nombre de carpeta (saneado con `folder_name`), no a un
objeto del catálogo ni a un servidor en runtime. Vive en disco, no en SQLite — árbol de carpetas
bajo una raíz que el núcleo recibe y no resuelve (decisión de la aplicación de escritorio, no del
crate).
_No confundir con_: Marcador — el script no apunta a un objeto del catálogo, y sigue existiendo
aunque el servidor de esa conexión ya no esté.

## Procesos en segundo plano

**Proceso**:
Lo que corre en segundo plano y sobrevive a la ventana — mantenimiento, creación de índice,
backup, restore, import/export. El dueño es Rust (`ProcessRecord`, `AppState::processes`), no la
ventana: recargarla no lo corta ni le hace perder lo que llevaba informado.
_Avoid_: tarea.
_No confundir con_: lectura cancelable — el árbol y el DDL cancelan una lectura de primer plano
(`requestId` + `cancelable()` + `read_cancel`) que no deja nada corriendo después. Un proceso es
de segundo plano y sigue vivo aunque se cierre lo que lo pidió; una lectura cancelada, no.

> `TaskRun` (interfaz, `tasks.svelte.ts`) es el espejo de `ProcessRecord` del lado de la UI —
> candidato a renombrarse a "Proceso" para que el código no use dos nombres para lo mismo. Su
> `onDone` no cruza el canal: vive solo del lado de la interfaz, no es parte del proceso en Rust.

## Guardas de perfil

**Guarda de perfil**:
Campo del perfil que restringe o advierte sobre lo que se puede hacer contra ese servidor, sin
alterar cómo se establece la conexión. Las tres frenan en momento y con fuerza distinta:

- **`environment`** — *advierte*: pinta el servidor en árbol, detalle, barra de consulta y
  pestañas, y exige una confirmación extra antes de mutar. Frena al usuario, no al servidor —
  se puede pasar por encima confirmando.
- **`read_only`** — *impide*: lo rechaza PostgreSQL, no la interfaz. No se puede pasar por
  encima desde la aplicación. Ver [ADR-0005](docs/adr/0005-read-only-como-flag-de-arranque.md).
- **`autocommit`** — *valor inicial*: no advierte ni impide nada, es el punto de partida de
  cada pestaña de consulta nueva y el usuario lo cambia cuando quiera.

No asumir que las tres se pueden saltar igual, ni que las tres son igual de duras: solo
`read_only` es una barrera real; `environment` es una advertencia salvable; `autocommit` no es
ni una cosa ni la otra.

## IPC

**IPC**:
Todo lo que la interfaz le pide a Rust pasa por `ui/src/lib/ipc/`. Ningún componente llama a
`invoke` por su cuenta — quien escriba UI nueva tiene que agregar la función ahí, en el módulo
del dominio que corresponda, y consumirla desde el componente. Un `invoke` fuera de `ipc/` es una
violación de esta regla, no una variante aceptable.

## Errores y el estado del vínculo

**Desconexión**:
No falló esta operación puntual: falló el vínculo con el servidor, y cualquier otra cosa que se
le pida hasta reconectar también va a fallar. El criterio para clasificar un error como
Desconexión y no como un error cualquiera es ese: ¿tiene sentido reintentar la misma operación
tal cual? Si no lo tiene, es Desconexión.
_Avoid_: error de conexión (impreciso: no dice si vale la pena reintentar).

**Servidor caído**:
Diagnóstico del árbol (`Row.down`) cuando un comando cualquiera falla por Desconexión. No
destruye nada del usuario — solo pinta la fila y ofrece reconectar — y se revierte solo apenas
algo vuelve a responder contra ese servidor.
_Eje que lo separa de "servidor desconectado"_: si destruye o no trabajo del usuario. Caído, no.
Ver [ADR-0006](docs/adr/0006-servidor-caido-y-desconectado-dos-flags.md).

**Servidor desconectado**:
Fin de la sesión contra ese servidor (`connected: false`). Destruye estado — cierra todas las
pestañas abiertas de ese servidor — y hoy solo ocurre por acción explícita del usuario
(`disconnect()`); "servidor caído" nunca escala solo a "servidor desconectado": no hay una regla
de ascenso definida por reintentos ni por tiempo.
_Eje que lo separa de "servidor caído"_: si destruye o no trabajo del usuario. Desconectado, sí.
Ver [ADR-0006](docs/adr/0006-servidor-caido-y-desconectado-dos-flags.md).
