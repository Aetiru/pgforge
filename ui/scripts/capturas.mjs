// Capturas de las pantallas principales en claro, oscuro y oscuro con alto contraste.
//
// No compara píxeles: es una prueba de humo visual. Falla si alguna pantalla lanza una excepción y
// deja las imágenes en `ui/capturas/` para mirarlas. Contra una interfaz con datos inventados
// (`mock-tauri.js`), porque lo que se mira es el diseño, no el servidor.
//
//   pnpm ui:dev            # en otra terminal: la interfaz en http://localhost:1420
//   pnpm ui:capturas
//
// Variables: `PGFORGE_UI_URL` (otra dirección) y `CHROME_PATH` (un Chromium ya instalado; sin ella se
// usa el que descargó Playwright).
import { chromium } from "playwright-core";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const out = path.join(here, "..", "capturas");
const url = process.env.PGFORGE_UI_URL ?? "http://localhost:1420/";
const mock = fs.readFileSync(path.join(here, "mock-tauri.js"), "utf8");

const VARIANTS = [
  { name: "claro", theme: "light", contrast: false },
  { name: "oscuro", theme: "dark", contrast: false },
  { name: "oscuro-contraste", theme: "dark", contrast: true },
];

fs.mkdirSync(out, { recursive: true });
const browser = await chromium.launch({
  executablePath: process.env.CHROME_PATH || undefined,
  args: ["--no-sandbox"],
});
const errors = [];

async function open(variant) {
  const page = await browser.newPage({ viewport: { width: 1600, height: 900 } });
  page.on("pageerror", (error) => errors.push(`${variant.name}: ${error.message}`));
  await page.addInitScript(
    ({ theme, contrast }) => {
      localStorage.setItem("pgforge.theme", theme);
      if (contrast) localStorage.setItem("pgforge.contrast", "high");
    },
    variant,
  );
  await page.addInitScript({ content: mock });
  await page.goto(url);
  await page.waitForTimeout(900);
  return page;
}

async function connected(variant) {
  const page = await open(variant);
  await page.getByText("Producción ventas").first().dblclick();
  await page.waitForTimeout(600);
  return page;
}

async function expand(page, names) {
  for (const name of names) {
    await page.getByText(name, { exact: true }).first().click();
    await page.keyboard.press("ArrowRight");
    await page.waitForTimeout(250);
  }
}

for (const variant of VARIANTS) {
  const shot = (page, name) => page.screenshot({ path: path.join(out, `${variant.name}-${name}.png`) });

  // Árbol con producción conectado: marco de entorno, pestañas, ruta.
  let page = await connected(variant);
  await expand(page, ["ventas", "Esquemas", "public", "Tablas"]);
  await page.getByText("pedidos", { exact: true }).first().click();
  await page.waitForTimeout(500);
  await page.getByRole("tab", { name: /Dependencias/ }).click();
  await page.waitForTimeout(300);
  await shot(page, "detalle-dependencias");
  await page.getByRole("tab", { name: /Estadísticas/ }).click();
  await page.waitForTimeout(300);
  await shot(page, "detalle-estadisticas");

  // Consulta con resultados, y su plan.
  await page.getByRole("button", { name: "Nueva consulta" }).first().click();
  await page.waitForTimeout(700);
  await page.locator(".cm-content").first().click();
  await page.keyboard.type("SELECT id, cliente, fecha, total, estado FROM pedidos ORDER BY id;");
  await page.keyboard.press("Control+Enter");
  await page.waitForTimeout(900);
  await shot(page, "consulta");
  await page.getByLabel("Explicar", { exact: true }).first().click();
  await page.waitForTimeout(900);
  await shot(page, "plan");
  await page.close();

  // Monitoreo: sesiones con detalle y línea de tiempo.
  page = await connected(variant);
  await page.getByText("ventas", { exact: true }).first().click();
  await page.getByRole("button", { name: "Monitoreo" }).first().click();
  await page.waitForTimeout(1200);
  await shot(page, "sesiones");
  await page.locator("tbody tr").nth(4).click();
  await page.getByRole("tab", { name: "Línea de tiempo" }).click();
  await page.waitForTimeout(400);
  await shot(page, "sesiones-linea");
  await page.close();

  // Consultas lentas, con la sentencia que trae parámetros elegida.
  page = await connected(variant);
  await page.getByText("ventas", { exact: true }).first().click();
  await page.getByRole("button", { name: "Monitoreo" }).first().click();
  await page.waitForTimeout(600);
  await page.getByRole("tab", { name: /Consultas lentas/ }).click();
  await page.waitForTimeout(500);
  await page.locator('[role="row"]').filter({ hasText: "cliente_id = $1" }).first().click();
  await shot(page, "consultas-lentas");
  await page.close();

  // Diagrama, alejado hasta el modo mapa.
  page = await connected(variant);
  await expand(page, ["ventas", "Esquemas", "public"]);
  await page.getByText("public", { exact: true }).first().click();
  await page.getByTitle(/Dibuja las tablas/).first().click();
  await page.waitForTimeout(900);
  for (let i = 0; i < 3; i++) await page.getByTitle("Alejar").click();
  await page.waitForTimeout(400);
  await shot(page, "diagrama-mapa");
  await page.close();

  // Primera vez: sin servidores configurados.
  page = await browser.newPage({ viewport: { width: 1600, height: 900 } });
  page.on("pageerror", (error) => errors.push(`${variant.name}: ${error.message}`));
  await page.addInitScript(
    ({ theme, contrast }) => {
      localStorage.setItem("pgforge.theme", theme);
      if (contrast) localStorage.setItem("pgforge.contrast", "high");
    },
    variant,
  );
  await page.addInitScript({
    content: `${mock}\nwindow.__MOCK.list_profiles = () => []; window.__MOCK.list_groups = () => []; window.__MOCK.connected_servers = () => [];`,
  });
  await page.goto(url);
  await page.waitForTimeout(900);
  await shot(page, "primer-uso");
  await page.close();

  // El diálogo de nuevo servidor.
  page = await open(variant);
  await page.locator('button[title*="uevo"]').first().click();
  await page.waitForTimeout(400);
  await page.getByRole("button", { name: "Producción" }).click();
  await shot(page, "dialogo-conexion");
  await page.close();

  // Preferencias, con los colores del SQL.
  page = await open(variant);
  await page.getByLabel(/Preferencias/).first().click();
  await page.waitForTimeout(400);
  await shot(page, "preferencias");
  await page.close();
}

await browser.close();

if (errors.length > 0) {
  console.error(`Hubo ${errors.length} excepciones en la interfaz:\n${errors.join("\n")}`);
  process.exit(1);
}
console.log(`Listo: ${VARIANTS.length * 11} capturas en ${out}`);
