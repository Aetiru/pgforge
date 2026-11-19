/**
 * Plantillas listas para agregar con un clic.
 *
 * Una lista de abreviaturas vacía no enseña qué se puede esperar de ellas. Estas son las consultas
 * que se escriben una y otra vez —y las de diagnóstico, que nadie recuerda de memoria—, con huecos y
 * variables de verdad (`${tabla@tabla}`, `${n@numero:20}`…) para que se vea cómo se usan.
 *
 * Reglas que cumplen todas (las verifica el test): SQL válido de PostgreSQL 13 a 17 y nada de
 * lo que solo existe en las nuevas; ninguna ejecuta algo que destruya sin `WHERE`, y las que
 * escriben o borran llevan un comentario `-- cuidado:`; las abreviaturas son únicas, válidas, sin
 * avisos entre sí y distintas de las de fábrica.
 */
import type { SnippetLike } from "./snippet-check";

export interface LibraryItem {
  id: string;
  category: string;
  abbreviation: string;
  description: string;
  body: string;
}

const item = (
  category: string,
  abbreviation: string,
  description: string,
  body: string,
): LibraryItem => ({ id: `lib-${abbreviation}`, category, abbreviation, description, body });

export const LIBRARY: LibraryItem[] = [
  // ----------------------------------------------------------------------------- Consultas
  item(
    "Consultas",
    "ult",
    "Últimas N filas por fecha",
    `SELECT *
FROM \${tabla@tabla}
ORDER BY \${fecha@columna} DESC
LIMIT \${n@numero:20}`,
  ),
  item(
    "Consultas",
    "pag",
    "Paginar con LIMIT y OFFSET",
    `SELECT *
FROM \${tabla@tabla}
ORDER BY \${orden@columna}
LIMIT \${tam@numero:50} OFFSET \${desde@numero:0}`,
  ),
  item(
    "Consultas",
    "topg",
    "Primeras N filas de cada grupo",
    `SELECT *
FROM (
    SELECT t.*,
           row_number() OVER (PARTITION BY \${grupo@columna} ORDER BY \${orden@columna} DESC) AS pos
    FROM \${tabla@tabla} t
) x
WHERE x.pos <= \${n@numero:3}`,
  ),
  item(
    "Consultas",
    "txt",
    "Buscar un texto en una columna",
    `SELECT *
FROM \${tabla@tabla}
WHERE \${columna@columna} ILIKE '%\${texto:}%'`,
  ),
  item(
    "Consultas",
    "huer",
    "Filas sin relación en otra tabla",
    `SELECT a.*
FROM \${tabla_a@tabla} a
LEFT JOIN \${tabla_b@tabla} b ON b.\${fk@columna} = a.\${id@columna:id}
WHERE b.\${fk@columna} IS NULL`,
  ),
  item(
    "Consultas",
    "exis",
    "Filas que tienen alguna relacionada (EXISTS)",
    `SELECT *
FROM \${tabla@tabla} t
WHERE EXISTS (
    SELECT 1
    FROM \${otra@tabla} o
    WHERE o.\${fk@columna} = t.\${id@columna:id}
)`,
  ),
  item(
    "Consultas",
    "caso",
    "Columna calculada con CASE",
    `SELECT \${columna@columna},
       CASE
           WHEN \${columna@columna} > \${limite@numero:0} THEN \${si:'sí'}
           ELSE \${no:'no'}
       END AS \${alias:resultado}
FROM \${tabla@tabla}`,
  ),
  item(
    "Consultas",
    "coal",
    "Reemplazar nulos con COALESCE",
    `SELECT COALESCE(\${columna@columna}, \${valor:0}) AS \${alias:valor}
FROM \${tabla@tabla}`,
  ),
  item(
    "Consultas",
    "mes",
    "Filas de un mes",
    `SELECT *
FROM \${tabla@tabla}
WHERE \${fecha@columna} >= date_trunc('month', '\${mes@fecha}'::date)
  AND \${fecha@columna} <  date_trunc('month', '\${mes@fecha}'::date) + interval '1 month'`,
  ),

  // --------------------------------------------------------------------------------- Datos
  item(
    "Datos",
    "ups",
    "Insertar o actualizar (upsert)",
    `-- cuidado: pisa las filas que ya existan con esa clave
INSERT INTO \${tabla@tabla} (\${columnas:id, nombre})
VALUES (\${valores:1, 'valor'})
ON CONFLICT (\${clave@columna:id})
DO UPDATE SET \${columna@columna} = EXCLUDED.\${columna@columna}`,
  ),
  item(
    "Datos",
    "insel",
    "Insertar desde un SELECT",
    `INSERT INTO \${destino@tabla} (\${columnas:id, nombre})
SELECT \${columnas:id, nombre}
FROM \${origen@tabla}
WHERE \${condicion:true}`,
  ),
  item(
    "Datos",
    "updf",
    "UPDATE con valores de otra tabla",
    `-- cuidado: modifica filas; revisa el WHERE antes de ejecutar
UPDATE \${tabla@tabla} t
SET \${columna@columna} = o.\${valor@columna}
FROM \${otra@tabla} o
WHERE o.\${id@columna:id} = t.\${id@columna:id}
RETURNING t.*`,
  ),
  item(
    "Datos",
    "delu",
    "DELETE con USING y RETURNING",
    `-- cuidado: borra filas; comprueba antes con un SELECT del mismo WHERE
DELETE FROM \${tabla@tabla} t
USING \${otra@tabla} o
WHERE o.\${id@columna:id} = t.\${id@columna:id}
  AND \${condicion:o.activo = false}
RETURNING t.*`,
  ),
  item(
    "Datos",
    "copt",
    "Copiar una tabla (sin índices ni restricciones)",
    `-- no copia índices, claves ni valores por omisión
CREATE TABLE \${destino:copia} AS
SELECT *
FROM \${origen@tabla}
WHERE \${condicion:true}`,
  ),

  // ---------------------------------------------------------------------------- Diagnóstico
  item(
    "Diagnóstico",
    "dup",
    "Valores repetidos de una columna",
    `SELECT \${columna@columna}, count(*) AS veces
FROM \${tabla@tabla}
GROUP BY \${columna@columna}
HAVING count(*) > 1
ORDER BY veces DESC`,
  ),
  item(
    "Diagnóstico",
    "tam",
    "Tamaño de las tablas",
    `SELECT n.nspname AS esquema,
       c.relname AS tabla,
       pg_size_pretty(pg_total_relation_size(c.oid)) AS total,
       pg_size_pretty(pg_relation_size(c.oid)) AS tabla_sola,
       pg_size_pretty(pg_indexes_size(c.oid)) AS indices
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE c.relkind IN ('r', 'm')
  AND n.nspname NOT IN ('pg_catalog', 'information_schema')
ORDER BY pg_total_relation_size(c.oid) DESC
LIMIT \${n@numero:20}`,
  ),
  item(
    "Diagnóstico",
    "idxu",
    "Índices que nunca se usaron",
    `-- cuidado: los usos cuentan desde el último reinicio de las estadísticas
SELECT schemaname AS esquema,
       relname AS tabla,
       indexrelname AS indice,
       idx_scan AS usos,
       pg_size_pretty(pg_relation_size(indexrelid)) AS tamano
FROM pg_stat_user_indexes
WHERE idx_scan = 0
ORDER BY pg_relation_size(indexrelid) DESC`,
  ),
  item(
    "Diagnóstico",
    "act",
    "Sesiones activas",
    `SELECT pid,
       usename,
       application_name,
       state,
       now() - query_start AS duracion,
       left(query, 120) AS consulta
FROM pg_stat_activity
WHERE state <> 'idle'
  AND pid <> pg_backend_pid()
ORDER BY query_start`,
  ),
  item(
    "Diagnóstico",
    "blq",
    "Sesiones bloqueadas y quién las bloquea",
    `SELECT pid,
       pg_blocking_pids(pid) AS bloqueada_por,
       state,
       wait_event_type,
       wait_event,
       now() - query_start AS espera,
       left(query, 120) AS consulta
FROM pg_stat_activity
WHERE cardinality(pg_blocking_pids(pid)) > 0`,
  ),
  item(
    "Diagnóstico",
    "larg",
    "Consultas que llevan mucho tiempo",
    `SELECT pid,
       usename,
       now() - query_start AS duracion,
       state,
       left(query, 200) AS consulta
FROM pg_stat_activity
WHERE state = 'active'
  AND now() - query_start > interval '\${minutos@numero:5} minutes'
ORDER BY duracion DESC`,
  ),
  item(
    "Diagnóstico",
    "conx",
    "Conexiones por estado",
    `SELECT coalesce(state, 'sin permiso') AS estado,
       count(*) AS conexiones
FROM pg_stat_activity
WHERE backend_type = 'client backend'
GROUP BY 1
ORDER BY 2 DESC`,
  ),
  item(
    "Diagnóstico",
    "muer",
    "Tablas con más filas muertas",
    `SELECT schemaname AS esquema,
       relname AS tabla,
       n_live_tup,
       n_dead_tup,
       round(100.0 * n_dead_tup / nullif(n_live_tup + n_dead_tup, 0), 1) AS pct_muertas,
       last_autovacuum
FROM pg_stat_user_tables
ORDER BY n_dead_tup DESC
LIMIT \${n@numero:20}`,
  ),

  // ------------------------------------------------------------------------ Mantenimiento
  item(
    "Mantenimiento",
    "vac",
    "VACUUM ANALYZE de una tabla",
    `VACUUM (ANALYZE, VERBOSE) \${tabla@tabla}`,
  ),
  item(
    "Mantenimiento",
    "reidx",
    "Reconstruir un índice sin bloquear",
    `-- no puede correr dentro de una transacción
REINDEX INDEX CONCURRENTLY \${indice:nombre_del_indice}`,
  ),
  item(
    "Mantenimiento",
    "cidx",
    "Crear un índice sin bloquear",
    `-- no puede correr dentro de una transacción
CREATE INDEX CONCURRENTLY IF NOT EXISTS \${nombre:idx_tabla_columna}
    ON \${tabla@tabla} (\${columna@columna})`,
  ),
  item(
    "Mantenimiento",
    "matar",
    "Cerrar una sesión por su pid",
    `-- cuidado: cierra la sesión y revierte su transacción abierta
SELECT pg_terminate_backend(\${pid@numero})`,
  ),
  item(
    "Mantenimiento",
    "lck",
    "Ver los bloqueos que hay",
    `SELECT l.pid,
       l.locktype,
       l.mode,
       l.granted,
       c.relname AS relacion,
       a.usename,
       left(a.query, 120) AS consulta
FROM pg_locks l
LEFT JOIN pg_class c ON c.oid = l.relation
LEFT JOIN pg_stat_activity a ON a.pid = l.pid
WHERE l.pid <> pg_backend_pid()
ORDER BY l.granted, l.pid`,
  ),

  // ------------------------------------------------------------------------------ Estructura
  item(
    "Estructura",
    "cols",
    "Columnas de una tabla",
    `SELECT column_name, data_type, is_nullable, column_default
FROM information_schema.columns
WHERE table_schema = '\${esquema:public}'
  AND table_name = '\${tabla:nombre_de_la_tabla}'
ORDER BY ordinal_position`,
  ),
  item(
    "Estructura",
    "fks",
    "Claves foráneas de una tabla",
    `SELECT conname AS restriccion,
       conrelid::regclass AS tabla,
       pg_get_constraintdef(oid) AS definicion
FROM pg_constraint
WHERE contype = 'f'
  AND (conrelid = '\${tabla:esquema.tabla}'::regclass
       OR confrelid = '\${tabla:esquema.tabla}'::regclass)`,
  ),
  item(
    "Estructura",
    "coment",
    "Comentarios de las columnas de una tabla",
    `SELECT c.relname AS tabla,
       a.attname AS columna,
       d.description AS comentario
FROM pg_description d
JOIN pg_class c ON c.oid = d.objoid
JOIN pg_attribute a ON a.attrelid = d.objoid AND a.attnum = d.objsubid
WHERE c.relname = '\${tabla:nombre_de_la_tabla}'
ORDER BY a.attnum`,
  ),
];

/** Las categorías, en el orden en que aparecen en la biblioteca. */
export function libraryCategories(library: LibraryItem[] = LIBRARY): string[] {
  return [...new Set(library.map((entry) => entry.category))];
}

/** Las que todavía no están instaladas: se compara por abreviatura, sin distinguir mayúsculas. */
export function libraryAvailable(
  library: LibraryItem[],
  installed: Pick<SnippetLike, "abbreviation">[],
): LibraryItem[] {
  const taken = new Set(installed.map((s) => s.abbreviation.trim().toLowerCase()));
  return library.filter((entry) => !taken.has(entry.abbreviation.toLowerCase()));
}
