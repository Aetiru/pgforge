import type { SQLNamespace } from "@codemirror/lang-sql";
import { pickSave } from "./ipc";
import { confirmAffected, confirmMutation, confirmScript, isReadOnly } from "./access.svelte";
import { CAP, countQuery } from "./affected-count";
import { buildChanges, editedRows, planEdit, withEdit, type EditPlan } from "./result-edit";
import { isWatchable } from "./watch-guard";
import { defaultTimeout } from "./query-timeout";
import { BIG_TABLE_ROWS, sizeTarget, withLimit } from "./select-size";
import { describeWait, type WaitText } from "./wait-text";
import { fillVars, findVars, parseRemembered, unmapper, varNames } from "./query-vars";

/** El `LIMIT` que se ofrece al avisar de una tabla enorme. */
const SAFE_LIMIT = 1000;

/** Últimos valores de las variables `{{nombre}}`, por nombre: preferencia de la aplicación. */
const VARS_KEY = "pgforge.queryvars";
import { changesCatalog } from "./ddl-tags";
import { autoFormat } from "./editor.svelte";
import { formatSql } from "./format-options.svelte";
import { versions } from "./editor/versions.svelte";
import { explorer } from "./explorer.svelte";
import { oneLine } from "./format";
import { notify } from "./notify.svelte";
import { paging } from "./paging.svelte";
import type { EditorAction } from "./editor/actions";
import { cursorInfo, type CursorInfo } from "./cursor-info";
import { renamedScriptPath } from "./script-name";
import { Tab, tabs } from "./tabs.svelte";
import {
  Channel,
  describeError,
  isCanceled,
  isDisconnected,
  reportServerDown,
  queryAutocommit,
  queryCancel,
  dataApply,
  dataOpen,
  dataPreview,
  queryColumnTypes,
  queryCount,
  queryEstimatedRows,
  queryTimeout,
  queryWaitInfo,
  queryEditSource,
  type PreviewStatement,
  queryClose,
  queryCommit,
  queryExplain,
  queryOpen,
  queryRehearse,
  queryRollback,
  queryRun,
  queryTxStatus,
  schemaSnapshot,
  scriptNewName,
  scriptRead,
  scriptRename,
  scriptWrite,
  sqlReadFile,
  sqlWriteFile,
  type CoreError,
  type ExplainOptions,
  type Outcome,
  type Plan,
  type QueryEvent,
  type RehearsalReport,
  type ForeignKeyRef,
  type SchemaRelation,
  type TxStatus,
} from "./ipc";

export type MessageTone = "info" | "notice" | "error";

/** Un error de PostgreSQL con sus partes, para dibujarlo como tarjeta en vez de una línea. */
export interface MessageError {
  code: string;
  message: string;
  detail: string | null;
  hint: string | null;
  /** Línea del editor donde falló (base 1), si el servidor dio una posición. */
  line: number | null;
  /** Texto de esa línea, recortado, para ver de qué se trata sin ir al editor. */
  lineText: string | null;
  /** Desplazamiento en caracteres dentro del editor: lo que usa «Ir a la línea». */
  at: number | null;
}

export interface Message {
  tone: MessageTone;
  text: string;
  error?: MessageError;
}

/** El resultado de una sentencia, junto con de cuál del script salió. */
export interface ResultSet {
  index: number;
  line: number;
  /** Dónde empieza la sentencia dentro del script, en caracteres. Con esto, exportar el resultado
   *  elegido recupera su texto exacto aunque no sea el primero. */
  offset: number;
  outcome: Outcome;
}

/** Dónde marcar el error dentro del texto del editor. */
export interface ErrorMark {
  /** Desplazamiento en caracteres desde el principio del script. */
  at: number;
  message: string;
}

export type ResultView = "rows" | "plan" | "messages" | "history" | "saved";

/** Un resultado anclado: sobrevive a que se corra otra cosa en la pestaña. */
export interface PinnedResult {
  id: string;
  /** El SQL que lo generó, recortado para el encabezado del panel. */
  label: string;
  /** El SQL entero, para el `title` del panel. */
  sql: string;
  outcome: Outcome;
  /** Los tipos de columna, si se habían pedido al anclar. */
  columnTypes: string[] | null;
}

/**
 * Lo que el editor necesita del catálogo: el árbol que espera `@codemirror/lang-sql` para completar
 * detrás de un punto, y la lista plana, que es con la que se resuelven las columnas del `FROM`
 * (ver `sql-complete.ts`). Sale todo de la misma consulta, así que viaja junto.
 */
export interface EditorSchema {
  namespace: SQLNamespace;
  relations: SchemaRelation[];
  foreignKeys: ForeignKeyRef[];
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
      // `lang-sql` solo necesita el nombre pelado detrás de `tabla.`; el tipo y el comentario son
      // para el hover, no para esta lista.
      (namespace[relation.schema] ??= {})[relation.name] = relation.columns.map((c) => c.name);
    }
    return {
      namespace: namespace as SQLNamespace,
      relations: snapshot.relations,
      foreignKeys: snapshot.foreignKeys ?? [],
    };
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
  /** Claves foráneas de la base, para proponer el `ON` de un `JOIN`. */
  foreignKeys = $state.raw<ForeignKeyRef[]>([]);
  sql = $state("");
  opening = $state(true);
  running = $state(false);
  /** Desde cuándo corre lo que corre (ms); `null` si no hay nada en curso. Lo lee el cronómetro. */
  runStartedAt = $state<number | null>(null);

  /**
   * El texto tal como quedó la última vez que se guardó en un archivo o con nombre, o `null` si
   * nunca. Un script de la biblioteca no cuenta: se vuelca solo a su `.sql` al cambiar de pestaña,
   * así que no hay nada «sin guardar» que avisar ahí.
   */
  baseline = $state<string | null>(null);

  /** Hay un archivo o una consulta con nombre que ya no coincide con lo escrito. */
  get dirty(): boolean {
    return (
      (this.filePath !== null || this.savedId !== null) &&
      this.baseline !== null &&
      this.sql !== this.baseline
    );
  }

  /** Arranca con el valor del perfil; el interruptor de la barra lo cambia solo para esta pestaña. */
  autocommit = $state(true);

  /** Último ensayo de esta pestaña, o el error que lo impidió. Se muestra en un diálogo. */
  rehearsal = $state<RehearsalReport | null>(null);
  rehearsalError = $state<string | null>(null);
  rehearsing = $state(false);
  rehearsalOpen = $state(false);
  /** Lo dice el servidor después de cada ejecución, no se deduce del SQL escrito. */
  txStatus = $state<TxStatus>("idle");
  /** Desde cuándo está abierta la transacción (ms); `null` sin transacción. Lo lee la franja. */
  txSince = $state<number | null>(null);
  /** Sentencias y filas tocadas desde que se abrió: lo que se pierde si se revierte. */
  txStatements = $state(0);
  txRows = $state(0);

  /**
   * Único lugar que asigna `txStatus`: el contador y el reloj de la franja nacen y mueren con la
   * transacción, así que no pueden quedar desfasados de lo que dice el servidor.
   */
  applyTx(status: TxStatus) {
    if (status === "idle") {
      this.txSince = null;
      this.txStatements = 0;
      this.txRows = 0;
    } else if (this.txSince === null) {
      this.txSince = Date.now();
    }
    this.txStatus = status;
  }

  /**
   * Van en `$state.raw` y no en `$state` porque se reemplazan enteros y nunca se mutan por dentro.
   * `$state` envuelve en un proxy cada objeto anidado: un resultado de diez mil filas por veinte
   * columnas serían doscientas mil envolturas, creadas para nada, justo antes de dibujar la grilla.
   */
  results = $state.raw<ResultSet[]>([]);
  plan = $state.raw<Plan | null>(null);
  /**
   * Resultados anclados: a diferencia de `results`, no se tocan en `run()` — es justamente lo que
   * tiene que sobrevivir a la ejecución siguiente. Se pierden al cerrar la pestaña, como el resto
   * de su estado.
   */
  pinned = $state.raw<PinnedResult[]>([]);

  messages = $state<Message[]>([]);
  errorMark = $state<ErrorMark | null>(null);
  view = $state<ResultView>("rows");
  /** El SQL de la última ejecución, tal como se mandó. */
  ranSql = $state("");

  /**
   * Los tipos de las columnas del resultado, cuando se piden.
   *
   * No vienen con las filas: el resultado viaja como texto y el protocolo simple no trae los tipos
   * (ver `sql::exec`). Saberlos cuesta preparar la sentencia de nuevo en el servidor —una vuelta
   * más, sin ejecutar nada—, y aun así viene **encendido**: al leer un resultado, si esa columna es
   * un `numeric` o un `text` cambia lo que dice, y averiguarlo con el interruptor apagado obligaba
   * a acordarse de que existía. El interruptor queda para apagarlo, no para prenderlo.
   */
  showTypes = $state(true);
  columnTypes = $state.raw<string[] | null>(null);
  /** Cuál de los resultados se está mirando, cuando el script devolvió más de uno. */
  shown = $state(0);
  /**
   * Si se está mirando un anclado en vez de uno en vivo: el `id` de cuál. Un resultado anclado
   * funciona como una pestaña más de la tira —se elige igual que `shown` elige uno en vivo—, así que
   * los dos son mutuamente excluyentes y no dos cosas que conviven en pantalla.
   */
  pinnedShown = $state<string | null>(null);

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

  /**
   * Su script en la biblioteca fija, si tiene uno.
   *
   * Independiente de `filePath`, con la misma lógica que ya separa a `filePath` de `savedId`:
   * `filePath` es «guardé esto en otro lado con Ctrl+S», y esto es el `.sql` que la aplicación
   * mantiene sola en `<config>/scripts/<conexión>/`, con su propio ciclo de autoguardado y de
   * renombre. Una pestaña abierta con `openSqlFiles` no tiene `scriptPath` —ya tiene su `filePath`,
   * que es justo el archivo que el usuario eligió—; una abierta con `openQuery` sí, reservado desde
   * el arranque; y una abierta con `openScript` también, apuntando al script que se vino a editar.
   */
  scriptPath = $state<string | null>(null);
  /**
   * Si `scriptPath` ya existe en disco. El nombre se reserva al abrir la pestaña, pero el archivo
   * recién se crea con el primer contenido no vacío (ver `flushScript`): sin esta bandera, cada
   * pestaña que se abre y se cierra sin escribir nada dejaría un `.sql` vacío en la biblioteca.
   * `openScript` la enciende de entrada: ahí el archivo ya existe, es el que se vino a abrir.
   */
  scriptCreated = false;
  private scriptSaveTimer: ReturnType<typeof setTimeout> | undefined;

  /**
   * Se incrementa para pedirle a la pestaña que formatee, desde un lugar que no tiene el editor de
   * CodeMirror a mano —la paleta de comandos—. `QueryPanel` lo mira y dispara el mismo camino que
   * el botón y el atajo.
   */
  formatRequest = $state(0);
  /** Lo mismo para abrir el diálogo de opciones del formateador y el de versiones anteriores desde
   *  la paleta: el diálogo vive en `QueryPanel`, que es quien escucha estos contadores. */
  formatOptionsRequest = $state(0);
  versionsRequest = $state(0);

  /**
   * Cursor y scroll del editor al salir de la pestaña.
   *
   * `{#key}` en `App.svelte` destruye y vuelve a montar el panel entero al cambiar de pestaña, así
   * que el `EditorView` de CodeMirror nace de cero cada vez; sin guardar esto acá, volver a una
   * consulta que ya se había abierto dejaba siempre el cursor en cero y el scroll arriba del todo.
   * No es `$state`: nadie dibuja con esto, solo lo lee `SqlEditor` al nacer.
   */
  editorSelection: { anchor: number; head: number } | null = null;
  /** Lo mismo, para la barra de estado: sí es reactivo, `editorSelection` no. */
  cursor = $state<CursorInfo>({ line: 1, column: 1, selected: 0 });
  /** Posición del documento que quedaba arriba del todo, no el `scrollTop` en píxeles —ver `ondetach`
   *  de `SqlEditor`. */
  editorTopPos: number | null = null;

  requestFormat() {
    this.formatRequest += 1;
  }

  /**
   * Un pedido de la paleta al editor de esta pestaña (expandir el `*`, renombrar un alias, una
   * transformación de texto…). Es un contador junto al nombre de lo pedido, como `formatRequest`:
   * dos pedidos iguales seguidos tienen que dispararse los dos, y el valor solo no cambiaría.
   */
  editorRequest = $state<{ action: EditorAction | null; n: number }>({ action: null, n: 0 });

  requestEditor(action: EditorAction) {
    this.editorRequest = { action, n: this.editorRequest.n + 1 };
  }

  requestFormatOptions() {
    this.formatOptionsRequest += 1;
  }

  requestVersions() {
    this.versionsRequest += 1;
  }

  /** Trae al editor una consulta guardada. */
  applySaved(saved: { id: number; name: string; sql: string }) {
    this.sql = saved.sql;
    this.baseline = saved.sql;
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

  /** Ancla el resultado en vivo como una pestaña más de la tira, que no se pisa con el próximo run. */
  pin(sql: string, outcome: Outcome, columnTypes: string[] | null) {
    this.pinned = [
      ...this.pinned,
      { id: crypto.randomUUID(), sql, label: oneLine(sql, 60), outcome, columnTypes },
    ];
  }

  /** Saca un ancla. Si era la que se estaba mirando, vuelve a la pestaña en vivo. */
  unpin(id: string) {
    this.pinned = this.pinned.filter((item) => item.id !== id);
    if (this.pinnedShown === id) this.pinnedShown = null;
  }

  /** Elige una pestaña en vivo (una de `results`). */
  showLive(index: number) {
    this.shown = index;
    this.pinnedShown = null;
  }

  /** Elige una pestaña anclada. */
  showPinned(id: string) {
    this.pinnedShown = id;
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
    this.baseline = this.sql;
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
    // Lo anclado quedó de la base anterior: seguir mostrándolo confundiría, sin nada que avise que
    // ya no es de acá.
    this.pinned = [];
    this.pinnedShown = null;

    try {
      if (previous) await queryClose(previous);
      const opened = await queryOpen(this.profileId, database);
      this.tabId = opened.tabId;
      this.database = opened.database;
      this.autocommit = opened.autocommit;
      this.applyTx(opened.txStatus);
      this.messages = [];
      this.log("info", `Conectado a ${opened.database}.`);
      if (this.timeoutMs !== null) await this.setTimeoutMs(this.timeoutMs);
      await this.loadSchema();
    } catch (error) {
      this.log("error", describeError(error));
      this.view = "messages";
    } finally {
      this.opening = false;
    }
  }

  /**
   * Abre una sesión nueva después de que el servidor se cayó y volvió. La anterior murió con la
   * conexión, y con ella lo que tuviera: una transacción abierta, las tablas temporales, los `SET`.
   * El texto de la pestaña, el resultado y lo anclado quedan como estaban.
   */
  async reopenSession() {
    if (this.running) return;
    this.stopWatch();
    const hadTransaction = this.txStatus !== "idle";
    const previous = this.tabId;
    this.opening = true;
    try {
      // La anterior ya no responde: cerrarla es para soltar lo que quedó del lado de Rust.
      if (previous) await queryClose(previous).catch(() => {});
      const opened = await queryOpen(this.profileId, this.database);
      this.tabId = opened.tabId;
      this.applyTx(opened.txStatus);
      if (opened.autocommit !== this.autocommit) await this.setAutocommit(this.autocommit);
      // El tiempo máximo era de la sesión que murió: se vuelve a poner en la nueva.
      if (this.timeoutMs !== null) await this.setTimeoutMs(this.timeoutMs);
      this.log(
        "notice",
        hadTransaction
          ? "Conectado de nuevo. La transacción que estaba abierta se perdió: nada de lo que hizo quedó confirmado."
          : "Conectado de nuevo.",
      );
    } catch (error) {
      this.log("error", describeError(error));
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
    this.foreignKeys = [];
    try {
      const loaded = await schemaFor(this.profileId, this.database);
      this.schema = loaded.namespace;
      this.relations = loaded.relations;
      this.foreignKeys = loaded.foreignKeys;
    } catch (error) {
      this.log("notice", `Sin autocompletado: ${describeError(error)}`);
    }
  }

  log(tone: MessageTone, text: string, error?: MessageError) {
    this.messages.push({ tone, text, error });
  }

  override async dispose() {
    this.stopWatch();
    this.stopWaitWatch();
    // Última copia del texto: es lo que se recupera si la pestaña se cerró sin querer.
    versions.noteClose(this);
    // Cerrar la pestaña guarda y no borra el script: que sobreviva es todo el punto de tenerlo.
    await this.flushScript();
    if (this.tabId) await queryClose(this.tabId);
  }

  override async flush() {
    await this.flushScript();
  }

  /**
   * Vuelca `sql` al script pendiente, si hay uno y tiene algo escrito.
   *
   * Sin contenido no escribe nada: es lo que evita que una pestaña abierta y cerrada sin tocar el
   * editor deje un `.sql` vacío en la biblioteca. Que falle no es un error del usuario —el mismo
   * texto sigue en el editor, y el próximo disparador lo vuelve a intentar—, así que se anota en el
   * registro de la pestaña y no interrumpe nada.
   */
  async flushScript() {
    clearTimeout(this.scriptSaveTimer);
    this.scriptSaveTimer = undefined;
    if (!this.scriptPath || this.sql.trim() === "") return;
    try {
      await scriptWrite(this.scriptPath, this.sql);
      this.scriptCreated = true;
    } catch (error) {
      this.log("error", `No se pudo guardar el script: ${describeError(error)}`);
    }
  }

  /**
   * Debounce de ~1 s sin cambios, uno de los tres disparadores del autoguardado (los otros dos son
   * perder el foco de la ventana y cambiar de pestaña activa, ver `App.svelte` y `Tabs.activate`).
   * `QueryPanel` lo llama en un `$effect` que mira `sql`.
   */
  scheduleScriptSave() {
    if (!this.scriptPath) return;
    clearTimeout(this.scriptSaveTimer);
    this.scriptSaveTimer = setTimeout(() => void this.flushScript(), 1000);
  }

  /**
   * Pregunta el valor de `askSize` cuando un `SELECT` sin límite va contra una tabla enorme de
   * producción. Devuelve el SQL a ejecutar —con o sin `LIMIT`— o `null` si se canceló. Cualquier
   * fallo al estimar deja pasar la sentencia tal cual: avisar es una ayuda, no una condición.
   */
  askSize: ((table: string, rows: number, limit: number) => Promise<"limit" | "run" | "cancel">) | null = null;

  private async checkSize(sql: string): Promise<string | null> {
    const profile = explorer.profiles.find((item) => item.id === this.profileId);
    if (profile?.environment !== "prod" || !this.tabId || !this.askSize) return sql;

    const target = sizeTarget(sql);
    if (!target) return sql;

    let rows: number | null = null;
    try {
      rows = await queryEstimatedRows(this.tabId, target.schema, target.table);
    } catch {
      return sql;
    }
    if (rows === null || rows < BIG_TABLE_ROWS) return sql;

    const name = target.schema ? `${target.schema}.${target.table}` : target.table;
    const choice = await this.askSize(name, rows, SAFE_LIMIT);
    if (choice === "cancel") return null;
    return choice === "limit" ? withLimit(sql, SAFE_LIMIT) : sql;
  }

  /**
   * La confirmación de antes de ejecutar. Un `UPDATE` o `DELETE` simple se cuenta primero y la
   * pregunta lleva el número; si no se puede contar —forma rara, error, cancelación del conteo— se
   * cae a la confirmación de siempre.
   */
  private async confirmRun(sql: string): Promise<boolean> {
    const plan = this.tabId ? countQuery(sql) : null;
    if (!plan || !this.tabId) return confirmScript(this.profileId, sql);

    this.running = true;
    this.runStartedAt = Date.now();
    let rows: number | null = null;
    let canceled = false;
    try {
      rows = await queryCount(this.tabId, plan.sql);
    } catch (error) {
      // Cancelar el conteo es cancelar la ejecución: no se sigue como si nada. Cualquier otro fallo
      // —contar es una ayuda— deja a la sentencia con su confirmación habitual.
      canceled = isCanceled(error);
    } finally {
      this.running = false;
      this.runStartedAt = null;
    }
    if (canceled) return false;
    if (rows === null) return confirmScript(this.profileId, sql);
    return confirmAffected(this.profileId, rows, rows > CAP, sql);
  }

  /**
   * Pregunta los valores de las variables `{{nombre}}` del texto. Lo pone el panel (es un diálogo);
   * sin él una variable no se puede resolver y la consulta no corre.
   */
  askVars: ((variables: { name: string; default: string }[]) => Promise<Record<string, string> | null>) | null =
    null;

  /** Lleva una posición del texto enviado al escrito, para que la marca del error caiga bien. */
  private runUnmap: (position: number) => number = (position) => position;

  /**
   * Reemplaza las variables por lo que se escribió. `null` si se canceló. Va antes de la
   * confirmación de producción: lo que se confirma tiene que ser lo que se va a ejecutar.
   */
  private async bindVars(
    sql: string,
  ): Promise<{ sql: string; unmap: (position: number) => number } | null> {
    const uses = findVars(sql);
    const identity = (position: number) => position;
    if (uses.length === 0) return { sql, unmap: identity };

    if (!this.askVars) {
      this.log("error", "La consulta tiene variables {{nombre}} y no hay dónde pedir sus valores.");
      return null;
    }

    let remembered: Record<string, string> = {};
    try {
      remembered = parseRemembered(localStorage.getItem(VARS_KEY));
    } catch {
      // Sin `localStorage` no se recuerda nada: se pregunta con los campos vacíos.
    }

    const values = await this.askVars(
      varNames(uses).map((name) => ({ name, default: remembered[name] ?? "" })),
    );
    if (values === null) return null;

    try {
      localStorage.setItem(VARS_KEY, JSON.stringify({ ...remembered, ...values }));
    } catch {
      // Recordar es una comodidad; no recordar no impide ejecutar.
    }
    return { sql: fillVars(sql, uses, values), unmap: unmapper(uses, values) };
  }

  /**
   * `base` es dónde empieza `sql` dentro del texto del editor. Al ejecutar una sola sentencia se le
   * manda al servidor solo ese fragmento, así que la posición que devuelve un error está referida
   * al fragmento: sin sumar el desplazamiento, la marca caería sobre la primera sentencia.
   */
  async run(sql: string, base = 0) {
    if (!this.tabId || this.running || sql.trim() === "") return;
    // Lo que se está por correr es lo que uno querría poder recuperar si sale mal.
    versions.noteRun(this);

    const bound = await this.bindVars(sql);
    if (bound === null) return;
    sql = bound.sql;
    this.runUnmap = bound.unmap;

    const sized = await this.checkSize(sql);
    if (sized === null) return;
    sql = sized;

    // En producción, un `DROP`, un `TRUNCATE` o un `DELETE`/`UPDATE` sin `WHERE` piden escribir el
    // nombre del servidor. El resto del script corre sin preguntar, como siempre.
    if (!(await this.confirmRun(sql))) return;

    // Lo último que se mandó, para poder exportarlo sin volver a ejecutarlo. La grilla tiene solo
    // las filas que entraron en el techo; exportar tiene que ir de nuevo al servidor con la consulta.
    this.ranSql = sql;
    // Una ejecución nueva reemplaza el resultado: lo que se editaba ya no es lo que se mira.
    this.editPlan = null;
    this.edits = new Map();
    this.editError = null;
    this.running = true;
    this.runStartedAt = Date.now();
    this.startWaitWatch();
    this.results = [];
    this.messages = [];
    this.errorMark = null;
    this.plan = null;
    this.shown = 0;
    // Ejecutar algo nuevo siempre lleva a mirarlo: lo anclado sigue en la tira, para volver después.
    this.pinnedShown = null;
    this.view = "rows";

    const starts = new Map<number, { line: number; offset: number }>();
    const channel = new Channel<QueryEvent>();
    channel.onmessage = (event) => this.apply(event, starts, base);

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
      if (isDisconnected(error)) reportServerDown(this.profileId);
      this.log("error", describeError(error));
      this.view = "messages";
    } finally {
      this.stopWaitWatch();
      this.running = false;
      this.runStartedAt = null;
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

  // --- Qué espera la consulta ------------------------------------------------------------------

  /** Lo que el servidor dice que espera la consulta en curso, o `null`. */
  waiting = $state.raw<WaitText | null>(null);
  private waitTimer: ReturnType<typeof setInterval> | null = null;
  private waitDelay: ReturnType<typeof setTimeout> | null = null;

  /**
   * Empieza a preguntar qué espera la consulta. No antes de unos segundos: una consulta que termina
   * enseguida no merece ni una pregunta, y la que tarda es justo la que lo necesita. Se llama al
   * arrancar una ejecución y se corta sola en `stopWaitWatch`.
   */
  private startWaitWatch() {
    this.stopWaitWatch();
    this.waitDelay = setTimeout(() => {
      const ask = async () => {
        if (!this.tabId || !this.running) return;
        try {
          this.waiting = describeWait(await queryWaitInfo(this.tabId));
        } catch {
          // Preguntar es una ayuda: si falla, la consulta sigue sin el detalle.
          this.waiting = null;
        }
      };
      void ask();
      this.waitTimer = setInterval(() => void ask(), 2000);
    }, 3000);
  }

  private stopWaitWatch() {
    if (this.waitDelay !== null) clearTimeout(this.waitDelay);
    if (this.waitTimer !== null) clearInterval(this.waitTimer);
    this.waitDelay = null;
    this.waitTimer = null;
    this.waiting = null;
  }

  // --- Tiempo máximo ---------------------------------------------------------------------------

  /** Tiempo máximo por sentencia: `null` el del perfil, `0` sin límite. */
  timeoutMs = $state<number | null>(null);

  async setTimeoutMs(millis: number | null) {
    if (!this.tabId) return;
    try {
      await queryTimeout(this.tabId, millis);
      this.timeoutMs = millis;
    } catch (error) {
      // Dentro de una transacción abortada un `SET` no corre: se dice en vez de aparentar que sí.
      this.log("error", describeError(error));
    }
  }

  // --- Vigilar ---------------------------------------------------------------------------------

  /** Cada cuántos segundos se repite la consulta, o `null` si no se vigila. */
  watchSeconds = $state<number | null>(null);
  /** Cuándo terminó la última repetición, para decir «hace N s». */
  watchedAt = $state<number | null>(null);
  private watchTimer: ReturnType<typeof setInterval> | null = null;

  /**
   * Si se puede vigilar lo último que se ejecutó: una sola lectura y con autocommit. Con autocommit
   * apagado cada repetición dejaría una transacción abierta contra el servidor.
   */
  get canWatch(): boolean {
    return this.autocommit && this.ranSql !== "" && isWatchable(this.ranSql);
  }

  startWatch(seconds: number) {
    this.stopWatch();
    if (!this.canWatch) return;
    this.watchSeconds = seconds;
    this.watchTimer = setInterval(() => void this.watchTick(), seconds * 1000);
  }

  stopWatch() {
    if (this.watchTimer !== null) clearInterval(this.watchTimer);
    this.watchTimer = null;
    this.watchSeconds = null;
  }

  private async watchTick() {
    // Sin ventana a la vista no hay quien mire, y con una repetición en curso no se encima otra.
    if (document.hidden || this.running || this.txStatus !== "idle" || !this.canWatch) return;
    await this.run(this.ranSql);
    this.watchedAt = Date.now();
    // Un error —conexión caída, permiso— se mostraría igual en cada vuelta: mejor frenar y que se vea.
    if (this.messages.some((message) => message.tone === "error")) this.stopWatch();
  }

  // --- Edición del resultado -------------------------------------------------------------------

  /** El plan de edición mientras se edita el resultado; `null` fuera de ese modo. */
  editPlan = $state.raw<EditPlan | null>(null);
  /** Cambios sin guardar, por fila y columna del resultado. */
  edits = $state.raw<ReadonlyMap<number, ReadonlyMap<number, string | null>>>(new Map());
  /** Por qué no se pudo entrar al modo de edición, o qué falló al guardar. */
  editError = $state<string | null>(null);
  editPreview = $state.raw<PreviewStatement[] | null>(null);
  editBusy = $state(false);

  get editedRowCount(): number {
    return editedRows(this.edits);
  }

  /** El resultado que se edita: el único con filas del último script. */
  private get editableOutcome() {
    const only = this.results.length === 1 ? this.results[0] : null;
    return only?.outcome.kind === "rows" ? only.outcome : null;
  }

  /** Si tiene sentido ofrecer el botón: una sola sentencia con filas, sin transacción por medio. */
  get canTryEdit(): boolean {
    return this.editableOutcome !== null && !this.running && !isReadOnly(this.profileId);
  }

  /**
   * Entra al modo de edición. Le pregunta al servidor de qué tabla sale el resultado y exige que la
   * clave esté entera entre las columnas: ante la duda no se ofrece, y se dice por qué.
   */
  async startEdit() {
    this.editError = null;
    if (!this.tabId || !this.editableOutcome) return;
    // Repetir la consulta pisaría lo que se está editando.
    this.stopWatch();
    // La escritura va por otra conexión: con una transacción abierta en esta pestaña vería datos
    // distintos de los que se están mirando, y podría quedar esperando un bloqueo de esta misma.
    if (this.txStatus !== "idle") {
      this.editError =
        "Hay una transacción abierta en esta pestaña: confirmala o revertila antes de editar el resultado.";
      return;
    }

    this.editBusy = true;
    try {
      const source = await queryEditSource(this.tabId, this.ranSql);
      const shape = await dataOpen(this.profileId, source.oid, this.database);
      const plan = planEdit(shape, source);
      if (typeof plan === "string") {
        this.editError = plan;
        return;
      }
      this.edits = new Map();
      this.editPlan = plan;
    } catch (error) {
      this.editError = describeError(error);
    } finally {
      this.editBusy = false;
    }
  }

  cancelEdit() {
    this.editPlan = null;
    this.edits = new Map();
    this.editPreview = null;
    this.editError = null;
  }

  setEdit(row: number, column: number, value: string | null) {
    const outcome = this.editableOutcome;
    if (!outcome) return;
    this.edits = withEdit(this.edits, outcome.rows, row, column, value);
  }

  private editChanges() {
    const outcome = this.editableOutcome;
    return this.editPlan && outcome ? buildChanges(this.editPlan, outcome.rows, this.edits) : [];
  }

  async showEditPreview() {
    if (!this.editPlan) return;
    this.editError = null;
    try {
      this.editPreview = await dataPreview(this.editPlan.shape, this.editChanges());
    } catch (error) {
      this.editError = describeError(error);
    }
  }

  /** Aplica los cambios y vuelve a ejecutar la consulta, para mostrar lo que quedó en la base. */
  async saveEdit() {
    const plan = this.editPlan;
    const changes = this.editChanges();
    if (!plan || changes.length === 0) return;
    if (!(await confirmMutation(this.profileId, `Se van a aplicar ${changes.length} cambios.`))) return;

    this.editBusy = true;
    this.editError = null;
    try {
      await dataApply(this.profileId, plan.shape, changes, this.database);
    } catch (error) {
      this.editError = describeError(error);
      this.editBusy = false;
      return;
    }
    const sql = this.ranSql;
    this.editBusy = false;
    this.cancelEdit();
    await this.run(sql);
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

  private apply(
    event: QueryEvent,
    starts: Map<number, { line: number; offset: number }>,
    base: number,
  ) {
    switch (event.type) {
      case "started":
        starts.set(event.index, { line: event.line, offset: event.offset });
        break;

      case "finished": {
        const start = starts.get(event.index);
        // Reasignar y no `push`: con `$state.raw` el cambio se notifica al reemplazar el arreglo.
        this.results = [
          ...this.results,
          {
            index: event.index,
            line: start?.line ?? 1,
            offset: start?.offset ?? 0,
            outcome: event.outcome,
          },
        ];
        // Con autocommit encendido cada sentencia se confirma sola: no hay nada que contar.
        if (!this.autocommit || this.txStatus !== "idle") {
          this.txStatements += 1;
          if (event.outcome.kind === "command") this.txRows += event.outcome.affected;
        }
        if (event.outcome.kind === "command") {
          this.log("info", `${event.outcome.tag}: ${event.outcome.affected}`);
          if (changesCatalog(event.outcome.tag)) this.catalogDirty = true;
        } else if (event.outcome.truncated) {
          this.log(
            "notice",
            `Se recortó el resultado: se trajeron ${event.outcome.rows.length} filas y hay más. ` +
              "El servidor corta en el máximo elegido, así que no se sabe cuántas son en total.",
          );
        }
        // Mostrar el último resultado con filas es lo que uno espera al ejecutar un script que
        // termina en un SELECT.
        this.shown = this.results.reduce(
          (last, result, index) => (result.outcome.kind === "rows" ? index : last),
          0,
        );
        break;
      }

      case "notice":
        this.log("notice", `${event.severity}: ${event.message}`);
        break;

      case "transaction":
        this.applyTx(event.status);
        break;

      case "failed": {
        if (isCanceled(event.error)) {
          this.log("info", "Consulta cancelada.");
          break;
        }
        if (isDisconnected(event.error)) reportServerDown(this.profileId);
        const description = describeError(event.error);
        this.view = "messages";
        // Si el núcleo detectó que falta un `;`, la marca va sobre el arranque de la segunda
        // sentencia y no sobre el token que señaló el servidor: ese token es inocente, PostgreSQL
        // se confundió por leer las dos sentencias como una sola (ver `sql::missing_separator`).
        // `separatorAt` ya viene en caracteres y con base 0, igual que `offset`.
        if (event.separatorAt !== null) {
          this.errorMark = {
            at: base + this.runUnmap(event.offset + event.separatorAt),
            message: description,
          };
        } else if (event.error.kind === "database" && event.error.position !== null) {
          // La posición del servidor viene con base 1 y relativa a su sentencia.
          this.errorMark = {
            at: base + this.runUnmap(event.offset + event.error.position - 1),
            message: event.error.message,
          };
        }

        // Un error del servidor se muestra como tarjeta; los demás —conexión, permiso— siguen siendo
        // una línea, que es todo lo que tienen para decir.
        if (event.error.kind === "database") {
          const at = this.errorMark?.at ?? null;
          const where = at === null ? null : cursorInfo(this.sql, at, at);
          const lineText =
            where === null ? null : (this.sql.split("\n")[where.line - 1]?.trim().slice(0, 200) ?? null);
          this.log("error", description, {
            code: event.error.code,
            message: event.error.message,
            detail: event.error.detail,
            hint: event.error.hint,
            line: where?.line ?? null,
            lineText: lineText || null,
            at,
          });
        } else {
          this.log("error", description);
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
      this.applyTx(await queryAutocommit(this.tabId, enabled));
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
      this.applyTx(await action(this.tabId));
      this.log("info", done);
      // Recién ahora el DDL de la transacción existe para las otras conexiones. Un `ROLLBACK` pasa
      // por acá igual: descartar el cambio también deja al árbol pidiendo una relectura, porque lo
      // que se ve puede venir de una lectura hecha en el medio.
      void this.syncCatalog();
    } catch (error) {
      this.log("error", describeError(error));
      this.view = "messages";
      // Un `COMMIT` que falla igual termina la transacción: se vuelve a preguntar en vez de suponer.
      this.applyTx(await queryTxStatus(this.tabId).catch(() => this.txStatus));
    }
  }

  /**
   * Ensaya el script: lo corre adentro de `BEGIN … ROLLBACK` y muestra qué habría pasado. No pasa
   * por la confirmación de producción ni por el historial porque no deja nada aplicado.
   */
  async rehearse(sql: string) {
    if (!this.tabId || this.running || this.rehearsing || sql.trim() === "") return;

    const bound = await this.bindVars(sql);
    if (bound === null) return;
    sql = bound.sql;

    this.rehearsing = true;
    this.rehearsal = null;
    this.rehearsalError = null;
    this.rehearsalOpen = true;
    try {
      this.rehearsal = await queryRehearse(this.tabId, sql);
    } catch (error) {
      this.rehearsalError = describeError(error);
    } finally {
      this.rehearsing = false;
    }
  }

  async explain(sql: string, base: number, options: ExplainOptions) {
    if (!this.tabId || this.running || sql.trim() === "") return;

    const bound = await this.bindVars(sql);
    if (bound === null) return;
    sql = bound.sql;
    this.runUnmap = bound.unmap;

    this.running = true;
    this.runStartedAt = Date.now();
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
        this.errorMark = {
          at: base + this.runUnmap(failure.position - 1),
          message: failure.message,
        };
      }
    } finally {
      this.running = false;
      this.runStartedAt = null;
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
    // `sql::format` es pura y nunca falla: en el peor caso, la guarda de equivalencia del núcleo
    // devuelve el texto tal cual estaba, así que no hace falta un try/catch aparte para esto.
    if (autoFormat.enabled) tab.sql = await formatSql(tab.sql);

    let path = askPath ? null : tab.filePath;
    if (!path) {
      path = await pickSave({
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
    // Sin reservar script: la pestaña ya tiene su archivo, que es justo el que se está abriendo.
    // `filePath` es el camino de «guardé esto en otro lado con Ctrl+S», y sigue siéndolo acá.
    const tab = await openQuery(profileId, database, name, false);
    last = tab;
    try {
      tab.sql = await sqlReadFile(path);
      tab.filePath = path;
      tab.baseline = tab.sql;
    } catch (error) {
      // La pestaña queda abierta y vacía con el motivo adentro: cerrarla sola escondería el error.
      tab.log("error", `No se pudo abrir ${path}: ${describeError(error)}`);
      tab.view = "messages";
    }
  }

  return last;
}

/**
 * Abre un script de la biblioteca fija en una pestaña nueva.
 *
 * A diferencia de `openSqlFiles`, la ruta queda en `scriptPath` y no en `filePath`: sigue el ciclo
 * propio de la biblioteca —autoguardado, renombre desde la pestaña—, no el de «guardar como» manual.
 * `scriptCreated` arranca en `true`: el archivo ya existe, es el que se vino a abrir.
 */
export async function openScript(
  path: string,
  profileId: string,
  database: string,
): Promise<QueryTab> {
  const name = path.split(/[\\/]/).pop() ?? "script.sql";
  const tab = await openQuery(profileId, database, name.replace(/\.sql$/i, ""), false);
  tab.scriptPath = path;
  tab.scriptCreated = true;
  try {
    tab.sql = await scriptRead(path);
  } catch (error) {
    tab.log("error", `No se pudo abrir ${path}: ${describeError(error)}`);
    tab.view = "messages";
  }
  return tab;
}

/**
 * Abre una pestaña de consulta contra una base y la deja seleccionada.
 *
 * `reserveScript` reserva de entrada el nombre del script de la biblioteca fija que le va a
 * corresponder (`script_new_name`, que arma la ruta completa del lado de Rust: ver
 * `commands/scripts.rs`). El archivo recién se crea en disco con el primer contenido no vacío —ver
 * `QueryTab.flushScript`—, así que reservarlo acá no escribe nada todavía. Va en `true` por omisión
 * porque es lo que corresponde a una pestaña nueva y en blanco; `openSqlFiles` y `openScript` lo
 * apagan porque ya tienen su propia ruta (`filePath` o `scriptPath`, según el caso).
 */
export async function openQuery(
  profileId: string,
  database: string,
  title: string,
  reserveScript = true,
): Promise<QueryTab> {
  const tab = tabs.add(new QueryTab(profileId, database, title));

  try {
    const opened = await queryOpen(profileId, database);
    tab.tabId = opened.tabId;
    tab.autocommit = opened.autocommit;
    tab.applyTx(opened.txStatus);
    tab.log("info", `Conectado a ${opened.database}.`);
    await applyDefaultTimeout(tab, profileId);
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

  if (reserveScript) {
    // Tampoco `await`: reservar el nombre no bloquea la pestaña, y si falla —perfil desconocido,
    // error de disco— simplemente no queda script asociado, que es degradar bien.
    scriptNewName(profileId)
      .then((path) => (tab.scriptPath = path))
      .catch(() => {});
  }

  return tab;
}

/**
 * Renombra la pestaña, y con ella su script si tiene uno.
 *
 * Doble clic sobre el título (y `F2` con la pestaña activa, ver `App.svelte`) son el único gesto
 * para esto: antes no había forma de ponerle nombre a una pestaña, solo de renombrar el archivo que
 * ya se había guardado con «Guardar como».
 *
 * Sin `scriptPath` no hay nada que tocar en disco: el título es un rótulo nada más. Con `scriptPath`
 * pero sin `scriptCreated` —la pestaña nunca escribió nada—, tampoco: `script_rename` fallaría
 * contra un archivo que no existe, así que alcanza con mover la reserva del nombre.
 */
export async function renameQueryTab(tab: QueryTab, title: string) {
  const trimmed = title.trim();
  if (!trimmed || trimmed === tab.title) return;

  if (!tab.scriptPath) {
    tab.title = trimmed;
    return;
  }

  const newPath = renamedScriptPath(tab.scriptPath, trimmed);
  if (newPath === tab.scriptPath) {
    tab.title = trimmed;
    return;
  }

  if (!tab.scriptCreated) {
    tab.scriptPath = newPath;
    tab.title = trimmed;
    return;
  }

  try {
    await scriptRename(tab.scriptPath, newPath);
    tab.scriptPath = newPath;
    tab.title = trimmed;
  } catch (error) {
    // Nombre repetido, permiso, lo que sea: se avisa en el registro de la pestaña y el título no se
    // toca, para no dejar la pestaña diciendo un nombre que el archivo no tiene.
    tab.log("error", `No se pudo renombrar el script: ${describeError(error)}`);
  }
}

/** En producción, una pestaña nueva arranca con tiempo máximo (ver `query-timeout.ts`). */
async function applyDefaultTimeout(tab: QueryTab, profileId: string) {
  const profile = explorer.profiles.find((item) => item.id === profileId);
  const ms = defaultTimeout(profile?.environment, profile?.statementTimeoutMs);
  if (ms !== null) await tab.setTimeoutMs(ms);
}
