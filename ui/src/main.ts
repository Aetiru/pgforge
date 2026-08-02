import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
// Antes de montar: el tema se escribe en el documento al importar el módulo, así la primera pintura
// ya sale con los colores definitivos y no hay un destello claro al abrir la ventana en oscuro.
import "./lib/theme.svelte";

export default mount(App, { target: document.getElementById("app")! });
