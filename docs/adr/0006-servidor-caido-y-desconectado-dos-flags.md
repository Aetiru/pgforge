# "Servidor caído" y "servidor desconectado" son dos flags, no uno

**Decisión**: `Row.down` (servidor caído) y `connected: false` (servidor desconectado) son dos
señales separadas, aunque las dos digan "este servidor no responde".

**Razón**: un corte de tres segundos no debe costarle al usuario una consulta a medio escribir.
`Row.down` solo pinta el árbol y ofrece reconectar; bajar `connected` cierra todas las pestañas
de ese servidor (efecto de `App.svelte`). Si fueran un solo flag, cualquier falla transitoria por
Desconexión tendría que elegir entre no avisar nada o cerrar pestañas de golpe.

**Alternativa descartada**: un solo flag. Más simple de razonar, pero acopla el diagnóstico
("¿responde?") a la destrucción de estado ("cerrá todo lo que tenías abierto ahí").

**Consecuencia**: hay dos consumidores que ya asumen la diferencia (`markDown`/`markUp` del
árbol, y el cierre de pestañas de `App.svelte`); unificarlos ahora los rompe a los dos.

**Sin regla de ascenso definida**: hoy "caído" nunca escalona solo a "desconectado" — no hay
umbral de reintentos ni de tiempo que lo dispare. Pasar a desconectado es siempre una acción
explícita del usuario (`disconnect()`, `ipcDisconnect`); un servidor puede quedar marcado como
caído indefinidamente hasta que alguien lo reconecte a mano o lo desconecte a mano.
