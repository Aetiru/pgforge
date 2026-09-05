# La clave del marcador ata a servidor, base y schema — no es portable como la consulta guardada

**Decisión**: la clave única de un marcador es `(profile_id, database, schema, name, kind)`.

**Razón de la asimetría con Consulta guardada**: una consulta guardada es texto que tiene sentido
correr contra cualquier base — `SELECT * FROM clientes` vale igual en dev que en prod. Un
marcador no es texto, es un puntero a un objeto concreto: `public.clientes` en dev y
`public.clientes` en prod son objetos distintos, y un marcador que no distinguiera servidor
apuntaría a los dos a la vez sin poder abrir ninguno con certeza.

**Alternativa descartada**: hacerlo portable solo por nombre, como `saved`. Se pierde la
precisión de "esta tabla en este servidor", que es justo el motivo de tener marcadores en el
árbol y no una lista de nombres sueltos.

**Consecuencia**: cambiar la clave rompe los marcadores existentes — cualquier cambio futuro a
qué la compone necesita migración del archivo SQLite, no solo del código.
