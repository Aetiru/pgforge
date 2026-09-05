# El núcleo no depende de Tauri; la CLI existe para forzarlo

`pgforge-core` no tiene ninguna dependencia de Tauri ni de la UI; `src-tauri` solo traduce
argumentos y delega. Se pudo meter lógica directo en los comandos de Tauri —más rápido al
empezar—, pero eso mezcla el núcleo con el shell de escritorio y nada garantiza que siga
funcionando sin ventana. `pgforge-cli` existe justamente para eso: si una funcionalidad solo se
puede ejercitar desde la ventana, es señal de que quedó en el lugar equivocado. Los tests
ejercitan el núcleo directamente, nunca vía Tauri.
