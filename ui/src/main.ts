import { mount } from "svelte";
// Source Code Pro empaquetada: no viene instalada en Windows y la ventana no tiene red, así que se
// incluye en el bundle. Solo los pesos que la interfaz usa (normal, medium, semibold, bold); cada
// archivo declara todos los subconjuntos con su unicode-range y el navegador baja solo el que toca.
import "@fontsource/source-code-pro/400.css";
import "@fontsource/source-code-pro/500.css";
import "@fontsource/source-code-pro/600.css";
import "@fontsource/source-code-pro/700.css";
import "./app.css";
import App from "./App.svelte";
// Antes de montar: el tema se escribe en el documento al importar el módulo, así la primera pintura
// ya sale con los colores definitivos y no hay un destello claro al abrir la ventana en oscuro.
import "./lib/theme.svelte";

export default mount(App, { target: document.getElementById("app")! });
