# `ServerCaps`: predicados de capacidad calculados una vez, no chequeo de versión en cada sitio

Ninguna consulta va atada a una versión de PostgreSQL escrita a mano en el sitio de uso
(`if server_version >= 14`, por ejemplo). Al conectar se calcula una vez `ServerCaps`
(`pgforge-core::caps`), que expone predicados con nombre (`has_pg_stat_io()`,
`has_query_id()`, `has_reindex_concurrently()`, …) que cada módulo consulta para armar su SQL.
El chequeo inline es más directo de leer en el momento, pero dispersa el conocimiento de qué
versión trajo qué por todo el código; con `ServerCaps`, agregar un predicado nuevo es el único
lugar que hay que tocar al sumar soporte para una vista o columna que no está en todo el rango
soportado (PG 13 a 17).
