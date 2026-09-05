# Inspector: región que convive, no pestaña

**Decisión**: el detalle del objeto elegido en el árbol vive en el inspector, una región a la
derecha que convive con cualquier panel y cualquier pestaña — no es exclusiva con nada, y la
controla su propio atajo (`Ctrl+I`), no el riel.

**Razón**: como pestaña ("Detalle"), mirar las columnas de una tabla mientras se escribía la
consulta que las usa costaba cambiar de pestaña —y perder de vista el editor— cada vez. El
inspector no compite por el mismo espacio que las pestañas: se pliega y se despliega aparte, y
sigue al árbol sin taparle nada al panel principal.

**Alternativa descartada**: dejarlo como pestaña más, junto a consulta/datos/ERD/comparación. Es
más simple de razonar —una sola lista de pestañas—, pero repite el problema que motivó a
Biblioteca ([ADR-0007](0007-navegacion-unica.md)): algo que se consulta *junto con* otra cosa, no
*en lugar de*, pierde si tiene que competir por el mismo lugar.

**Consecuencia**: el inspector comparte la clase CSS `.panel` con Explorador/Biblioteca/Procesos
—es presentación, no significa que sea uno de ellos—: es entrada propia del glosario, no un cuarto
panel del riel. `Ctrl+I` pliega y despliega el inspector sin tocar qué panel muestra el riel ni
qué pestaña está activa.
