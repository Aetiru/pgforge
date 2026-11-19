import { ago, bytes, count, duration, percent } from "./format";
import { daysText, growthText, levelLabel, millions } from "./diagnose-view";
import type { Health } from "./health";
import type {
  AutovacuumRow,
  CacheReport,
  CheckpointReport,
  ConfigReview,
  ConnectionReport,
  Metrics,
  SequenceUsage,
  SpaceOverview,
  Wraparound,
} from "./ipc";

/**
 * Informe de salud: lo que el monitoreo mira, escrito para pasárselo a otra persona o adjuntarlo a
 * un ticket. Se arma en dos pasos —un modelo de secciones y dos escritores (Markdown y HTML)— para
 * que los dos archivos digan exactamente lo mismo.
 *
 * Solo entra lo que **pide algo**: una tabla de doscientas tablas «en orden» es lo que hace que
 * nadie lea un informe. Lo que no se pudo leer (permisos, extensión ausente) va en su propia
 * sección en vez de omitirse en silencio, porque «sin avisos» y «no se miró» no son lo mismo.
 */
export interface ReportInput {
  generatedAt: Date;
  server: string;
  database: string;
  health: Health | null;
  metrics: Metrics | null;
  connections: ConnectionReport | null;
  sequences: SequenceUsage[] | null;
  wraparound: Wraparound | null;
  autovacuum: AutovacuumRow[] | null;
  cache: CacheReport | null;
  space: SpaceOverview | null;
  checkpoints: CheckpointReport | null;
  config: ConfigReview | null;
  /** Qué parte no se pudo leer y por qué: `[«Secuencias», «permiso denegado»]`. */
  unavailable: [string, string][];
}

export interface Section {
  title: string;
  paragraphs: string[];
  bullets: string[];
  table?: { head: string[]; rows: string[][] };
}

const HEALTH_TEXT = {
  ok: "Todo en orden",
  warn: "Hay cosas para revisar",
  bad: "Requiere atención",
} as const;

function section(title: string, fill: Partial<Section> = {}): Section {
  return { title, paragraphs: [], bullets: [], ...fill };
}

export function buildReport(input: ReportInput): Section[] {
  const sections: Section[] = [];

  const verdict = input.health ? HEALTH_TEXT[input.health.level] : "Sin muestra del monitoreo";
  sections.push(
    section("Resumen", {
      paragraphs: [`${verdict}.`],
      bullets: input.health?.issues.map((issue) => issue.text) ?? [],
    }),
  );

  if (input.connections) {
    const c = input.connections;
    const usable = c.maxConnections - c.reserved;
    sections.push(
      section("Conexiones", {
        paragraphs: [
          `${count(c.total)} de ${count(usable)} disponibles (máximo ${count(c.maxConnections)}, ${count(c.reserved)} reservadas a superusuarios).`,
        ],
        table: c.groups.length
          ? {
              head: ["Aplicación", "Rol", "Dirección", "Total", "Inactivas", "En transacción"],
              rows: c.groups
                .slice(0, 10)
                .map((g) => [
                  g.application ?? "(sin permiso)",
                  g.user ?? "—",
                  g.address ?? "—",
                  count(g.total),
                  count(g.idle),
                  count(g.idleInTransaction),
                ]),
            }
          : undefined,
      }),
    );
  }

  const sequences = (input.sequences ?? []).filter((s) => s.level === "review" || s.level === "attention");
  if (sequences.length) {
    sections.push(
      section("Secuencias por agotarse", {
        table: {
          head: ["Secuencia", "Tipo", "Usado", "Quedan", "Se agota en"],
          rows: sequences.map((s) => [
            `${s.schema}.${s.name}`,
            s.dataType,
            percent(s.usedRatio),
            count(s.remaining),
            daysText(s.daysLeft),
          ]),
        },
      }),
    );
  }

  const wrap = input.wraparound;
  if (wrap) {
    const dbs = wrap.databases.filter((d) => d.level !== "ok");
    const tables = wrap.tables.filter((t) => t.level !== "ok");
    if (dbs.length || tables.length) {
      sections.push(
        section("Edad de las transacciones", {
          paragraphs: [
            `El autovacuum fuerza una limpieza a los ${millions(wrap.freezeMaxAge)}; a los 2.147 M el servidor deja de aceptar escrituras.`,
          ],
          bullets: [
            ...dbs.map((d) => `Base ${d.name}: ${millions(d.age)} (${levelLabel(d.level)})`),
            ...tables.map((t) => `${t.schema}.${t.table}: ${millions(t.age)} (${levelLabel(t.level)})`),
          ],
        }),
      );
    }
  }

  const stuck = (input.autovacuum ?? []).filter((r) => r.level === "review" || r.level === "attention");
  if (stuck.length) {
    sections.push(
      section("Autovacuum que no alcanza", {
        paragraphs: ["Tablas que ya pasaron su umbral y no se limpian, o con el autovacuum apagado."],
        table: {
          head: ["Tabla", "Filas muertas", "Umbral", "Última limpieza", "Estado"],
          rows: stuck.map((r) => [
            `${r.schema}.${r.table}`,
            count(r.deadTuples),
            count(Math.round(r.vacuumAt)),
            ago(r.lastAutovacuumSeconds),
            r.status === "disabled" ? "apagado" : "le toca",
          ]),
        },
      }),
    );
  }

  if (input.cache) {
    const rows = input.cache.tables.filter((t) => t.level !== "ok");
    if (rows.length) {
      sections.push(
        section("Caché", {
          paragraphs: ["Tablas que se leen del disco más de lo esperado (aciertos por debajo del 90 %)."],
          table: {
            head: ["Tabla", "Aciertos tabla", "Aciertos índices", "Lecturas a disco"],
            rows: rows.map((t) => [
              `${t.schema}.${t.table}`,
              percent(t.heapRatio),
              percent(t.indexRatio),
              count(t.heapRead + t.indexRead),
            ]),
          },
        }),
      );
    }
  }

  if (input.space) {
    const { map, growth } = input.space;
    const base = growth.find((g) => g.kind === "database");
    const bullets = [`Base ${map.database}: ${bytes(map.databaseBytes)}`];
    if (base?.forecast) bullets.push(`Crecimiento de la base: ${growthText(base.forecast.perDay)}`);
    sections.push(
      section("Espacio", {
        bullets,
        table: {
          head: ["Tabla", "Total", "Tabla", "Índices", "TOAST y otros", "Crece"],
          rows: map.entries.slice(0, 10).map((e) => {
            const g = growth.find((x) => x.kind === "table" && x.name === `${e.schema}.${e.table}`);
            return [
              `${e.schema}.${e.table}`,
              bytes(e.heapBytes + e.toastBytes + e.indexBytes),
              bytes(e.heapBytes),
              bytes(e.indexBytes),
              bytes(e.toastBytes),
              g?.forecast ? growthText(g.forecast.perDay) : "—",
            ];
          }),
        },
      }),
    );
  }

  if (input.checkpoints) {
    const c = input.checkpoints;
    sections.push(
      section("Checkpoints y WAL", {
        paragraphs: [
          `${count(c.timed)} programados y ${count(c.requested)} forzados` +
            (c.forcedRatio !== null ? ` (${percent(c.forcedRatio, 0)} forzados)` : "") +
            (c.resetSeconds !== null ? `, desde ${ago(c.resetSeconds)}.` : "."),
          ...(c.wal?.bytesPerSecond != null
            ? [`WAL: ${bytes(c.wal.bytesPerSecond)}/s de promedio.`]
            : []),
        ],
        bullets: c.advice,
      }),
    );
  }

  const findings = (input.config?.findings ?? []).filter((f) => f.level !== "info");
  if (findings.length) {
    sections.push(
      section("Configuración", {
        table: {
          head: ["Parámetro", "Hoy", "Sugerido", "Motivo"],
          rows: findings.map((f) => [f.name, f.current, f.suggested ?? "—", f.reason]),
        },
      }),
    );
  }

  if (input.unavailable.length) {
    sections.push(
      section("Lo que no se pudo leer", {
        bullets: input.unavailable.map(([what, why]) => `${what}: ${why}`),
      }),
    );
  }
  return sections;
}

function title(input: ReportInput): string {
  return `Informe de salud · ${input.server} · ${input.database}`;
}

function stamp(input: ReportInput): string {
  return `Generado el ${input.generatedAt.toLocaleString("es")}. Los números son de ese momento.`;
}

/** Una celda de Markdown no puede llevar `|` ni saltos de línea sin romper la tabla. */
const cell = (text: string) => text.replace(/\|/g, "\\|").replace(/\s*\n\s*/g, " ");

export function toMarkdown(input: ReportInput): string {
  const out = [`# ${title(input)}`, "", stamp(input), ""];
  for (const s of buildReport(input)) {
    out.push(`## ${s.title}`, "");
    for (const p of s.paragraphs) out.push(p, "");
    if (s.bullets.length) out.push(...s.bullets.map((b) => `- ${b}`), "");
    if (s.table) {
      out.push(`| ${s.table.head.map(cell).join(" | ")} |`);
      out.push(`| ${s.table.head.map(() => "---").join(" | ")} |`);
      for (const row of s.table.rows) out.push(`| ${row.map(cell).join(" | ")} |`);
      out.push("");
    }
  }
  return out.join("\n");
}

const esc = (text: string) =>
  text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** Un solo archivo, sin recursos externos: se adjunta o se abre donde sea. Claro y oscuro. */
export function toHtml(input: ReportInput): string {
  const body = buildReport(input)
    .map((s) => {
      const parts = [`<h2>${esc(s.title)}</h2>`];
      parts.push(...s.paragraphs.map((p) => `<p>${esc(p)}</p>`));
      if (s.bullets.length) parts.push(`<ul>${s.bullets.map((b) => `<li>${esc(b)}</li>`).join("")}</ul>`);
      if (s.table) {
        const head = s.table.head.map((h) => `<th>${esc(h)}</th>`).join("");
        const rows = s.table.rows
          .map((r) => `<tr>${r.map((c) => `<td>${esc(c)}</td>`).join("")}</tr>`)
          .join("");
        parts.push(`<div class="scroll"><table><thead><tr>${head}</tr></thead><tbody>${rows}</tbody></table></div>`);
      }
      return parts.join("\n");
    })
    .join("\n");
  return `<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${esc(title(input))}</title>
<style>
  :root { --bg: #fff; --fg: #1b2230; --muted: #5b6577; --line: #dfe3ea; }
  @media (prefers-color-scheme: dark) { :root { --bg: #141922; --fg: #e6e9ef; --muted: #9aa4b6; --line: #2b3342; } }
  body { background: var(--bg); color: var(--fg); font: 15px/1.5 system-ui, sans-serif; margin: 0 auto; max-width: 960px; padding: 24px 16px; }
  h1 { font-size: 22px; margin: 0 0 4px; }
  h2 { font-size: 17px; margin: 28px 0 8px; }
  .meta { color: var(--muted); margin: 0 0 8px; }
  .scroll { overflow-x: auto; }
  table { border-collapse: collapse; width: 100%; font-size: 13.5px; }
  th, td { border-bottom: 1px solid var(--line); padding: 6px 10px; text-align: left; vertical-align: top; }
  th { color: var(--muted); font-weight: 600; }
</style>
</head>
<body>
<h1>${esc(title(input))}</h1>
<p class="meta">${esc(stamp(input))}</p>
${body}
</body>
</html>
`;
}

/** Duración corta para el pie de pantalla: cuánto hace que se generó. */
export const generatedAgo = (input: ReportInput, now = new Date()) =>
  duration((now.getTime() - input.generatedAt.getTime()) / 1000);
