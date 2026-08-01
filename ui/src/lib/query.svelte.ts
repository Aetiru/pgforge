import type { SQLNamespace } from "@codemirror/lang-sql";
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

let sequence = 0;

/**
 * Los nombres de una base, cacheados mientras dure la ventana.
 *
 * Dos pestañas contra la misma base comparten el resultado: la consulta al catálogo no es gratis y
 * abrir varias pestañas sobre la base en la que uno está trabajando es lo normal.
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

/**
 * Una pestaña de consulta.
 *
 * `tabId` es el identificador que devuelve el backend y que representa la conexión; `key` es local
 * y existe desde antes, porque la pestaña aparece en la interfaz apenas se la abre y la conexión
 * puede tardar o fallar.
 */
export class QueryTab {
  readonly key = `consulta-${++sequence}`;
  readonly profileId: string;
  readonly database: string;
  readonly title: string;

  tabId = $state<string | null>(null);
  /** Nombres del esquema en el formato que espera `@codemirror/lang-sql`. */
  schema = $state.raw<SQLNamespace | undefined>(undefined);
  sql = $state("");
  opening = $state(true);
  running = $state(false);

  /**
   * Los tres van en `$state.raw` y no en `$state` porque se reemplazan enteros y nunca se mutan por
   * dentro. `$state` envuelve en un proxy cada objeto anidado: un resultado de diez mil filas por
   * veinte columnas serían doscientas mil envolturas, creadas para nada, justo antes de dibujar la
   * grilla.
   */
  results = $state.raw<ResultSet[]>([]);
  plan = $state.raw<Plan | null>(null);

  messages = $state<Message[]>([]);
  errorMark = $state<ErrorMark | null>(null);
  view = $state<ResultView>("rows");
  /** Cuál de los resultados se está mirando, cuando el script devolvió más de uno. */
  shown = $state(0);

  constructor(profileId: string, database: string, title: string) {
    this.profileId = profileId;
    this.database = database;
    this.title = title;
  }

  get result(): ResultSet | null {
    return this.results[this.shown] ?? null;
  }

  log(tone: MessageTone, text: string) {
    this.messages.push({ tone, text });
  }
}

class QueryTabs {
  tabs = $state<QueryTab[]>([]);
  /** `null` significa que se está mirando el detalle del objeto, no una consulta. */
  active = $state<string | null>(null);

  get current(): QueryTab | null {
    return this.tabs.find((tab) => tab.key === this.active) ?? null;
  }

  find(key: string): QueryTab | null {
    return this.tabs.find((tab) => tab.key === key) ?? null;
  }

  /** Abre una pestaña contra una base y la deja seleccionada. */
  async open(profileId: string, database: string, title: string): Promise<QueryTab> {
    const tab = new QueryTab(profileId, database, title);
    this.tabs.push(tab);
    this.active = tab.key;

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

    // El autocompletado no bloquea: la pestaña ya sirve para escribir y ejecutar mientras el
    // catálogo se consulta. Si falla, se pierde el autocompletado y nada más.
    schemaFor(profileId, database)
      .then((namespace) => {
        tab.schema = namespace;
      })
      .catch((error) => tab.log("notice", `Sin autocompletado: ${describeError(error)}`));

    return tab;
  }

  async close(key: string) {
    const tab = this.find(key);
    if (!tab) return;

    this.tabs = this.tabs.filter((item) => item.key !== key);
    if (this.active === key) {
      this.active = this.tabs.at(-1)?.key ?? null;
    }

    // La conexión se suelta aunque la consulta siga corriendo: el servidor la corta al cerrarse.
    if (tab.tabId) {
      await queryClose(tab.tabId).catch(() => {});
    }
  }

  /** Cierra las pestañas de un servidor que se desconectó; su conexión ya no existe. */
  async closeFor(profileId: string) {
    for (const tab of this.tabs.filter((item) => item.profileId === profileId)) {
      await this.close(tab.key);
    }
  }

  /**
   * `base` es dónde empieza `sql` dentro del texto del editor. Al ejecutar una sola sentencia se le
   * manda al servidor solo ese fragmento, así que la posición que devuelve un error está referida
   * al fragmento: sin sumar el desplazamiento, la marca caería sobre la primera sentencia.
   */
  async run(tab: QueryTab, sql: string, base = 0) {
    if (!tab.tabId || tab.running || sql.trim() === "") return;

    tab.running = true;
    tab.results = [];
    tab.messages = [];
    tab.errorMark = null;
    tab.plan = null;
    tab.shown = 0;
    tab.view = "rows";

    const lines = new Map<number, number>();
    const channel = new Channel<QueryEvent>();
    channel.onmessage = (event) => this.apply(tab, event, lines, base);

    try {
      await queryRun(tab.tabId, sql, channel);
    } catch (error) {
      tab.log("error", describeError(error));
      tab.view = "messages";
    } finally {
      tab.running = false;
    }
  }

  private apply(tab: QueryTab, event: QueryEvent, lines: Map<number, number>, base: number) {
    switch (event.type) {
      case "started":
        lines.set(event.index, event.line);
        break;

      case "finished":
        // Reasignar y no `push`: con `$state.raw` el cambio se notifica al reemplazar el arreglo.
        tab.results = [
          ...tab.results,
          {
            index: event.index,
            line: lines.get(event.index) ?? 1,
            outcome: event.outcome,
          },
        ];
        if (event.outcome.kind === "command") {
          tab.log("info", `${event.outcome.tag}: ${event.outcome.affected}`);
        } else if (event.outcome.truncated) {
          tab.log(
            "notice",
            `Se recortó el resultado: hay ${event.outcome.rowCount} filas y se trajeron ` +
              `${event.outcome.rows.length}.`,
          );
        }
        // Mostrar el último resultado con filas es lo que uno espera al ejecutar un script que
        // termina en un SELECT.
        tab.shown = tab.results.reduce(
          (last, result, index) => (result.outcome.kind === "rows" ? index : last),
          0,
        );
        break;

      case "notice":
        tab.log("notice", `${event.severity}: ${event.message}`);
        break;

      case "failed": {
        if (isCanceled(event.error)) {
          tab.log("info", "Consulta cancelada.");
          break;
        }
        tab.log("error", describeError(event.error));
        tab.view = "messages";
        // La posición del servidor viene con base 1 y relativa a su sentencia.
        if (event.error.kind === "database" && event.error.position !== null) {
          tab.errorMark = {
            at: base + event.offset + event.error.position - 1,
            message: event.error.message,
          };
        }
        break;
      }

      case "completed":
        if (event.executed > 1) {
          tab.log("info", `${event.executed} sentencias en ${event.seconds.toFixed(3)} s.`);
        }
        break;
    }
  }

  async cancel(tab: QueryTab) {
    if (!tab.tabId || !tab.running) return;
    try {
      await queryCancel(tab.tabId);
    } catch (error) {
      tab.log("error", describeError(error));
    }
  }

  async explain(tab: QueryTab, sql: string, base: number, options: ExplainOptions) {
    if (!tab.tabId || tab.running || sql.trim() === "") return;

    tab.running = true;
    tab.errorMark = null;
    tab.messages = [];

    try {
      tab.plan = await queryExplain(tab.tabId, sql, options);
      tab.view = "plan";
    } catch (error) {
      tab.log("error", describeError(error));
      tab.view = "messages";

      // La posición ya viene descontado el `EXPLAIN (…)` que antepuso el núcleo.
      const failure = error as CoreError;
      if (failure.kind === "database" && failure.position !== null) {
        tab.errorMark = {
          at: base + failure.position - 1,
          message: failure.message,
        };
      }
    } finally {
      tab.running = false;
    }
  }
}

export const queries = new QueryTabs();
