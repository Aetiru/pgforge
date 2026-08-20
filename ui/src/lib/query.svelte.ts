import type { SQLNamespace } from "@codemirror/lang-sql";
import { save } from "@tauri-apps/plugin-dialog";
import { changesCatalog } from "./ddl-tags";
import { explorer } from "./explorer.svelte";
import { notify } from "./notify.svelte";
import { paging } from "./paging.svelte";
import { Tab, tabs } from "./tabs.svelte";
import {
  Channel,
  describeError,
  isCanceled,
  queryAutocommit,
  queryCancel,
  queryColumnTypes,
  queryClose,
  queryCommit,
  queryExplain,
  queryOpen,
  queryRollback,
  queryRun,
  queryTxStatus,
  schemaSnapshot,
  sqlReadFile,
  sqlWriteFile,
  type CoreError,
  type ExplainOptions,
  type Outcome,
  type Plan,
  type QueryEvent,
  type SchemaRelation,
  type TxStatus,
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

export type ResultView = "rows" | "plan" | "messages" | "history" | "saved";

/**
 * Lo que el editor necesita del catálogo: el árbol que espera `@codemirror/lang-sql` para completar
 * detrás de un punto, y la lista plana, que es con la que se resuelven las columnas del `FROM`
 * (ver `sql-complete.ts`). Sale todo de la misma consulta, así que viaja junto.
 */
export interface EditorSchema {
  namespace: SQLNamespace;
  relations: SchemaRelation[];
}

/**
 * Los nombres de una base, cacheados mientras dure la ventana.
 *
 * Dos pestañas contra la misma base comparten el resultado: la consulta al catálogo no es gratis y
 * abrir varias pestañas sobre la base en la que uno trabaja es lo normal.
 */
const namespaces = new Map<string, Promise<EditorSchema>>();

async function schemaFor(profileId: string, database: string): Promise<EditorSchema> {
  const key = `${profileId}:${database}`;
  const cached = namespaces.get(key);
  if (cached) return cached;

  const pending = schemaSnapshot(profileId, database).then((snapshot) => {
    const namespace: Record<string, Record<string, string[]>> = {};
    for (const schema of snapshot.schemas) namespace[schema] = {};
    for (const relation of snapshot.relations) {
      (namespace[relation.schema] ??= {})[relation.name] = relation.columns;
    }
    return { namespace: namespace as SQLNamespace, relations: snapshot.relations };
  });

  // Un fallo no se cachea: la próxima pestaña vuelve a intentarlo.
  pending.catch(() => namespaces.delete(key));
  namespaces.set(key, pending);
  return pending;
}

/**
 * Tira los nombres cacheados de una base. Después de un `CREATE TABLE` el autocompletado seguía
 * ofreciendo el catálogo de antes —sin la tabla recién creada— hasta cerrar la ventana.
 */
export function invalidateSchema(profileId: string, database: string) {
  namespaces.delete(`${profileId}:${database}`);
}

export class QueryTab extends Tab {
  readonly kind = "query" as const;

  /** Identificador de la conexión del lado del backend. */
  tabId = $state<string | null>(null);
  /** Nombres del esquema en el formato que espera `@codemirror/lang-sql`. */
  schema = $state.raw<SQLNamespace | undefined>(undefined);
  /** Los mismos nombres en plano, para completar las columnas del `FROM` sin calificar. */
  relations = $state.raw<SchemaRelation[]>([]);
  sql = $state("");
  opening = $state(true);
  running = $state(false);

  /** Arranca con el valor del perfil; el interruptor de la barra lo cambia solo para esta pestaña. */
  autocommit = $state(true);
  /** Lo dice el servidor después de cada ejecución, no se deduce del SQL escrito. */
  txStatus = $state<TxStatus>("idle");

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
  /** El SQL de la última ejecución, tal como se mandó. */
  ranSql = $state("");

  /**
   * Los tipos de las columnas del resultado, cuando se piden.
   *
   * No vienen con las filas: el resultado viaja como texto y el protocolo simple no trae los tipos
   * (ver `sql::exec`). Saberlos cuesta preparar la sentencia de nuevo en el servidor, así que se
   * pide solo con el interruptor encendido y no en cada ejecución.
   */
  showTypes = $state(false);
  columnTypes = $state.raw<string[] | null>(null);
  /** Cuál de los resultados se está mirando, cuando el script devolvió más de uno. */
  shown = $state(0);

  /** Dónde se guardó el texto, para que el siguiente Ctrl+S no vuelva a preguntar. */
  filePath = $state<string | null>(null);

  /**
   * De qué consulta guardada salió el texto, si salió de alguna.
   *
   * Se conserva para que volver a guardar reescriba esa y no cree una copia con otro nombre. Es
   * independiente de `filePath`: una cosa es un archivo del disco y la otra una consulta con nombre
   * adentro de la aplicación, y una pestaña puede tener las dos o ninguna.
   */
  savedId = $state<number | null>(null);
  /** El nombre con el que se guardó, para proponerlo la próxima vez. */
  savedName = $state<string | null>(null);

  /** Trae al editor una consulta guardada. */
  applySaved(saved: { id: number; name: string; sql: string }) {
    this.sql = saved.sql;
    this.savedId = saved.id;
    this.savedName = saved.name;
    this.title = saved.name;
    this.view = "rows";
  }

  /**
   * Se corrió DDL y todavía no se avisó al árbol ni al autocompletado. No es `$state`: no se dibuja
   * en ningún lado, solo decide si hay que releer el catálogo cuando la transacción cierra.
   */
  private catalogDirty = false;

  get result(): ResultSet | null {
    return this.results[this.shown] ?? null;
  }

  /**
   * Guarda el texto del editor en un archivo.
   *
   * Escribir el archivo es del lado de Rust por la misma razón que `erd_export_svg`: el contenido
   * lo tiene la interfaz y sumar el complemento de archivos por un caso costaba más que el comando.
   */
  async saveTo(path: string) {
    await sqlWriteFile(path, this.sql);
    this.filePath = path;
    // El nombre del archivo es lo que uno busca en la barra cuando tiene cuatro pestañas abiertas.
    const name = path.split(/[\\/]/).pop();
    if (name) this.title = name;
    this.log("info", `Guardado en ${path}.`);
  }

  /**
   * Vuelve a apuntar la pestaña a otra base del mismo servidor.
   *
   * La sesión se cierra y se abre de nuevo: no hay forma de cambiar de base sobre una conexión
   * abierta. El texto del editor queda intacto —es lo que uno quiere correr contra la otra base—,
   * pero los resultados no, porque ya no son de donde dice el encabezado.
   */
  async switchDatabase(database: string) {
    if (this.running || database === this.database) return;

    const previous = this.tabId;
    this.tabId = null;
    this.opening = true;
    this.results = [];
    this.plan = null;
    this.columnTypes = null;
    this.errorMark = null;
    this.ranSql = "";
    this.shown = 0;

    try {
      if (previous) await queryClose(previous);
      const opened = await queryOpen(this.profileId, database);
      this.tabId = opened.tabId;
      this.database = opened.database;
      this.autocommit = opened.autocommit;
      this.txStatus = opened.txStatus;
      this.messages = [];
      this.log("info", `Conectado a ${opened.database}.`);
      await this.loadSchema();
    } catch (error) {
      this.log("error", describeError(error));
      this.view = "messages";
    } finally {
      this.opening = false;
    }
  }

  /**
   * Pide los nombres para el autocompletado. No bloquea nada: la pestaña ya sirve para escribir y
   * ejecutar mientras el catálogo se consulta, y si falla lo único que se pierde es completar.
   */
  async loadSchema() {
    this.schema = undefined;
    this.relations = [];
    try {
      const loaded = await schemaFor(this.profileId, this.database);
      this.schema = loaded.namespace;
      this.relations = loaded.relations;
    } catch (error) {
      this.log("notice", `Sin autocompletado: ${describeError(error)}`);
    }
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

    // Lo último que se mandó, para poder exportarlo sin volver a ejecutarlo. La grilla tiene solo
    // las filas que entraron en el techo; exportar tiene que ir de nuevo al servidor con la consulta.
    this.ranSql = sql;
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

    // Se cuenta acá y no con el `seconds` del evento `completed` para que incluya la ida y vuelta
    // entera: es lo que esperó quien la lanzó, que es de lo que se trata el aviso.
    const startedAt = Date.now();
    let failed = false;

    try {
      // El techo de filas es el mismo que elige la grilla de datos: es la misma pregunta —cuántas
      // filas se traen de una— y una consulta sin `WHERE` sobre una tabla grande, con el techo del
      // núcleo, dejaba diez mil filas en memoria para mirar las primeras veinte.
      await queryRun(this.tabId, sql, channel, { maxRows: paging.size });
      if (this.showTypes) await this.loadColumnTypes();
    } catch (error) {
      failed = true;
      this.log("error", describeError(error));
      this.view = "messages";
    } finally {
      this.running = false;
      // Una consulta pesada se lanza y uno se va a otra pantalla, igual que con un `VACUUM`. Lo que
      // tardó poco no avisa: el umbral y el interruptor viven en `notify.svelte.ts`.
      void notify.queryEnded({
        server: explorer.profiles.find((profile) => profile.id === this.profileId)?.name ?? "",
        database: this.database,
        seconds: (Date.now() - startedAt) / 1000,
        failed: failed || this.messages.some((message) => message.tone === "error"),
      });
    }
  }

  async setShowTypes(on: boolean) {
    this.showTypes = on;
    if (on) await this.loadColumnTypes();
    else this.columnTypes = null;
  }

  /** Los tipos del último resultado. Solo tiene sentido con una sola sentencia: `PREPARE` no
   * acepta un script, y con varios resultados no habría a cuál pegarle los tipos. */
  async loadColumnTypes() {
    this.columnTypes = null;
    if (!this.tabId || this.results.length !== 1 || this.ranSql.trim() === "") return;

    try {
      const columns = await queryColumnTypes(this.tabId, this.ranSql);
      this.columnTypes = columns.map((column) => column.typeName);
    } catch {
      // Preparar puede fallar por motivos legítimos —un `SET`, un `CREATE`, un script—: sin tipos,
      // el encabezado queda como estaba y no se molesta al usuario con un error por algo opcional.
      this.columnTypes = null;
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
          if (changesCatalog(event.outcome.tag)) this.catalogDirty = true;
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

      case "transaction":
        this.txStatus = event.status;
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
        // Sin `await`: el resultado ya está en pantalla y releer el catálogo no debe hacerlo esperar.
        void this.syncCatalog();
        break;
    }
  }

  /**
   * Le avisa al árbol y al autocompletado que el catálogo cambió.
   *
   * Con una transacción abierta no se avisa todavía: el árbol lee por otra conexión y ahí el objeto
   * nuevo **no existe** —volvería a traer lo de antes y a dejarlo desaparecido hasta el próximo
   * refresco a mano—. La marca queda puesta y se resuelve al confirmar.
   */
  private async syncCatalog() {
    if (!this.catalogDirty || this.txStatus !== "idle") return;
    this.catalogDirty = false;

    invalidateSchema(this.profileId, this.database);
    void this.loadSchema();
    await explorer.refreshServer(this.profileId);
  }

  async cancel() {
    if (!this.tabId || !this.running) return;
    try {
      await queryCancel(this.tabId);
    } catch (error) {
      this.log("error", describeError(error));
    }
  }

  async commit() {
    await this.endTransaction(queryCommit, "Transacción confirmada.");
  }

  async rollback() {
    await this.endTransaction(queryRollback, "Transacción revertida.");
  }

  /**
   * Enciende o apaga el autocommit. Encenderlo con una transacción abierta no la confirma: el estado
   * que devuelve el backend sigue diciendo que hay algo pendiente, y la barra lo sigue mostrando.
   */
  async setAutocommit(enabled: boolean) {
    if (!this.tabId) return;
    this.autocommit = enabled;
    try {
      this.txStatus = await queryAutocommit(this.tabId, enabled);
      if (enabled && this.txStatus !== "idle") {
        this.log("notice", "Queda una transacción abierta: confirmala o revertila.");
      }
    } catch (error) {
      this.log("error", describeError(error));
    }
  }

  private async endTransaction(
    action: (tabId: string) => Promise<TxStatus>,
    done: string,
  ) {
    if (!this.tabId || this.running) return;
    try {
      this.txStatus = await action(this.tabId);
      this.log("info", done);
      // Recién ahora el DDL de la transacción existe para las otras conexiones. Un `ROLLBACK` pasa
      // por acá igual: descartar el cambio también deja al árbol pidiendo una relectura, porque lo
      // que se ve puede venir de una lectura hecha en el medio.
      void this.syncCatalog();
    } catch (error) {
      this.log("error", describeError(error));
      this.view = "messages";
      // Un `COMMIT` que falla igual termina la transacción: se vuelve a preguntar en vez de suponer.
      this.txStatus = await queryTxStatus(this.tabId).catch(() => this.txStatus);
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

/**
 * Suelta el vínculo de las pestañas con una consulta guardada que se borró.
 *
 * Sin esto la pestaña queda apuntando a un identificador que ya no existe: el próximo «Guardar»
 * intenta reescribir lo borrado, no toca ninguna fila y termina en un «la consulta guardada ya no
 * existe» sin salida —el usuario quería guardar, no reescribir—. Suelto el vínculo, ese mismo botón
 * la guarda de nuevo como una consulta nueva.
 */
export function forgetSaved(savedId: number) {
  for (const tab of tabs.all) {
    if (tab instanceof QueryTab && tab.savedId === savedId) {
      tab.savedId = null;
      // El nombre se conserva: sigue siendo el título de la pestaña y lo que se propone al guardar.
    }
  }
}

/**
 * Guarda el texto de la pestaña como archivo `.sql`.
 *
 * Con `askPath` apagado y una ruta ya elegida no vuelve a preguntar, que es lo que uno espera de
 * Ctrl+S mientras trabaja. Vive acá y no en `QueryPanel` porque el atajo también llega desde la
 * ventana, cuando el foco está afuera del editor.
 */
export async function saveQueryTab(tab: QueryTab, askPath: boolean) {
  try {
    let path = askPath ? null : tab.filePath;
    if (!path) {
      path = await save({
        title: "Guardar la consulta",
        defaultPath: tab.filePath ?? `${tab.title.replace(/[\\/:*?"<>|]/g, "_")}.sql`,
        filters: [{ name: "SQL", extensions: ["sql"] }],
      });
    }
    if (!path) return;
    await tab.saveTo(path);
  } catch (error) {
    tab.log("error", describeError(error));
    tab.view = "messages";
  }
}

/**
 * Abre archivos `.sql` en pestañas nuevas, una por archivo.
 *
 * Contra qué base corren no lo dice el archivo: lo decide quien lo abre, igual que una pestaña
 * vacía. Cada pestaña queda con su ruta puesta, así que el primer `Ctrl+S` guarda encima sin
 * preguntar, que es lo que uno espera de un archivo que abrió.
 */
export async function openSqlFiles(
  paths: string[],
  profileId: string,
  database: string,
): Promise<QueryTab | null> {
  let last: QueryTab | null = null;

  for (const path of paths) {
    const name = path.split(/[\\/]/).pop() ?? "consulta.sql";
    const tab = await openQuery(profileId, database, name);
    last = tab;
    try {
      tab.sql = await sqlReadFile(path);
      tab.filePath = path;
    } catch (error) {
      // La pestaña queda abierta y vacía con el motivo adentro: cerrarla sola escondería el error.
      tab.log("error", `No se pudo abrir ${path}: ${describeError(error)}`);
      tab.view = "messages";
    }
  }

  return last;
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
    tab.autocommit = opened.autocommit;
    tab.txStatus = opened.txStatus;
    tab.log("info", `Conectado a ${opened.database}.`);
    if (!opened.autocommit) {
      tab.log("info", "Autocommit apagado: cada ejecución abre una transacción.");
    }
  } catch (error) {
    tab.log("error", describeError(error));
    tab.view = "messages";
  } finally {
    tab.opening = false;
  }

  // Sin `await`: la pestaña ya sirve para escribir y ejecutar mientras el catálogo se consulta.
  void tab.loadSchema();

  return tab;
}
