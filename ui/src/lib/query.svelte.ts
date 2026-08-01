import type { SQLNamespace } from "@codemirror/lang-sql";
import { Tab, tabs } from "./tabs.svelte";
import {
  Channel,
  describeError,
  isCanceled,
  queryCancel,
  queryClose,
  queryExplain,
  queryOpen,
  queryRun,
  schemaSnapshot,
  type CoreError,
  type ExplainOptions,
  type Outcome,
  type Plan,
  type QueryEvent,
} from "./ipc";

export type MessageTone = "info" | "notice" | "error";

export interface Message {
  tone: MessageTone;
  text: string;
}

/** El resultado de una sentencia, junto con de cuál del script salió. */
export interface ResultSet {
  index: number;
  line: number;
  outcome: Outcome;
}

/** Dónde marcar el error dentro del texto del editor. */
export interface ErrorMark {
  /** Desplazamiento en caracteres desde el principio del script. */
  at: number;
  message: string;
}

export type ResultView = "rows" | "plan" | "messages" | "history";

/**
 * Los nombres de una base, cacheados mientras dure la ventana.
 *
 * Dos pestañas contra la misma base comparten el resultado: la consulta al catálogo no es gratis y
 * abrir varias pestañas sobre la base en la que uno trabaja es lo normal.
 */
const namespaces = new Map<string, Promise<SQLNamespace>>();

async function schemaFor(profileId: string, database: string): Promise<SQLNamespace> {
  const key = `${profileId}:${database}`;
  const cached = namespaces.get(key);
  if (cached) return cached;

  const pending = schemaSnapshot(profileId, database).then((snapshot) => {
    const namespace: Record<string, Record<string, string[]>> = {};
    for (const schema of snapshot.schemas) namespace[schema] = {};
    for (const relation of snapshot.relations) {
      (namespace[relation.schema] ??= {})[relation.name] = relation.columns;
    }
    return namespace as SQLNamespace;
  });

  // Un fallo no se cachea: la próxima pestaña vuelve a intentarlo.
  pending.catch(() => namespaces.delete(key));
  namespaces.set(key, pending);
  return pending;
}

export class QueryTab extends Tab {
  readonly kind = "query" as const;

  /** Identificador de la conexión del lado del backend. */
  tabId = $state<string | null>(null);
  /** Nombres del esquema en el formato que espera `@codemirror/lang-sql`. */
  schema = $state.raw<SQLNamespace | undefined>(undefined);
  sql = $state("");
  opening = $state(true);
  running = $state(false);

  /**
   * Van en `$state.raw` y no en `$state` porque se reemplazan enteros y nunca se mutan por dentro.
   * `$state` envuelve en un proxy cada objeto anidado: un resultado de diez mil filas por veinte
   * columnas serían doscientas mil envolturas, creadas para nada, justo antes de dibujar la grilla.
   */
  results = $state.raw<ResultSet[]>([]);
  plan = $state.raw<Plan | null>(null);

  messages = $state<Message[]>([]);
  errorMark = $state<ErrorMark | null>(null);
  view = $state<ResultView>("rows");
  /** Cuál de los resultados se está mirando, cuando el script devolvió más de uno. */
  shown = $state(0);

  get result(): ResultSet | null {
    return this.results[this.shown] ?? null;
  }

  log(tone: MessageTone, text: string) {
    this.messages.push({ tone, text });
  }

  override async dispose() {
    if (this.tabId) await queryClose(this.tabId);
  }

  /**
   * `base` es dónde empieza `sql` dentro del texto del editor. Al ejecutar una sola sentencia se le
   * manda al servidor solo ese fragmento, así que la posición que devuelve un error está referida
   * al fragmento: sin sumar el desplazamiento, la marca caería sobre la primera sentencia.
   */
  async run(sql: string, base = 0) {
    if (!this.tabId || this.running || sql.trim() === "") return;

    this.running = true;
    this.results = [];
    this.messages = [];
    this.errorMark = null;
    this.plan = null;
    this.shown = 0;
    this.view = "rows";

    const lines = new Map<number, number>();
    const channel = new Channel<QueryEvent>();
    channel.onmessage = (event) => this.apply(event, lines, base);

    try {
      await queryRun(this.tabId, sql, channel);
    } catch (error) {
      this.log("error", describeError(error));
      this.view = "messages";
    } finally {
      this.running = false;
    }
  }

  private apply(event: QueryEvent, lines: Map<number, number>, base: number) {
    switch (event.type) {
      case "started":
        lines.set(event.index, event.line);
        break;

      case "finished":
        // Reasignar y no `push`: con `$state.raw` el cambio se notifica al reemplazar el arreglo.
        this.results = [
          ...this.results,
          {
            index: event.index,
            line: lines.get(event.index) ?? 1,
            outcome: event.outcome,
          },
        ];
        if (event.outcome.kind === "command") {
          this.log("info", `${event.outcome.tag}: ${event.outcome.affected}`);
        } else if (event.outcome.truncated) {
          this.log(
            "notice",
            `Se recortó el resultado: hay ${event.outcome.rowCount} filas y se trajeron ` +
              `${event.outcome.rows.length}.`,
          );
        }
        // Mostrar el último resultado con filas es lo que uno espera al ejecutar un script que
        // termina en un SELECT.
        this.shown = this.results.reduce(
          (last, result, index) => (result.outcome.kind === "rows" ? index : last),
          0,
        );
        break;

      case "notice":
        this.log("notice", `${event.severity}: ${event.message}`);
        break;

      case "failed": {
        if (isCanceled(event.error)) {
          this.log("info", "Consulta cancelada.");
          break;
        }
        this.log("error", describeError(event.error));
        this.view = "messages";
        // La posición del servidor viene con base 1 y relativa a su sentencia.
        if (event.error.kind === "database" && event.error.position !== null) {
          this.errorMark = {
            at: base + event.offset + event.error.position - 1,
            message: event.error.message,
          };
        }
        break;
      }

      case "completed":
        if (event.executed > 1) {
          this.log("info", `${event.executed} sentencias en ${event.seconds.toFixed(3)} s.`);
        }
        break;
    }
  }

  async cancel() {
    if (!this.tabId || !this.running) return;
    try {
      await queryCancel(this.tabId);
    } catch (error) {
      this.log("error", describeError(error));
    }
  }

  async explain(sql: string, base: number, options: ExplainOptions) {
    if (!this.tabId || this.running || sql.trim() === "") return;

    this.running = true;
    this.errorMark = null;
    this.messages = [];

    try {
      this.plan = await queryExplain(this.tabId, sql, options);
      this.view = "plan";
    } catch (error) {
      this.log("error", describeError(error));
      this.view = "messages";

      // La posición ya viene descontado el `EXPLAIN (…)` que antepuso el núcleo.
      const failure = error as CoreError;
      if (failure.kind === "database" && failure.position !== null) {
        this.errorMark = { at: base + failure.position - 1, message: failure.message };
      }
    } finally {
      this.running = false;
    }
  }
}

/** Abre una pestaña de consulta contra una base y la deja seleccionada. */
export async function openQuery(
  profileId: string,
  database: string,
  title: string,
): Promise<QueryTab> {
  const tab = tabs.add(new QueryTab(profileId, database, title));

  try {
    const opened = await queryOpen(profileId, database);
    tab.tabId = opened.tabId;
    tab.log("info", `Conectado a ${opened.database}.`);
  } catch (error) {
    tab.log("error", describeError(error));
    tab.view = "messages";
  } finally {
    tab.opening = false;
  }

  // El autocompletado no bloquea: la pestaña ya sirve para escribir y ejecutar mientras el catálogo
  // se consulta. Si falla, se pierde el autocompletado y nada más.
  schemaFor(profileId, database)
    .then((namespace) => {
      tab.schema = namespace;
    })
    .catch((error) => tab.log("notice", `Sin autocompletado: ${describeError(error)}`));

  return tab;
}
