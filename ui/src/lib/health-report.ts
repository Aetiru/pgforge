import { i18n, t } from "./i18n.svelte";
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

const healthText = (level: "ok" | "warn" | "bad") =>
  ({
    ok: t("Todo en orden"),
    warn: t("Hay cosas para revisar"),
    bad: t("Requiere atención"),
  })[level];

function section(title: string, fill: Partial<Section> = {}): Section {
  return { title, paragraphs: [], bullets: [], ...fill };
}

export function buildReport(input: ReportInput): Section[] {
  const sections: Section[] = [];

  const verdict = input.health ? healthText(input.health.level) : t("Sin muestra del monitoreo");
  sections.push(
    section(t("Resumen"), {
      paragraphs: [t("{verdict}.", { verdict })],
      bullets: input.health?.issues.map((issue) => issue.text) ?? [],
    }),
  );

  if (input.connections) {
    const c = input.connections;
    const usable = c.maxConnections - c.reserved;
    sections.push(
      section(t("Conexiones"), {
        paragraphs: [
          t("{total} de {usable} disponibles (máximo {max}, {reserved} reservadas a superusuarios).", {
            total: count(c.total),
            usable: count(usable),
            max: count(c.maxConnections),
            reserved: count(c.reserved),
          }),
        ],
        table: c.groups.length
          ? {
              head: [t("Aplicación"), t("Rol"), t("Dirección"), t("Total"), t("Inactivas"), t("En transacción")],
              rows: c.groups
                .slice(0, 10)
                .map((g) => [
                  g.application ?? t("(sin permiso)"),
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
      section(t("Secuencias por agotarse"), {
        table: {
          head: [t("Secuencia"), t("Tipo"), t("Usado"), t("Quedan"), t("Se agota en")],
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
    const tables = wrap.tables.filter((tb) => tb.level !== "ok");
    if (dbs.length || tables.length) {
      sections.push(
        section(t("Edad de las transacciones"), {
          paragraphs: [
            t("El autovacuum fuerza una limpieza a los {age}; a los 2.147 M el servidor deja de aceptar escrituras.", { age: millions(wrap.freezeMaxAge) }),
          ],
          bullets: [
            ...dbs.map((d) => t("Base {name}: {age} ({level})", { name: d.name, age: millions(d.age), level: levelLabel(d.level) })),
            ...tables.map((tb) => `${tb.schema}.${tb.table}: ${millions(tb.age)} (${levelLabel(tb.level)})`),
          ],
        }),
      );
    }
  }

  const stuck = (input.autovacuum ?? []).filter((r) => r.level === "review" || r.level === "attention");
  if (stuck.length) {
    sections.push(
      section(t("Autovacuum que no alcanza"), {
        paragraphs: [t("Tablas que ya pasaron su umbral y no se limpian, o con el autovacuum apagado.")],
        table: {
          head: [t("Tabla"), t("Filas muertas"), t("Umbral"), t("Última limpieza"), t("Estado")],
          rows: stuck.map((r) => [
            `${r.schema}.${r.table}`,
            count(r.deadTuples),
            count(Math.round(r.vacuumAt)),
            ago(r.lastAutovacuumSeconds),
            r.status === "disabled" ? t("apagado") : t("le toca"),
          ]),
        },
      }),
    );
  }

  if (input.cache) {
    const rows = input.cache.tables.filter((tb) => tb.level !== "ok");
    if (rows.length) {
      sections.push(
        section(t("Caché"), {
          paragraphs: [t("Tablas que se leen del disco más de lo esperado (aciertos por debajo del 90 %).")],
          table: {
            head: [t("Tabla"), t("Aciertos tabla"), t("Aciertos índices"), t("Lecturas a disco")],
            rows: rows.map((tb) => [
              `${tb.schema}.${tb.table}`,
              percent(tb.heapRatio),
              percent(tb.indexRatio),
              count(tb.heapRead + tb.indexRead),
            ]),
          },
        }),
      );
    }
  }

  if (input.space) {
    const { map, growth } = input.space;
    const base = growth.find((g) => g.kind === "database");
    const bullets = [t("Base {name}: {size}", { name: map.database, size: bytes(map.databaseBytes) })];
    if (base?.forecast) bullets.push(t("Crecimiento de la base: {growth}", { growth: growthText(base.forecast.perDay) }));
    sections.push(
      section(t("Espacio"), {
        bullets,
        table: {
          head: [t("Tabla"), t("Total"), t("Tabla"), t("Índices"), t("TOAST y otros"), t("Crece")],
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
      section(t("Checkpoints y WAL"), {
        paragraphs: [
          t("{timed} programados y {requested} forzados", {
            timed: count(c.timed),
            requested: count(c.requested),
          }) +
            (c.forcedRatio !== null
              ? " " + t("({ratio} forzados)", { ratio: percent(c.forcedRatio, 0) })
              : "") +
            (c.resetSeconds !== null ? t(", desde {since}.", { since: ago(c.resetSeconds) }) : "."),
          ...(c.wal?.bytesPerSecond != null
            ? [t("WAL: {rate}/s de promedio.", { rate: bytes(c.wal.bytesPerSecond) })]
            : []),
        ],
        bullets: c.advice,
      }),
    );
  }

  const findings = (input.config?.findings ?? []).filter((f) => f.level !== "info");
  if (findings.length) {
    sections.push(
      section(t("Configuración"), {
        table: {
          head: [t("Parámetro"), t("Hoy"), t("Sugerido"), t("Motivo")],
          rows: findings.map((f) => [f.name, f.current, f.suggested ?? "—", f.reason]),
        },
      }),
    );
  }

  if (input.unavailable.length) {
    sections.push(
      section(t("Lo que no se pudo leer"), {
        bullets: input.unavailable.map(([what, why]) => `${what}: ${why}`),
      }),
    );
  }
  return sections;
}

function title(input: ReportInput): string {
  return t("Informe de salud · {server} · {database}", { server: input.server, database: input.database });
}

function stamp(input: ReportInput): string {
  return t("Generado el {date}. Los números son de ese momento.", {
    date: input.generatedAt.toLocaleString(i18n.current),
  });
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
<html lang="${i18n.current}">
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
