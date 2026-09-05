# `read_only` rechaza el servidor entero, no una lista de operaciones

**Decisión**: un perfil con `read_only` agrega `-c default_transaction_read_only=on` a las
opciones de arranque de la conexión, en vez de chequear operación por operación en cada diálogo
de mutación.

**Razón**: quien rechaza la escritura es PostgreSQL, no la interfaz. Un diálogo de mutación
nuevo queda cubierto solo con existir, sin que nadie tenga que acordarse de agregarle el chequeo.

**Alternativa descartada**: chequeo por operación en cada diálogo. Da mejores mensajes de error y
más flexibilidad (por ejemplo, permitir alguna escritura puntual), pero es opt-in — se olvida, y
lo que se olvida en un diálogo de mutación nuevo queda sin protección.

**Consecuencia aceptada**: el error de una escritura bloqueada llega desde el servidor, con el
texto de PostgreSQL y no uno propio del proyecto. Volver al chequeo por operación exige tocar
cada diálogo de mutación existente, no solo un lugar central.
