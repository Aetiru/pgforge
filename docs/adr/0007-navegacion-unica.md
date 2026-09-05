# Riel con paneles reemplaza la barra de vistas superior

**Decisión**: una sola navegación excluyente decide qué se ve. El riel de la izquierda alterna
entre tres paneles —Explorador, Biblioteca, Procesos—, y todo lo que corre contra un servidor
—consulta, datos, ERD, comparación, monitoreo, configuración— es una pestaña del panel principal.
El criterio que separa uno de otro: **lo que corre contra un servidor es pestaña; lo que no, es
panel.**

**Razón**: antes había dos navegaciones superpuestas — una barra de vistas excluyentes arriba que
al cambiar se llevaba puestos el árbol y las pestañas, y adentro de una de ellas la barra de
pestañas de consulta. Mirar el dashboard mientras corría una consulta era irse, mirar y volver a
buscar dónde estaba uno. Monitoreo y configuración pasaron a ser pestañas —tienen un servidor
adentro, igual que una consulta—, y con eso lo único que queda excluyente es qué panel ocupa el
lateral.

Historial y Consultas guardadas siguen la misma regla, leída al revés: no corren contra nada por
sí solas —son una lista para elegir qué ejecutar—, así que salieron de adentro de la pestaña de
consulta (antes solo se veían ahí, y para repetir lo de ayer había que abrir una consulta primero
y elegir contra qué base, que era justo el dato que se quería sacar de la lista) y pasaron a un
panel propio, Biblioteca.

**Alternativa descartada**: mantener las vistas de arriba y agregarles pestañas propias, una barra
por vista. Eso conserva las dos navegaciones y el problema de origen: cambiar de vista para mirar
otra cosa sigue tapando lo que se estaba mirando.

**Consecuencia**: un panel no tiene pestañas propias ni estado por servidor — es Explorador,
Biblioteca o Procesos, y punto; lo que antes vivía "dentro de una vista" (el `<select>` de servidor
de Monitoreo y Configuración) ahora sale de dónde está parado el usuario (`contextServer`,
`App.svelte`), igual que abrir una consulta nueva. Aparte de los tres paneles, Anclados queda fijo
al pie del lateral, ajeno a cuál esté elegido — no es una alternativa al riel, es una cuarta cosa
con otra regla: permanencia, no elección.
