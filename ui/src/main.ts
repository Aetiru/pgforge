import { mount } from "svelte";
// Las fuentes van empaquetadas: no vienen instaladas en Windows y la ventana no tiene red, así que
// se incluyen en el bundle. IBM Plex Sans es la de la interfaz; las dos monoespaciadas son las
// elegibles para código y datos. Solo los pesos que se usan (normal, medium, semibold, bold); cada
// archivo declara todos los subconjuntos con su unicode-range y el navegador baja solo el que toca.
// Las dos monoespaciadas se importan siempre, aunque solo una esté elegida, para que cambiar de
// una a otra en `font.svelte.ts` no dependa de una carga de red que puede no estar.
import "@fontsource/ibm-plex-sans/400.css";
import "@fontsource/ibm-plex-sans/500.css";
import "@fontsource/ibm-plex-sans/600.css";
import "@fontsource/ibm-plex-sans/700.css";
import "@fontsource/source-code-pro/400.css";
import "@fontsource/source-code-pro/500.css";
import "@fontsource/source-code-pro/600.css";
import "@fontsource/source-code-pro/700.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "@fontsource/jetbrains-mono/600.css";
import "@fontsource/jetbrains-mono/700.css";
import "./app.css";
import App from "./App.svelte";
// Antes de montar: el tema y la fuente se escriben en el documento al importar el módulo, así la
// primera pintura ya sale definitiva y no hay destello con la anterior.
import "./lib/theme.svelte";
import "./lib/font.svelte";
import "./lib/sql-theme.svelte";

export default mount(App, { target: document.getElementById("app")! });
