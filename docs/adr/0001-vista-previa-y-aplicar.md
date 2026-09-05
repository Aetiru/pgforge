# Vista previa y aplicar como dos comandos separados

Toda mutación del núcleo se parte en dos: un comando que **genera el SQL** (o la línea de
`pg_dump`/`pg_restore`) de forma pura, y otro que lo **ejecuta**. Se pudo resolver con un solo
comando y un flag `dry_run`, pero eso deja la responsabilidad de no ejecutar en cada sitio de
llamada. Separar en dos comandos hace que lo que la interfaz muestra antes de aplicar sea
exactamente lo que el segundo comando va a correr —la misma función lo genera para los dos—, y
que la función generadora se pueda probar sin servidor ni ventana.
