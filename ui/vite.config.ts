import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";

// El host solo se define cuando se desarrolla contra un dispositivo remoto.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte(), tailwindcss()],

  // Tauri ya muestra sus propios errores en la terminal; que Vite no los tape.
  clearScreen: false,

  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },

  envPrefix: ["VITE_", "TAURI_ENV_*"],

  build: {
    // Solo corre sobre el WebView del sistema, no sobre navegadores viejos.
    target: "chrome105",
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },

  // Lo que se prueba acá es lógica pura —formatos, mapeo de errores, qué cambio sale de un
  // formulario—, no componentes montados: por eso alcanza con el entorno de Node y no hace falta
  // un DOM simulado.
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
