/**
 * La frontera con Rust.
 *
 * Sigue valiendo la regla de siempre: **este es el único lugar de la interfaz que habla con Rust**.
 * Ningún componente llama a `invoke` por su cuenta; todo pasa por acá, donde viven los tipos espejo
 * de los del núcleo y una función por comando. Lo único que cambió es que dejó de ser un archivo de
 * dos mil líneas que nadie leía entero: ahora es una carpeta con un módulo por dominio, y este
 * índice los reexporta para que quien importa siga escribiendo `from "./ipc"`.
 *
 * Al agregar un comando hay que tocar tres puntos, como antes: el módulo del núcleo, el comando en
 * `src-tauri/src/commands/` **más su registro en el `generate_handler!`**, y el módulo de acá que
 * corresponda al dominio.
 */

export * from "./core";
export * from "./servers";
export * from "./schema";
export * from "./compare";
export * from "./monitor";
export * from "./tasks";
export * from "./backup";
export * from "./query";
export * from "./snippets";
export * from "./data";
export * from "./ddl";
export * from "./objects";
export * from "./settings";
export * from "./security";
export * from "./update";
