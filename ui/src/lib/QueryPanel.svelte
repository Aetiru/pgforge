<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Confirm from "./Confirm.svelte";
  import Empty from "./Empty.svelte";
  import ExportDialog from "./ExportDialog.svelte";
  import { environmentOf, isReadOnly } from "./access.svelte";
  import { envBar, READ_ONLY_LOOK } from "./badges";
  import { explorer } from "./explorer.svelte";
  import HistoryPanel from "./HistoryPanel.svelte";
  import Icon from "./Icon.svelte";
  import IndexDialog from "./IndexDialog.svelte";
  import Modal from "./Modal.svelte";
  import PlanAdvice from "./PlanAdvice.svelte";
  import RehearsalResult from "./RehearsalResult.svelte";
  import PlanTree from "./PlanTree.svelte";
  import { planHeat } from "./plan-heat";
  import ResultGrid from "./ResultGrid.svelte";
  import ResultCompareDialog, { type CompareSource } from "./ResultCompareDialog.svelte";
  import ResultFileDialog from "./ResultFileDialog.svelte";
  import SaveQueryDialog from "./SaveQueryDialog.svelte";
  import SavedPanel from "./SavedPanel.svelte";
  import GridSize from "./GridSize.svelte";
  import SnippetDialog from "./SnippetDialog.svelte";
  import FormatOptionsDialog from "./FormatOptionsDialog.svelte";
  import VersionsDialog from "./VersionsDialog.svelte";
  import { formatSql } from "./format-options.svelte";
  import { versions } from "./editor/versions.svelte";
  import SnippetVarsDialog from "./SnippetVarsDialog.svelte";
  import SizeDecision from "./SizeDecision.svelte";
  import type { SnippetVariable } from "./snippet-vars";
  import SqlEditor from "./SqlEditor.svelte";
  import ObjectPeek from "./ObjectPeek.svelte";
  import ObjectPicker from "./ObjectPicker.svelte";
  import OutlinePanel from "./OutlinePanel.svelte";
  import { sqlIdent } from "./sql-generate";
  import { WITH_ROWS } from "./tree-actions";
  import type { SearchHit, SchemaRelation, TreeNode } from "./ipc";
  import { mount, unmount } from "svelte";
  import { autoFormat, editorSplit } from "./editor.svelte";
  import { offsetOfStatement } from "./format-cursor";
  import { PAGE_SIZES, paging } from "./paging.svelte";
  import { count, decimal } from "./format";
  import { planText } from "./plan-text";
  import { resultLabel } from "./result-tabs";
  import { shortcutStore } from "./shortcuts.svelte";
  import Elapsed from "./Elapsed.svelte";
  import ErrorCard from "./ErrorCard.svelte";
  import { cursorInfo } from "./cursor-info";
  import TimeZoneSelect from "./TimeZoneSelect.svelte";
  import { WATCH_STEPS } from "./watch-guard";
  import { TIMEOUT_OPTIONS } from "./query-timeout";
  import TxBar from "./TxBar.svelte";
  import TxDecision from "./TxDecision.svelte";
  import { openQuery, saveQueryTab, type QueryTab } from "./query.svelte";
  import { openData } from "./data.svelte";
  import { generateSql } from "./sql-generate";
  import type { DraggedObject } from "./tree-object";
  import {
    dataShapeNamed,
    dataOpen,
    describeError,
    routineDefinitions,
    explainWarning,
    statementAtCursor,
    queryCheck,
    columnValues,
    routineSignatures,
    historySuggest,
    databaseNames,
    type ExplainOptions,
    type IndexTarget,
    type Outcome,
    type TableColumn,
  } from "./ipc";

  let { tab }: { tab: QueryTab } = $props();

  /** `Mayús`+soltar un objeto del árbol: una relación entra como `SELECT` con todas sus columnas. */
  async function dropObject(object: DraggedObject): Promise<string | null> {
    if (!object.hasRows) return object.qualified;
    try {
      return generateSql(await dataOpen(object.profileId, object.oid, object.database), "select");
    } catch (error) {
      tab.log("error", describeError(error));
      return object.qualified;
    }
  }

  /** `Ctrl`+clic sobre una llamada: la definición de la función en una consulta nueva. */
  async function openRoutine(schema: string | null, name: string) {
    try {
      const found = await routineDefinitions(tab.profileId, schema, name, tab.database);
      if (found.length === 0) {
        tab.log("notice", `No se encontró una función o procedimiento llamado «${name}».`);
        return;
      }
      const opened = await openQuery(tab.profileId, tab.database, name);
      // Cada sobrecarga termina en `;` para que el script se pueda ejecutar entero.
      opened.sql = found.map((definition) => `${definition.trimEnd()};`).join("\n\n");
    } catch (error) {
      tab.log("error", describeError(error));
    }
  }

  // Valores de las variables `{{nombre}}`: la pestaña pregunta y espera a que el diálogo conteste.
  let varsAsk = $state<{
    variables: SnippetVariable[];
    resolve: (values: Record<string, string> | null) => void;
  } | null>(null);

  let sizeAsk = $state<{
    table: string;
    rows: number;
    limit: number;
    resolve: (choice: "limit" | "run" | "cancel") => void;
  } | null>(null);

  $effect(() => {
    const target = tab;
    target.askSize = (table, rows, limit) =>
      new Promise((resolve) => {
        sizeAsk = { table, rows, limit, resolve };
      });
    return () => {
      sizeAsk?.resolve("cancel");
      sizeAsk = null;
      target.askSize = null;
    };
  });

  $effect(() => {
    const target = tab;
    target.askVars = (variables) =>
      new Promise((resolve) => {
        // Las `{{variables}}` de una consulta son siempre de texto libre: el tipo es de las abreviaturas.
        varsAsk = { variables: variables.map((variable) => ({ ...variable, kind: "text" as const })), resolve };
      });
    return () => {
      // Soltar a quien esté esperando: una promesa que nunca se resuelve dejaría la pestaña muda.
      varsAsk?.resolve(null);
      varsAsk = null;
      target.askVars = null;
    };
  });

  // Tercer disparador del autoguardado del script: ~1 s sin cambios (los otros dos son perder el
  // foco de la ventana y cambiar de pestaña activa, ver `App.svelte` y `Tabs.activate`). Depende de
  // `tab.sql` sin usarlo directo —`scheduleScriptSave` lee `this.sql` cuando el temporizador
  // vence— para no reiniciar el rebote con cada letra desde afuera de la pestaña.
  $effect(() => {
    void tab.sql;
    tab.scheduleScriptSave();
  });

  /** Qué se acaba de copiar del plan, para el «Copiado» del botón. */
  let planCopied = $state<"text" | "json" | null>(null);

  /**
   * El texto se arma acá con lo que ya está en pantalla y el JSON es el que mandó el servidor.
   * Volver a pedirle el plan en formato texto significaría, con ANALYZE, ejecutar la consulta otra
   * vez; y reconstruir el JSON desde el árbol sería inventar los campos que no se muestran.
   */
  async function copyPlan(what: "text" | "json") {
    if (!tab.plan) return;
    const text = what === "text" ? planText(tab.plan) : tab.plan.json;
    await navigator.clipboard.writeText(text).catch(() => {});
    planCopied = what;
    setTimeout(() => (planCopied = planCopied === what ? null : planCopied), 1500);
  }

  /**
   * El índice que se está por crear desde una sugerencia del plan.
   *
   * Las columnas de la tabla se piden al abrir: la sugerencia sabe cuáles indexar, pero el diálogo
   * muestra todas para poder agregar una más antes de crearlo.
   */
  let newIndex = $state<{
    target: IndexTarget;
    columns: TableColumn[];
  } | null>(null);

  async function openIndexDialog(target: IndexTarget) {
    try {
      const shape = await dataShapeNamed(tab.profileId, target.schema, target.table, tab.database);
      newIndex = { target, columns: shape.columns };
    } catch (error) {
      tab.log("error", describeError(error));
    }
  }

  let exportOpen = $state(false);
  /** El texto exacto de la sentencia que se exporta, resuelto al apretar el botón. */
  let exportSql = $state("");

  /**
   * Con un solo resultado, exportar el texto entero de `tab.ranSql` es lo de siempre. Con varios,
   * cada pestaña de resultado tiene su propio recorte del script —viven en `ResultSet.offset`—, y
   * hay que pedirle al núcleo dónde termina esa sentencia para no mandar el script completo a un
   * `COPY` que solo envuelve una consulta.
   */
  async function openExport() {
    const chosen = tab.result;
    if (tab.results.length <= 1 || !chosen) {
      exportSql = tab.ranSql;
    } else {
      const statement = await statementAtCursor(tab.ranSql, chosen.offset);
      exportSql = statement?.text ?? tab.ranSql;
    }
    exportOpen = true;
  }

  /**
   * Ancla el resultado que se está mirando. Mismo patrón que `openExport()` para resolver el SQL
   * exacto de la sentencia elegida cuando el script devolvió más de un resultado.
   */
  async function pinCurrent() {
    const item = tab.result;
    if (!item) return;
    let sql = tab.ranSql;
    if (tab.results.length > 1) {
      const statement = await statementAtCursor(tab.ranSql, item.offset);
      sql = statement?.text ?? tab.ranSql;
    }
    tab.pin(sql, item.outcome, tab.showTypes ? tab.columnTypes : null);
  }

  let fileExportOpen = $state(false);
  let compareOpen = $state(false);

  /**
   * Los resultados con filas que se pueden comparar: los en vivo del último script y los anclados.
   * Comparar un resultado con el de antes de un cambio es para lo que sirve anclar.
   */
  const compareSources = $derived.by<CompareSource[]>(() => {
    const out: CompareSource[] = [];
    tab.results.forEach((item, index) => {
      if (item.outcome.kind !== "rows") return;
      out.push({
        id: `live:${index}`,
        label: `En vivo #${index + 1} · ${count(item.outcome.rowCount)} filas`,
        columns: item.outcome.columns,
        rows: item.outcome.rows,
        truncated: item.outcome.truncated,
      });
    });
    for (const item of tab.pinned) {
      if (item.outcome.kind !== "rows") continue;
      out.push({
        id: `pin:${item.id}`,
        label: `Anclado · ${item.label}`,
        columns: item.outcome.columns,
        rows: item.outcome.rows,
        truncated: item.outcome.truncated,
      });
    }
    return out;
  });
  const compareFrom = $derived(
    tab.pinnedShown !== null ? `pin:${tab.pinnedShown}` : `live:${tab.shown}`,
  );

  let saveOpen = $state(false);
  let snippetsOpen = $state(false);
  /** Texto con el que abrir el diálogo de abreviaturas ya en «nueva» (guardar la selección). */
  let snippetDraft = $state<{ body: string; convertLiterals: boolean } | null>(null);
  let formatOptionsOpen = $state(false);
  let versionsOpen = $state(false);
  /** Se incrementa al guardar, para que el panel de guardadas relea sin volver a montarse. */
  let savedStamp = $state(0);
  let editor = $state<ReturnType<typeof SqlEditor> | null>(null);
  let databases = $state<string[]>([]);
  let switching = $state<string | null>(null);
  let pending = $state<{
    sql: string;
    base: number;
    options: ExplainOptions;
    warning: string;
  } | null>(null);

  const environment = $derived(environmentOf(tab.profileId));
  // Dónde se escribe, para el alcance de las abreviaturas y sus valores automáticos.
  const snippetContext = $derived.by(() => {
    const profile = explorer.profiles.find((candidate) => candidate.id === tab.profileId);
    return {
      profileId: tab.profileId,
      database: tab.database,
      server: profile?.name ?? null,
      user: profile?.user ?? null,
    };
  });
  const serverName = $derived(
    explorer.profiles.find((profile) => profile.id === tab.profileId)?.name ?? null,
  );
  const result = $derived(tab.result);
  const withRows = $derived(result?.outcome.kind === "rows" ? result.outcome : null);
  const pinnedResult = $derived(
    tab.pinnedShown === null ? null : (tab.pinned.find((item) => item.id === tab.pinnedShown) ?? null),
  );
  const errors = $derived(tab.messages.filter((message) => message.tone === "error").length);

  /** Qué pesa el nodo más caro del plan, para que el color de cada uno signifique algo. */
  const heat = $derived(tab.plan ? planHeat(tab.plan.root) : null);

  /**
   * Qué ejecutar cuando no se pidió el script entero: lo seleccionado si hay selección, y si no la
   * sentencia donde está el cursor. Quién decide dónde termina una sentencia es el núcleo.
   */
  async function resolve(selection: string, cursor: number) {
    if (selection.trim() !== "") return { sql: selection, base: cursor };

    const statement = await statementAtCursor(tab.sql, cursor);
    return statement ? { sql: statement.text, base: statement.offset } : null;
  }

  /**
   * Dónde está el cursor se le pregunta al editor en el momento del clic.
   *
   * Guardarlo en el panel cada vez que el editor avisaba dejaba a los botones ejecutando la
   * sentencia de la vez anterior: el atajo lo actualizaba, pero llegar al botón con el mouse no.
   */
  // Marcar qué sentencia ejecutaría Ctrl+Enter. Se pregunta al núcleo —es quien decide dónde termina
  // una sentencia— con un respiro: mover el cursor con las flechas no puede costar una vuelta por
  // cada tecla. Un resultado que llega tarde se descarta si el cursor ya se movió.
  let markTimer: ReturnType<typeof setTimeout> | undefined;
  let markSerial = 0;
  function scheduleStatementMark() {
    clearTimeout(markTimer);
    markTimer = setTimeout(async () => {
      const serial = ++markSerial;
      const { text, cursor } = here();
      if (text.trim() !== "" || tab.sql.trim() === "") {
        editor?.markStatement(null);
        return;
      }
      const statement = await statementAtCursor(tab.sql, cursor).catch(() => null);
      if (serial === markSerial) editor?.markStatement(statement);
    }, 150);
  }

  function here() {
    return editor?.selection() ?? { text: "", cursor: 0 };
  }

  /**
   * «Ejecutar hasta el cursor» y «desde el cursor»: el mismo camino que el script entero
   * (`tab.run`: variables, tamaño, confirmación de producción) con el tramo que el editor recortó.
   * No se formatea antes aunque «autoformatear» esté prendido: reescribir el texto movería el
   * `base` con el que se ubica un error.
   */
  async function runRange(range: { text: string; base: number }) {
    await tab.run(range.text, range.base);
  }

  // El esquema del script se recuerda entre sesiones: es una forma de trabajar, no de una pestaña.
  const OUTLINE_KEY = "pgforge.outline";
  let outlineOpen = $state(
    (() => {
      try {
        return localStorage.getItem(OUTLINE_KEY) === "1";
      } catch {
        return false;
      }
    })(),
  );

  function toggleOutline() {
    outlineOpen = !outlineOpen;
    try {
      localStorage.setItem(OUTLINE_KEY, outlineOpen ? "1" : "0");
    } catch {
      // Sin almacenamiento el panel igual funciona; solo no se recuerda.
    }
  }

  /** Insertar un objeto buscándolo: el diálogo abierto, o `null`. */
  let picking = $state(false);

  /** Quién se queda con el foco al cerrar el buscador o el esquema. */
  function insertPicked(hit: SearchHit, withColumns: boolean) {
    picking = false;
    // La etiqueta de una función lleva la firma (`calcular(a integer)`): se inserta solo el nombre.
    const name = typeof hit.kind === "string" && (hit.kind === "function" || hit.kind === "procedure")
      ? hit.label.replace(/\(.*$/s, "")
      : hit.label;
    const qualified = `${sqlIdent(hit.schema)}.${sqlIdent(name)}`;
    const hasRows = typeof hit.kind === "string" && WITH_ROWS.includes(hit.kind);

    if (withColumns && hasRows) {
      void dropObject({
        profileId: tab.profileId,
        database: hit.database,
        oid: hit.oid,
        qualified,
        hasRows: true,
      }).then((text) => editor?.insertText(text ?? qualified));
    } else {
      editor?.insertText(qualified);
    }
  }

  /**
   * La tarjeta de vista rápida de una tabla del editor: la misma del árbol (`ObjectPeek`), montada
   * dentro del globo que CodeMirror abre y desmontada al cerrarse. `SchemaRelation` no dice si es
   * tabla o vista, así que se dibuja con el ícono de tabla.
   */
  function peekFor(relation: SchemaRelation, target: HTMLElement): () => void {
    const node: TreeNode = {
      id: "",
      label: relation.name,
      kind: "table",
      hasChildren: false,
      database: tab.database,
      schema: relation.schema,
      oid: relation.oid,
      comment: relation.comment ?? undefined,
    };
    const card = mount(ObjectPeek, { target, props: { profileId: tab.profileId, node, inline: true } });
    return () => void unmount(card);
  }

  // Lo que la paleta le pide al editor de esta pestaña (`QueryTab.requestEditor`). El esquema y el
  // buscador de objetos viven acá y no en el editor; el resto lo resuelve él.
  let lastEditorRequest = untrack(() => tab.editorRequest.n);
  $effect(() => {
    const { action, n } = tab.editorRequest;
    if (n === lastEditorRequest || action === null) return;
    lastEditorRequest = n;
    if (action === "outline") toggleOutline();
    else if (action === "insertObject") picking = true;
    else editor?.runAction(action);
  });

  async function run(selection: string, cursor: number) {
    const target = await resolve(selection, cursor);
    if (target) await tab.run(target.sql, target.base);
  }

  /**
   * Cuántas sentencias preceden, en el script sin formatear, a la que contiene el cursor.
   *
   * No hay un `sql::split` expuesto aparte —solo `statementAtCursor`—, así que se cuenta llamándolo
   * de a una en vez de reimplementar el partidor acá: es la misma regla que ya se sigue en
   * `resolve()`, y evita que aparezca un segundo partidor que se desincronice del núcleo.
   */
  async function statementIndex(sql: string, cursor: number): Promise<number> {
    const target = await statementAtCursor(sql, cursor);
    if (!target) return 0;

    let index = 0;
    let at = 0;
    for (;;) {
      const statement = await statementAtCursor(sql, at);
      if (!statement || statement.offset >= target.offset) return index;
      index += 1;
      at = statement.offset + statement.text.length + 1;
    }
  }

  /**
   * Formatea la selección si hay una, o el documento entero si no. Es la llamada al núcleo que
   * `SqlEditor` no puede hacer por su cuenta: el atajo, el botón y el comando de la paleta terminan
   * los tres acá, cada uno con la selección y el cursor de donde le tocaba pedirlos.
   */
  async function doFormat(selection: string, cursor: number) {
    if (!editor) return;

    try {
      if (selection.trim() !== "") {
        const formatted = await formatSql(selection);
        editor.applyFormat(formatted, formatted.length);
        return;
      }

      const original = tab.sql;
      const formatted = await formatSql(original);
      // La guarda de equivalencia del núcleo no encontró nada para ordenar: no hay por qué mover
      // el cursor de donde estaba.
      if (formatted === original) return;

      const index = await statementIndex(original, cursor);
      editor.applyFormat(formatted, offsetOfStatement(formatted, index));
    } catch (error) {
      tab.log("error", describeError(error));
    }
  }

  /** «Ejecutar el script entero»: con el interruptor prendido, ordena el SQL antes de correrlo. */
  async function runWholeScript() {
    if (autoFormat.enabled) tab.sql = await formatSql(tab.sql);
    await tab.run(tab.sql);
  }

  // La paleta de comandos no tiene el editor de CodeMirror a mano: pide formatear incrementando
  // `tab.formatRequest`, y acá se lo escucha para disparar el mismo camino que el botón. El valor
  // inicial se lee con `untrack`: es solo el punto de partida de la comparación, no algo a lo que
  // este efecto tenga que reaccionar.
  let lastFormatRequest = untrack(() => tab.formatRequest);
  $effect(() => {
    if (tab.formatRequest === lastFormatRequest) return;
    lastFormatRequest = tab.formatRequest;
    const { text, cursor } = here();
    doFormat(text, cursor);
  });

  // Los diálogos de opciones del formateador y de versiones se piden desde la paleta con contadores,
  // igual que formatear: este panel es quien los tiene montados.
  let lastFormatOptionsRequest = untrack(() => tab.formatOptionsRequest);
  let lastVersionsRequest = untrack(() => tab.versionsRequest);
  $effect(() => {
    if (tab.formatOptionsRequest !== lastFormatOptionsRequest) {
      lastFormatOptionsRequest = tab.formatOptionsRequest;
      formatOptionsOpen = true;
    }
    if (tab.versionsRequest !== lastVersionsRequest) {
      lastVersionsRequest = tab.versionsRequest;
      versionsOpen = true;
    }
  });

  // Cada cambio del texto reinicia la cuenta de quietud tras la que se guarda una copia. El
  // temporizador vive en `versions`, no acá: este panel se destruye al cambiar de pestaña y la copia
  // tiene que guardarse igual.
  $effect(() => {
    void tab.sql;
    versions.touch(tab);
  });

  /**
   * «Ensayar»: la selección si hay, y si no el script entero. Al revés que «Ejecutar», que toma la
   * sentencia del cursor: lo que se ensaya es casi siempre un script, y ensayar una sola sentencia
   * suelta de un script de cinco da un recuento que no es el de la corrida de verdad.
   */
  async function rehearse(selection: string) {
    await tab.rehearse(selection.trim() !== "" ? selection : tab.sql);
  }

  async function explain(selection: string, cursor: number, options: ExplainOptions) {
    const target = await resolve(selection, cursor);
    if (!target) return;

    try {
      // El aviso lo decide el núcleo para que la CLI advierta exactamente lo mismo.
      const warning = await explainWarning(target.sql, options);
      if (warning) {
        pending = { ...target, options, warning };
        return;
      }
    } catch (error) {
      tab.log("error", describeError(error));
    }

    await tab.explain(target.sql, target.base, options);
  }

  /**
   * Lo mínimo que se le deja a los resultados: la barra y unas filas. El alto del editor se guarda en
   * píxeles, y uno elegido en una pantalla alta —un escritorio remoto a 1080p— no entra en una más
   * baja o con la escala de Windows al 125 %: el editor es `shrink-0`, así que se comía todo, y el
   * divisor para achicarlo quedaba recortado fuera de vista, sin forma de recuperar la grilla.
   */
  const RESULTS_MIN = 120;

  let panelHeight = $state(0);

  /**
   * El alto que se dibuja, recortado al espacio que hay. La preferencia guardada no se toca: de
   * vuelta en la pantalla grande, el editor recupera el alto que se eligió ahí.
   */
  const editorHeight = $derived(
    panelHeight > 0
      ? Math.min(editorSplit.height, Math.max(0, panelHeight - RESULTS_MIN))
      : editorSplit.height,
  );

  function startResize(event: MouseEvent) {
    event.preventDefault();
    const origin = event.clientY;
    // Desde lo que se ve y no desde lo guardado: si estaba recortado, arrastrar no tiene que
    // recorrer primero el tramo invisible para empezar a moverse.
    const initial = editorHeight;

    const move = (moved: MouseEvent) => {
      editorSplit.set(Math.min(initial + moved.clientY - origin, panelHeight - RESULTS_MIN));
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      document.body.classList.remove("cursor-row-resize");
    };
    document.body.classList.add("cursor-row-resize");
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }

  // `verbose` va siempre y no es una preferencia: es lo único que hace que el plan traiga el esquema
  // de cada relación, y sin eso el `CREATE INDEX` que se sugiere queda sin calificar —y copiado a
  // otra sesión podría caer sobre otra tabla que se llame igual—. Lo que agrega de más (la lista de
  // columnas de salida) no se dibuja; sí viaja en el JSON, que es donde suma.
  const analyze: ExplainOptions = { analyze: true, buffers: true, verbose: true };
  const estimate: ExplainOptions = { analyze: false, buffers: false, verbose: true };

  // Las bases del servidor, para poder apuntar la pestaña a otra sin abrir una nueva. Se piden una
  // sola vez por pestaña: crear o borrar bases mientras se escribe una consulta no es lo normal.
  $effect(() => {
    const id = tab.profileId;
    let cancelled = false;
    databaseNames(id, { showSystemSchemas: false })
      .then((names) => {
        if (cancelled) return;
        databases = names;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  /** Menús del riel: Analizar y Archivo. Fijos en pantalla para no quedar cortados por el riel. */
  let railMenu = $state<{ kind: "analyze" | "file"; x: number; y: number } | null>(null);

  function openRailMenu(event: MouseEvent, kind: "analyze" | "file") {
    if (railMenu?.kind === kind) {
      railMenu = null;
      return;
    }
    const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
    railMenu = { kind, x: window.innerWidth - box.left + 4, y: box.top };
  }

  /** Cerrar el menú y correr la acción: todas las entradas hacen las dos cosas. */
  function fromMenu(action: () => void) {
    railMenu = null;
    action();
  }

  /** Pedido de encender Auto con una transacción abierta: espera la decisión. */
  let askAuto = $state(false);
  let deciding = $state(false);

  function pickMode(auto: boolean) {
    if (auto === tab.autocommit) return;
    if (auto && tab.txStatus !== "idle") {
      askAuto = true;
      return;
    }
    void tab.setAutocommit(auto);
  }

  async function decideAuto(choice: "commit" | "rollback" | "keep") {
    deciding = true;
    try {
      if (choice === "commit") await tab.commit();
      else if (choice === "rollback") await tab.rollback();
      // Si el commit falló la transacción sigue abierta: no se enciende el autocommit encima.
      if (choice === "keep" || tab.txStatus === "idle") await tab.setAutocommit(true);
    } finally {
      deciding = false;
      askAuto = false;
    }
  }

  async function switchDatabase(database: string) {
    // Cambiar de base cierra la sesión, y con ella se revierte lo que esté sin confirmar.
    if (tab.txStatus !== "idle") {
      switching = database;
      return;
    }
    await tab.switchDatabase(database);
  }

  // Dos grupos y no una sola tira: Resultados/Plan/Mensajes son el resultado de la corrida;
  // Historial/Guardadas son de dónde sacar el SQL para la siguiente. Mezclados en una sola `.seg` se
  // leían como si fueran la misma pregunta.
  const RESULT_VIEWS = [
    { value: "rows", label: "Resultados", icon: "table", hint: "Las filas que devolvió" },
    { value: "plan", label: "Plan", icon: "plan", hint: "Cómo resolvió la consulta el servidor" },
    {
      value: "messages",
      label: "Mensajes",
      icon: "info",
      hint: "Los avisos del servidor y los errores de cada sentencia",
    },
  ] as const;

  const SOURCE_VIEWS = [
    { value: "history", label: "Historial", icon: "clock", hint: "Lo que se ejecutó antes" },
    { value: "saved", label: "Guardadas", icon: "star", hint: "Las consultas guardadas con nombre" },
  ] as const;

  /**
   * Alt+1…Alt+9 salta al resultado N; Alt+←/→ al anterior o siguiente. Solo mientras se está mirando
   * «Resultados» y hay más de uno: en cualquier otra solapa esas teclas no tendrían con qué trabajar.
   * Va en un listener de la ventana porque el foco suele estar en el editor de CodeMirror y no en la
   * tira de pestañas.
   */
  function onResultKey(event: KeyboardEvent) {
    if (!event.altKey || tab.view !== "rows" || tab.results.length < 2) return;

    if (event.key >= "1" && event.key <= "9") {
      const index = Number(event.key) - 1;
      if (index < tab.results.length) {
        event.preventDefault();
        tab.shown = index;
      }
    } else if (event.key === "ArrowLeft" && tab.shown > 0) {
      event.preventDefault();
      tab.shown -= 1;
    } else if (event.key === "ArrowRight" && tab.shown < tab.results.length - 1) {
      event.preventDefault();
      tab.shown += 1;
    }
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key === "Escape" && railMenu) railMenu = null;
    onResultKey(event);
  }}
/>

<div class="flex h-full flex-col" bind:clientHeight={panelHeight}>
  <TxBar {tab} prodName={environment === "prod" ? serverName : null} />

  <!--
    Plegado, el editor se queda con todo el alto: por eso crece en vez de llevar altura fija. Antes
    esta era la barra «Ejecutar» horizontal, encima del editor; ahora es un riel angosto a la
    derecha, para devolverle al editor la fila de alto que ocupaba. El editor y el riel comparten
    esta fila `flex`: el primero se achica (`min-w-0`) para no empujar al segundo afuera.
  -->
  <div
    class="flex min-h-0 {editorSplit.resultsHidden
      ? 'flex-1'
      : editorSplit.editorHidden
        ? 'hidden'
        : 'shrink-0'}"
    style={editorSplit.resultsHidden ? "" : `height: ${editorHeight}px`}
  >
    <div class="min-h-0 min-w-0 flex-1 overflow-hidden">
      <SqlEditor
        bind:this={editor}
        bind:value={tab.sql}
        schema={tab.schema}
        relations={tab.relations}
        foreignKeys={tab.foreignKeys}
        errorMark={tab.errorMark}
        statementAt={(text, cursor) => statementAtCursor(text, cursor).catch(() => null)}
        canCheck={() => !tab.running && tab.txStatus === "idle" && tab.tabId !== null}
        checkSql={(sql) => queryCheck(tab.tabId!, sql)}
        valuesFor={(schema, table, column) =>
          columnValues(tab.profileId, tab.database, schema, table, column)}
        signaturesFor={(schema, name) =>
          routineSignatures(tab.profileId, tab.database, schema, name)}
        suggestFor={(prefix) => historySuggest(prefix, 5)}
        {snippetContext}
        onsavesnippet={(body) => (snippetDraft = { body, convertLiterals: true })}
        initialSelection={tab.editorSelection}
        initialTopPos={tab.editorTopPos}
        onrun={(selection, cursor) => run(selection, cursor)}
        onrunScript={runWholeScript}
        oncommit={() => tab.txStatus === "active" && tab.commit()}
        onrollback={() => tab.txStatus !== "idle" && tab.rollback()}
        oncancel={() => tab.cancel()}
        onsave={(askPath) => saveQueryTab(tab, askPath)}
        onformat={(selection, cursor) => doFormat(selection, cursor)}
        onversions={() => (versionsOpen = true)}
        onreveal={(relation) =>
          relation &&
          explorer.revealRelation(tab.profileId, tab.database, relation.schema, relation.oid)}
        onopen={(relation) =>
          void openData(tab.profileId, tab.database, relation.name, relation.oid)}
        onroutine={(schema, name) => void openRoutine(schema, name)}
        onobjectdrop={dropObject}
        onrunrange={runRange}
        onoutline={toggleOutline}
        onpickobject={() => (picking = true)}
        {peekFor}
        onposition={(state) => {
          tab.editorSelection = { anchor: state.anchor, head: state.head };
          tab.cursor = cursorInfo(tab.sql, state.anchor, state.head);
          tab.editorTopPos = state.topPos;
          scheduleStatementMark();
        }}
      />
    </div>

    <!--
      El riel: la barra «Ejecutar» de canto. Íconos y no palabras, igual que antes —«Ejecutar»
      conserva el `aria-label` sin texto visible porque acá no hay ancho para ninguno; el resto ya
      llevaba solo el `title`. El entorno sigue pintando la barra entera (`envBar`), ahora en el
      borde izquierdo del riel en vez del de arriba.

      `rail-compact` fija el ancho en 32px y deja el alto librado al `flex` de la fila de arriba;
      `.btn-icon` ahí llena esos 32px enteros, mismo criterio que `.toolbar-compact .btn-icon` en la
      barra horizontal. Los íconos van a 16px: con más aire por control que en la barra de 24px, un
      ícono de 12-13px se leía sin definición.
    -->
    {#if outlineOpen}
      <OutlinePanel
        text={tab.sql}
        line={tab.cursor.line}
        onpick={(from) => editor?.goToIndex(from)}
        onclose={toggleOutline}
      />
    {/if}

    <nav class="rail-compact {envBar(environment)}" aria-label="Ejecutar consulta">
      {#if tab.running}
        <button
          class="btn btn-danger-ghost btn-icon"
          aria-label="Cancelar"
          title="Cancela la consulta en curso"
          onclick={() => tab.cancel()}
        >
          <Icon name="close" size={16} />
        </button>
      {:else}
        <button
          class="btn btn-icon btn-run"
          disabled={tab.tabId === null}
          aria-label="Ejecutar"
          title={shortcutStore.hint("Ejecuta la selección, o la sentencia donde está el cursor", "editor.run")}
          onclick={() => {
            const { text, cursor } = here();
            run(text, cursor);
          }}
        >
          <Icon name="play" size={16} />
        </button>
        <button
          class="btn btn-ghost btn-icon"
          disabled={tab.tabId === null}
          aria-label="Ejecutar el script entero"
          title={shortcutStore.hint("Ejecuta todas las sentencias del editor", "editor.runScript")}
          onclick={runWholeScript}
        >
          <Icon name="play-all" size={16} />
        </button>
      {/if}

      <span class="rail-sep"></span>

      <!-- Explicar, medir y ensayar son la misma pregunta —«qué haría esto»— y viven juntos en un
           menú: tres íconos casi iguales en el riel no se distinguían sin pasar el mouse. -->
      <button
        class="btn btn-ghost btn-icon"
        disabled={tab.tabId === null}
        aria-label="Analizar"
        aria-haspopup="menu"
        title="Plan estimado, plan medido y ensayo"
        onclick={(event) => openRailMenu(event, "analyze")}
      >
        <Icon name="plan" size={16} />
      </button>

      <span class="rail-sep"></span>

      <button
        class="btn btn-ghost btn-icon"
        aria-label="Archivo y formato"
        aria-haspopup="menu"
        title="Guardar como .sql, guardar con nombre y formatear"
        onclick={(event) => openRailMenu(event, "file")}
      >
        <Icon name="save" size={16} />
      </button>

      <span class="rail-sep"></span>

      <button
        class="btn btn-ghost btn-icon"
        aria-pressed={outlineOpen}
        aria-label="Esquema del script"
        title={shortcutStore.hint("Lista las sentencias del script para saltar a cualquiera", "editor.outline")}
        onclick={toggleOutline}
      >
        <Icon name="columns" size={16} />
      </button>

      <!-- Se configuran acá y no en una pantalla de preferencias aparte: uno se acuerda de que
           quiere una abreviatura mientras escribe la consulta que la pediría. -->
      <button
        class="btn btn-ghost btn-icon"
        aria-label="Abreviaturas del editor"
        title="Abreviaturas: escribí una y apretá Tab para expandirla"
        onclick={() => (snippetsOpen = true)}
      >
        <Icon name="sql" size={16} />
      </button>

      <button
        class="btn btn-ghost btn-icon"
        aria-label="Versiones anteriores"
        title={shortcutStore.hint("Versiones anteriores de este texto", "editor.versions")}
        onclick={() => (versionsOpen = true)}
      >
        <Icon name="clock" size={16} />
      </button>

      <!-- El tamaño de letra se saca del riel (ocupaba tres filas verticales, "A−"/número/"A+",
           demasiado para un control que ya tiene atajo: Ctrl +, Ctrl - y Ctrl 0 siguen andando
           igual). Sigue con botón visible en `Sql.svelte` —el DDL no tiene foco de teclado propio
           y ahí sí hace falta—, pero en el editor de consultas, donde el foco está siempre en el
           texto, el atajo alcanza. -->
      <button
        class="btn btn-ghost btn-icon btn-toggle"
        aria-pressed={autoFormat.enabled}
        aria-label="Autoformatear"
        title={autoFormat.enabled
          ? "Autoformatear encendido: el SQL se ordena solo antes de guardar y antes de ejecutar el script entero"
          : "Autoformatear apagado: el SQL se ordena solo cuando apretás el botón de formatear"}
        onclick={() => autoFormat.set(!autoFormat.enabled)}
      >
        <Icon name="format-auto" size={16} />
      </button>

      <!-- Empuja lo que sigue al fondo del riel: misma idea que el `ml-auto` de la barra
           horizontal, rotada. -->
      <span class="mt-auto"></span>

      {#if tab.running}
        <span class="spinner my-1" title="Ejecutando…"></span>
      {/if}
    </nav>
  </div>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="group relative h-px shrink-0 bg-zinc-200 dark:bg-zinc-700 {editorSplit.mode !== 'split'
      ? 'hidden'
      : ''}"
    onmousedown={startResize}
    ondblclick={() => editorSplit.reset()}
    title="Arrastrá para cambiar la altura del editor; doble clic para restablecerla. Vale para
           todas las pestañas de consulta y se recuerda."
  >
    <div
      class="absolute inset-x-0 -top-[3px] h-[7px] cursor-row-resize transition-colors
             group-hover:bg-blue-500/40"
    ></div>
  </div>

  <div class="flex min-h-0 flex-col {editorSplit.resultsHidden ? 'shrink-0' : 'flex-1'}">
    <!-- Mismo tratamiento que la barra de «Ejecutar»: 24px, sin fondo de grupo, ver `toolbar-compact`
         en `app.css`. No usaba `.toolbar` antes de esta vuelta —era un `div` aparte con sus propias
         clases—, así que se suma acá y no en un tercer lugar. -->
    <div class="toolbar toolbar-compact">
      <!-- El botón queda del lado del panel que esconde, y sigue a la vista plegado: un panel que
           se esconde sin dejar de dónde agarrarlo no se vuelve a abrir. -->
      <button
        class="btn btn-ghost btn-icon btn-toggle"
        aria-label={editorSplit.resultsHidden ? "Mostrar los resultados" : "Ocultar los resultados"}
        aria-pressed={!editorSplit.resultsHidden}
        title={editorSplit.resultsHidden
          ? "Muestra de nuevo los resultados"
          : "Pliega los resultados y le deja todo el alto al editor"}
        onclick={() => editorSplit.toggleResults()}
      >
        <Icon name="panel-bottom" size={13} />
      </button>

      <!-- El inverso: le da todo el alto a la grilla. Va al lado del anterior porque son la misma
           pregunta —qué se queda con el espacio— hecha desde el otro lado. -->
      <button
        class="btn btn-ghost btn-icon btn-toggle"
        aria-label={editorSplit.editorHidden ? "Restablecer el reparto" : "Maximizar los resultados"}
        aria-pressed={!editorSplit.editorHidden}
        title={editorSplit.editorHidden
          ? "Vuelve a mostrar el editor"
          : "Pliega el editor y le deja todo el alto a la grilla"}
        onclick={() => editorSplit.toggleEditor()}
      >
        <Icon name="panel-top" size={13} />
      </button>

      <!--
        Cinco solapas escritas ocupaban un tercio de la barra para decir cinco palabras que no
        cambian nunca. Como íconos elegidos ocupan lo que ocupa un ícono, el nombre está en el
        `title`, y la única que necesita decir algo —cuántos errores tiene el script— lo dice con un
        contador encima. La raya del medio se queda: Resultados/Plan/Mensajes son el resultado de la
        corrida, e Historial/Guardadas son de dónde sacar el SQL de la próxima.
      -->
      <div class="flex items-center gap-0.5" role="tablist">
        {#each RESULT_VIEWS as item (item.value)}
          <button
            class="btn btn-ghost btn-icon btn-toggle relative"
            role="tab"
            aria-selected={tab.view === item.value}
            aria-label={item.label}
            title="{item.label} — {item.hint}"
            onclick={() => (tab.view = item.value)}
          >
            <Icon name={item.value === "messages" && errors > 0 ? "warn" : item.icon} size={13} />
            {#if item.value === "messages" && errors > 0}
              <span class="badge-count bg-rose-600">{errors}</span>
            {/if}
          </button>
        {/each}
      </div>

      <span class="toolbar-sep"></span>

      <div class="flex items-center gap-0.5" role="tablist">
        {#each SOURCE_VIEWS as item (item.value)}
          <button
            class="btn btn-ghost btn-icon btn-toggle"
            role="tab"
            aria-selected={tab.view === item.value}
            aria-label={item.label}
            title="{item.label} — {item.hint}"
            onclick={() => (tab.view = item.value)}
          >
            <Icon name={item.icon} size={13} />
          </button>
        {/each}
      </div>

      <span class="toolbar-sep"></span>

      <!--
        El modo de la transacción, con palabras: un ícono suelto obligaba a pasar el mouse para
        saber si lo que se ejecuta se confirma solo. Manual va en ámbar porque es el que pide
        atención. Commit y Rollback no están acá: viven en la franja de arriba del editor, que solo
        existe mientras hay una transacción.
      -->
      <div class="flex items-center gap-1.5">
        <div class="seg" role="group" aria-label="Modo de transacción">
          <button
            class="seg-item"
            aria-pressed={tab.autocommit}
            disabled={tab.tabId === null || tab.running}
            title="Autocommit: cada ejecución se confirma sola"
            onclick={() => pickMode(true)}
          >
            Auto
          </button>
          <button
            class="seg-item seg-warn"
            aria-pressed={!tab.autocommit}
            disabled={tab.tabId === null || tab.running}
            title="Manual: cada ejecución abre una transacción que se confirma con Commit"
            onclick={() => pickMode(false)}
          >
            Manual
          </button>
        </div>
        {#if !tab.autocommit && tab.txStatus === "idle"}
          <span class="text-xs muted">Se abrirá al ejecutar</span>
        {/if}
      </div>

      <!--
        Los controles que solo tienen sentido mirando un resultado: en «Mensajes» o «Historial»,
        «Tipos» y «Exportar» no actúan sobre nada.
      -->
      <!--
        Cuántas filas y cuánto tardó es lo primero que se mira al terminar una consulta, y estaba
        último: después de la letra, del techo, de los tipos y de los dos botones, contra el borde
        derecho de una pantalla ancha. Va acá, al lado de la pestaña que lo produjo.
      -->
      {#if tab.view === "rows" && withRows}
        <span class="flex items-center gap-2 text-xs muted">
          <span class="tabular-nums">
            {withRows.truncated ? "más de " : ""}<strong class="font-semibold text-zinc-800 dark:text-zinc-100"
              >{count(withRows.truncated ? withRows.rows.length : withRows.rowCount)}</strong
            >
            {withRows.rowCount === 1 ? "fila" : "filas"} · {decimal(withRows.seconds * 1000, 0)} ms
          </span>
        </span>
      {/if}

      <!--
        Contra qué servidor y qué base corre esta pestaña: contexto de la pestaña, no del
        resultado, así que va siempre —sin importar si se está mirando «Resultados», «Plan» o
        «Mensajes»— y no adentro del `{#if tab.view === "rows"}` de abajo. Antes vivía en la barra
        «Ejecutar»; movido acá porque ahora esa barra es el riel angosto de la derecha, sin lugar
        para un `<select>` ni para pastillas de texto. Es el primer `ml-auto` de la fila —el que
        empuja todo lo que sigue al borde derecho—, así que el bloque de después no lleva el suyo.
      -->
      <span class="ml-auto flex items-center gap-1.5 text-xs muted">
        {#if isReadOnly(tab.profileId)}
          <span class="tag tag-neutral" title={READ_ONLY_LOOK.title}>{READ_ONLY_LOOK.label}</span>
        {/if}
        <!-- Contra qué base corre se elige acá y no cerrando la pestaña para abrir otra: es la
             misma consulta contra otra base, que es como se prueba algo entre desarrollo y
             producción. -->
        <label class="flex items-center gap-1" title="Base sobre la que corre esta pestaña">
          <Icon name="database" size={11} />
          <select
            class="field max-w-44 py-0.5 text-xs"
            value={tab.database}
            disabled={tab.running || tab.opening}
            onchange={(event) => {
              // El desplegable vuelve a lo que hay hasta que el cambio salga bien: si falla, o si
              // la confirmación por la transacción abierta se cancela, mostraría una base que no
              // es.
              const chosen = event.currentTarget.value;
              event.currentTarget.value = tab.database;
              switchDatabase(chosen);
            }}
          >
            {#if !databases.includes(tab.database)}
              <option value={tab.database}>{tab.database}</option>
            {/if}
            {#each databases as name (name)}
              <option value={name}>{name}</option>
            {/each}
          </select>
        </label>
      </span>

      {#if tab.view === "rows"}
        <span class="flex flex-wrap items-center gap-1.5">
          <!--
            Estos actúan sobre el resultado en vivo —el techo de filas, sus tipos, exportarlo—, así
            que mirando una pestaña anclada no pintan nada: es una foto ya tomada, no algo que se
            vuelva a ejecutar.
          -->
          {#if tab.pinnedShown === null}
            <!-- Cuántas filas se traen de una. Es la misma preferencia que usa la grilla de datos
                 para su tanda: acá no hay scroll que pida la siguiente, así que sube el techo y se
                 vuelve a ejecutar. -->
            <!-- Los tipos no vienen con las filas: pedirlos cuesta preparar la sentencia de nuevo en
                 el servidor, así que es un interruptor y no algo que pase siempre. -->
            <button
              class="btn btn-ghost btn-icon btn-toggle"
              aria-pressed={tab.showTypes}
              aria-label="Tipos de las columnas"
              disabled={tab.running}
              title="Muestra el tipo de cada columna; se le pregunta al servidor sin ejecutar de nuevo"
              onclick={() => tab.setShowTypes(!tab.showTypes)}
            >
              <Icon name="type" size={13} />
            </button>

            <!-- Editar el resultado: solo si sale de una sola tabla y trae su clave. Se le pregunta al
                 servidor al apretar, no en cada ejecución. -->
            <button
              class="btn btn-ghost btn-icon btn-toggle"
              aria-pressed={tab.editPlan !== null}
              aria-label="Editar el resultado"
              disabled={!tab.canTryEdit || tab.editBusy}
              title={tab.editPlan
                ? "Salir de la edición (descarta lo no guardado)"
                : "Editar las celdas del resultado cuando sale de una sola tabla"}
              onclick={() => (tab.editPlan ? tab.cancelEdit() : tab.startEdit())}
            >
              <Icon name="edit" size={13} />
            </button>

            <!-- Vigilar: repite la última lectura cada tanto. Solo se ofrece si repetirla no cambia
                 nada (ver `watch-guard.ts`). -->
            <label
              class="flex items-center gap-1 text-xs muted"
              title={tab.canWatch
                ? "Repite la consulta cada tanto mientras la ventana está a la vista"
                : "Solo se puede vigilar una lectura simple, con autocommit encendido"}
            >
              <Icon name="eye" size={12} />
              <select
                class="field py-0.5 text-xs"
                disabled={!tab.canWatch && tab.watchSeconds === null}
                value={tab.watchSeconds ?? 0}
                onchange={(event) => {
                  const seconds = Number(event.currentTarget.value);
                  if (seconds === 0) tab.stopWatch();
                  else tab.startWatch(seconds);
                }}
              >
                <option value={0}>Vigilar: no</option>
                {#each WATCH_STEPS as seconds (seconds)}
                  <option value={seconds}>cada {seconds} s</option>
                {/each}
              </select>
            </label>

            <!-- Tiempo máximo por sentencia de esta pestaña. «Del perfil» vuelve al límite con que se
                 abrió la sesión. -->
            <label
              class="flex items-center gap-1 text-xs muted"
              title="Cancela solo cualquier sentencia de esta pestaña que pase de este tiempo"
            >
              <Icon name="clock" size={12} />
              <select
                class="field py-0.5 text-xs"
                disabled={!tab.tabId || tab.running}
                value={String(tab.timeoutMs)}
                onchange={(event) => {
                  const raw = event.currentTarget.value;
                  void tab.setTimeoutMs(raw === "null" ? null : Number(raw));
                }}
              >
                {#each TIMEOUT_OPTIONS as option (option.label)}
                  <option value={String(option.ms)}>{option.label}</option>
                {/each}
              </select>
            </label>

            <TimeZoneSelect />

            <label
              class="flex items-center gap-1 text-xs muted"
              title="Máximo de filas que se traen; para ver más, subilo y volvé a ejecutar"
            >
              <span>Máx.</span>
              <select
                class="field py-0.5 text-xs"
                value={paging.size}
                onchange={(event) => paging.set(Number(event.currentTarget.value))}
              >
                {#each PAGE_SIZES as size (size)}
                  <option value={size}>{size}</option>
                {/each}
              </select>
            </label>

            <!--
              A diferencia de Exportar, tiene sentido también con un resultado sin filas —un
              `UPDATE`, anclado como constancia de qué se corrió—, así que va afuera del
              `{#if withRows}`.
            -->
            <button
              class="btn btn-ghost btn-icon"
              disabled={!tab.result}
              aria-label="Anclar el resultado"
              title="Ancla este resultado como una pestaña más, que no se pisa con la próxima ejecución"
              onclick={pinCurrent}
            >
              <Icon name="pin" size={13} />
            </button>

            <!--
              Exportar no manda las filas que están en pantalla: vuelve a correr la consulta que
              corresponde al resultado elegido con un `COPY … TO STDOUT`, así el archivo tiene todas
              las filas y no las que entraron en el techo. Con más de un resultado, el texto exacto
              de esa sentencia se lo pregunta al núcleo (`ResultSet.offset`, ver «Pestañas de varios
              resultados» arriba): no alcanza con el script entero, que no es una consulta que
              `COPY` pueda envolver.
            -->
            {#if withRows}
              <button
                class="btn btn-ghost btn-icon"
                disabled={tab.ranSql.trim() === ""}
                aria-label="Exportar el resultado"
                title="Exporta todas las filas del resultado elegido, no solo las que se muestran"
                onclick={openExport}
              >
                <Icon name="download" size={13} />
              </button>
              <button
                class="btn btn-ghost btn-icon"
                aria-label="Exportar a JSON o Markdown"
                title="Escribe las filas cargadas como JSON o Markdown (respeta el máximo de filas)"
                onclick={() => (fileExportOpen = true)}
              >
                <Icon name="sql" size={13} />
              </button>
            {/if}

          {/if}

          <!-- Fuera del bloque de arriba: comparar contra un ancla se hace mirando el ancla. -->
          <button
            class="btn btn-ghost btn-icon"
            disabled={compareSources.length < 2}
            aria-label="Comparar resultados"
            title={compareSources.length < 2
              ? "Hacen falta dos resultados con filas: anclá uno y ejecutá de nuevo"
              : "Compara dos resultados celda por celda"}
            onclick={() => (compareOpen = true)}
          >
            <Icon name="compare" size={13} />
          </button>

          <!-- El tamaño de letra de la grilla va al final: se toca una vez y no se vuelve a mirar,
               al revés de todo lo que tiene a la izquierda. Y va afuera del `{#if}` de arriba: la
               letra se lee igual mirando un resultado anclado. -->
          <span class="toolbar-sep"></span>
          <GridSize />
        </span>
      {/if}
    </div>

    <!--
      La tira de resultados: una pestaña por cada sentencia en vivo del último script (cuando
      devolvió más de una) más una por cada resultado anclado. Un ancla funciona igual que las demás
      —se elige con un clic, tiene su cruz para sacarla— y no como un panel aparte al costado: dos
      grillas a la vez solo tiene sentido con el panel dividido de la ventana, que es otra cosa.
    -->
    {#if tab.view === "rows" && (tab.results.length > 1 || tab.pinned.length > 0)}
      <div class="divider-b flex items-stretch gap-px overflow-x-auto px-1 py-1" role="tablist">
        {#each tab.results as item, index (index)}
          {@const label = resultLabel(item)}
          <div class="tab-wrap">
            <button
              class="tab"
              role="tab"
              aria-selected={tab.pinnedShown === null && tab.shown === index}
              title={label.hint}
              onclick={() => tab.showLive(index)}
            >
              {label.title}
              <span class="seg-count">{label.detail}</span>
            </button>
          </div>
        {/each}

        {#each tab.pinned as item (item.id)}
          <div class="tab-wrap">
            <button
              class="tab pr-1"
              role="tab"
              aria-selected={tab.pinnedShown === item.id}
              title={item.sql}
              onclick={() => tab.showPinned(item.id)}
            >
              <Icon name="pin" size={10} class="text-blue-600 dark:text-blue-400" />
              {item.label}
            </button>
            <button
              class="tab-close"
              aria-label="Sacar el ancla"
              title="Sacar el ancla"
              onclick={() => tab.unpin(item.id)}
            >
              <Icon name="close" size={10} />
            </button>
          </div>
        {/each}
      </div>
    {/if}

    {#snippet resultBody(outcome: Outcome, columnTypes: string[] | null)}
      {#if outcome.kind === "rows"}
        <ResultGrid columns={outcome.columns} rows={outcome.rows} types={columnTypes} />
      {:else}
        <Empty
          icon="check"
          title="{outcome.tag} · {count(outcome.affected)} {outcome.affected === 1
            ? 'fila'
            : 'filas'}"
          hint="La sentencia no devuelve filas; el servidor informó cuántas tocó."
        />
      {/if}
    {/snippet}

    <div class="relative min-h-0 flex-1 {editorSplit.resultsHidden ? 'hidden' : ''}">
      <!--
        Mientras corre, el estado vacío decía «todavía no ejecutaste nada», que es exactamente lo
        contrario de lo que está pasando. Va por encima y no en lugar de la grilla: al volver a
        ejecutar, tapar el resultado anterior con una pantalla vacía hace parpadear todo.
      -->
      {#if tab.running && tab.view === "messages"}
        <!-- En «Mensajes» no se tapa nada: ahí es donde van llegando los RAISE NOTICE mientras
             corre. Solo queda una tira chica con el cancelar. -->
        <div
          class="absolute top-2 right-4 z-10 flex items-center gap-2 rounded-md border border-zinc-200
                 bg-white/90 px-2 py-1 shadow-sm dark:border-zinc-700 dark:bg-zinc-900/90"
        >
          <span class="spinner size-3.5"></span>
          <span class="text-xs muted">Ejecutando… <Elapsed since={tab.runStartedAt} /></span>
          {#if tab.waiting}
            <span
              class="truncate text-xs {tab.waiting.blocked ? 'text-amber-700 dark:text-amber-400' : 'muted'}"
              title={tab.waiting.detail ?? tab.waiting.summary}>{tab.waiting.summary}</span
            >
          {/if}
          <button class="btn btn-danger btn-sm" onclick={() => tab.cancel()}>
            <Icon name="close" size={11} />
            Cancelar
          </button>
        </div>
      {:else if tab.running}
        <div
          class="absolute inset-0 z-10 flex flex-col items-center justify-center gap-3
                 bg-white/70 backdrop-blur-[1px] dark:bg-zinc-950/70"
        >
          <span class="spinner size-6 border-2"></span>
          <p class="text-sm muted">Ejecutando… <Elapsed since={tab.runStartedAt} /></p>
          {#if tab.waiting}
            <div class="max-w-md px-4 text-center text-xs {tab.waiting.blocked ? 'text-amber-700 dark:text-amber-400' : 'muted'}">
              <p>{tab.waiting.summary}</p>
              {#if tab.waiting.detail}
                <p class="mt-1 truncate font-mono muted" title={tab.waiting.detail}>{tab.waiting.detail}</p>
              {/if}
            </div>
          {/if}
          <button class="btn btn-danger btn-sm" onclick={() => tab.cancel()}>
            <Icon name="close" size={11} />
            Cancelar
          </button>
        </div>
      {/if}

      {#if tab.view === "rows"}
        {#if pinnedResult}
          <!-- Una pestaña anclada se mira igual que una en vivo: una sola grilla a la vez, elegida
               en la tira de arriba. -->
          {#key pinnedResult.id}
            {@render resultBody(pinnedResult.outcome, pinnedResult.columnTypes)}
          {/key}
        {:else if withRows}
          {#if tab.editError}
            <Alert tone="warn" box class="m-2">{tab.editError}</Alert>
          {/if}
          {#if tab.editPlan}
            <div class="divider-b flex flex-wrap items-center gap-2 px-3 py-1 text-xs">
              <span>
                Editando <b>{tab.editPlan.shape.schema}.{tab.editPlan.shape.name}</b>
                · {tab.editedRowCount}
                {tab.editedRowCount === 1 ? "fila con cambios" : "filas con cambios"}
              </span>
              <span class="muted">Doble clic en una celda · nada se escribe hasta guardar</span>
              <span class="ml-auto flex gap-1.5">
                <button class="btn btn-sm" onclick={() => tab.cancelEdit()}>Descartar</button>
                <button
                  class="btn btn-sm"
                  disabled={tab.editedRowCount === 0 || tab.editBusy}
                  onclick={() => tab.showEditPreview()}>Ver SQL</button
                >
                <button
                  class="btn btn-primary btn-sm"
                  disabled={tab.editedRowCount === 0 || tab.editBusy}
                  onclick={() => tab.saveEdit()}
                >
                  {#if tab.editBusy}<span class="spinner"></span>{/if}
                  Guardar
                </button>
              </span>
            </div>
          {/if}
          {#key result}
            <ResultGrid
              columns={withRows.columns}
              rows={withRows.rows}
              types={tab.showTypes ? tab.columnTypes : null}
              editing={tab.editPlan
                ? {
                    editable: tab.editPlan.editable,
                    edits: tab.edits,
                    onedit: (row, column, value) => tab.setEdit(row, column, value),
                  }
                : null}
            />
          {/key}
        {:else if result && result.outcome.kind === "command"}
          <Empty
            icon="check"
            title="{result.outcome.tag} · {count(result.outcome.affected)} {result.outcome
              .affected === 1
              ? 'fila'
              : 'filas'}"
            hint="La sentencia no devuelve filas; el servidor informó cuántas tocó."
          />
        {:else}
          <Empty
            icon="sql"
            title="Todavía no ejecutaste nada"
            hint="Escribí una consulta y ejecutala. Si hay varias sentencias, se ejecuta la del cursor; si hay una selección, se ejecuta la selección."
          >
            <button
              class="btn btn-primary"
              disabled={tab.tabId === null || tab.sql.trim() === ""}
              onclick={() => {
                const { text, cursor } = here();
                run(text, cursor);
              }}
            >
              <Icon name="play" size={12} /> Ejecutar
            </button>
            <button class="btn" onclick={() => (tab.view = "history")}>Ver el historial</button>
            <button class="btn" onclick={() => (tab.view = "saved")}>Consultas guardadas</button>
            <p class="basis-full pt-1 text-[11px] muted">
              <kbd class="kbd">{shortcutStore.label("editor.run")}</kbd> la sentencia ·
              <kbd class="kbd">{shortcutStore.label("editor.runScript")}</kbd> el script ·
              <kbd class="kbd">{shortcutStore.label("app.tabs")}</kbd> pestañas
            </p>
          </Empty>
        {/if}
      {:else if tab.view === "plan"}
        {#if tab.plan}
          <div class="h-full overflow-auto p-2">
            <div class="mb-2 flex items-center gap-1.5">
              <span class="text-xs font-medium">
                {tab.plan.advice.length > 0
                  ? `${tab.plan.advice.length} ${tab.plan.advice.length === 1 ? "sugerencia" : "sugerencias"}`
                  : "Plan"}
              </span>
              <span class="ml-auto"></span>
              <button
                class="btn btn-sm"
                title="Copia el plan como texto, con la forma de psql"
                onclick={() => copyPlan("text")}
              >
                <Icon name={planCopied === "text" ? "check" : "copy"} size={11} />
                {planCopied === "text" ? "Copiado" : "Copiar texto"}
              </button>
              <button
                class="btn btn-sm"
                title="Copia el JSON tal como lo devolvió el servidor, para pegarlo en un visor de planes"
                onclick={() => copyPlan("json")}
              >
                <Icon name={planCopied === "json" ? "check" : "copy"} size={11} />
                {planCopied === "json" ? "Copiado" : "Copiar JSON"}
              </button>
            </div>

            <div class="mb-3">
              <PlanAdvice
                advice={tab.plan.advice}
                analyzed={tab.plan.analyzed}
                oncreateIndex={openIndexDialog}
              />
            </div>

            <PlanTree node={tab.plan.root} heat={heat ?? planHeat(tab.plan.root)} />
            <p class="mt-3 flex flex-wrap gap-x-3 px-2 text-xs muted">
              {#if tab.plan.planningMs !== null}
                <span>planificación {decimal(tab.plan.planningMs, 2)} ms</span>
              {/if}
              {#if tab.plan.executionMs !== null}
                <span>ejecución {decimal(tab.plan.executionMs, 2)} ms</span>
              {/if}
              {#if !tab.plan.analyzed}
                <span class="tag tag-neutral">plan estimado, sin ejecutar</span>
              {/if}
            </p>
          </div>
        {:else}
          <Empty
            icon="compass"
            title="Sin plan"
            hint="«Explicar» muestra cómo resolvería PostgreSQL la consulta sin ejecutarla; «Explicar y medir» la ejecuta y compara lo estimado con lo real."
          >
            <button
              class="btn btn-primary"
              disabled={tab.tabId === null || tab.running || tab.sql.trim() === ""}
              onclick={() => {
                const { text, cursor } = here();
                explain(text, cursor, estimate);
              }}
            >
              <Icon name="plan" size={12} /> Explicar
            </button>
            <button
              class="btn"
              disabled={tab.tabId === null || tab.running || tab.sql.trim() === ""}
              onclick={() => {
                const { text, cursor } = here();
                explain(text, cursor, analyze);
              }}
            >
              <Icon name="gauge" size={12} /> Explicar y medir
            </button>
          </Empty>
        {/if}
      {:else if tab.view === "history"}
        <HistoryPanel
          profileId={tab.profileId}
          onpick={(sql) => {
            tab.sql = sql;
            tab.view = "rows";
          }}
        />
      {:else if tab.view === "saved"}
        <SavedPanel reload={savedStamp} onpick={(saved) => tab.applySaved(saved)} />
      {:else if tab.messages.length === 0}
        <Empty
          icon="info"
          title="Sin mensajes"
          hint="Acá aparecen los avisos del servidor (NOTICE, WARNING) y los errores de cada sentencia."
        />
      {:else}
        <ul class="h-full overflow-auto p-2 font-mono text-xs select-text">
          {#each tab.messages as message, index (index)}
            <li
              class="flex items-start gap-2 rounded px-2 py-1 {message.error
                ? 'text-zinc-800 dark:text-zinc-200'
                : message.tone === 'error'
                ? 'text-rose-600 dark:text-rose-400'
                : message.tone === 'notice'
                  ? 'text-amber-700 dark:text-amber-400'
                  : 'muted'}"
            >
              {#if message.error}
                <ErrorCard error={message.error} ongoto={(at) => editor?.goTo(at)} />
              {:else}
                {#if message.tone !== "info"}
                  <Icon name="warn" size={12} class="mt-0.5 shrink-0" />
                {/if}
                <span class="min-w-0 flex-1 break-words whitespace-pre-wrap">{message.text}</span>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>
</div>

{#if compareOpen}
  <ResultCompareDialog
    sources={compareSources}
    initialLeft={compareFrom}
    onclose={() => (compareOpen = false)}
  />
{/if}

{#if fileExportOpen && withRows}
  <ResultFileDialog
    columns={withRows.columns}
    rows={withRows.rows}
    types={tab.showTypes ? tab.columnTypes : null}
    truncated={withRows.truncated}
    onclose={() => (fileExportOpen = false)}
  />
{/if}

{#if exportOpen}
  <ExportDialog
    profileId={tab.profileId}
    database={tab.database}
    source={{ kind: "query", sql: exportSql }}
    onclose={() => (exportOpen = false)}
  />
{/if}

{#if picking}
  <ObjectPicker
    profileId={tab.profileId}
    database={tab.database}
    onpick={insertPicked}
    onclose={() => {
      picking = false;
      editor?.focus();
    }}
  />
{/if}

{#if snippetsOpen || snippetDraft}
  <SnippetDialog
    draft={snippetDraft ?? undefined}
    profileId={tab.profileId}
    database={tab.database}
    onclose={() => {
      snippetsOpen = false;
      snippetDraft = null;
      editor?.focus();
    }}
  />
{/if}

{#if formatOptionsOpen}
  <FormatOptionsDialog onclose={() => (formatOptionsOpen = false)} />
{/if}

{#if versionsOpen}
  <!-- Restaurar asigna `tab.sql`: el editor aplica el reemplazo como un solo cambio, así que
       Ctrl+Z lo deshace. -->
  <VersionsDialog
    {tab}
    onrestore={(text) => (tab.sql = text)}
    onclose={() => {
      versionsOpen = false;
      editor?.focus();
    }}
  />
{/if}

{#if saveOpen}
  <SaveQueryDialog
    {tab}
    onsaved={() => (savedStamp += 1)}
    onclose={() => (saveOpen = false)}
  />
{/if}

{#if railMenu}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-40" onclick={() => (railMenu = null)} oncontextmenu={() => (railMenu = null)}></div>
  <div
    class="fixed z-50 flex min-w-60 flex-col rounded-md border border-zinc-200 bg-white py-1 shadow-lg
           dark:border-zinc-700 dark:bg-zinc-900"
    style="right: {railMenu.x}px; top: {railMenu.y}px"
    role="menu"
  >
    {#if railMenu.kind === "analyze"}
      <button
        class="row-menu"
        role="menuitem"
        disabled={tab.running}
        title="Muestra el plan estimado sin ejecutar la consulta"
        onclick={() =>
          fromMenu(() => {
            const { text, cursor } = here();
            explain(text, cursor, estimate);
          })}
      >
        <Icon name="plan" size={13} /> Explicar (plan estimado)
      </button>
      <button
        class="row-menu"
        role="menuitem"
        disabled={tab.running}
        title="Ejecuta la consulta y muestra los tiempos reales"
        onclick={() =>
          fromMenu(() => {
            const { text, cursor } = here();
            explain(text, cursor, analyze);
          })}
      >
        <Icon name="gauge" size={13} /> Explicar y medir (ejecuta)
      </button>
      <button
        class="row-menu"
        role="menuitem"
        disabled={tab.running || tab.rehearsing || tab.txStatus !== "idle"}
        title={tab.txStatus !== "idle"
          ? "Hay una transacción abierta: confirmala o revertila para ensayar"
          : "Lo corre adentro de BEGIN … ROLLBACK y muestra filas afectadas, errores y locks, sin dejar nada aplicado"}
        onclick={() => fromMenu(() => rehearse(here().text))}
      >
        <Icon name="flask" size={13} /> Ensayar (sin aplicar)
      </button>
    {:else}
      <button
        class="row-menu"
        role="menuitem"
        title={tab.filePath
          ? `Guarda en ${tab.filePath}`
          : "Guarda el texto como archivo .sql"}
        onclick={() => fromMenu(() => saveQueryTab(tab, false))}
      >
        <Icon name="save" size={13} /> Guardar como archivo .sql
        <span class="kbd ml-auto">{shortcutStore.label("app.save")}</span>
      </button>
      <button
        class="row-menu"
        role="menuitem"
        title={tab.savedName
          ? `Guarda los cambios en «${tab.savedName}»`
          : "Guarda la consulta con un nombre para volver a abrirla"}
        onclick={() => fromMenu(() => (saveOpen = true))}
      >
        <Icon name="star" size={13} /> Guardar con un nombre
      </button>
      <button
        class="row-menu"
        role="menuitem"
        title="Ordena la selección si hay una, si no el documento entero"
        onclick={() =>
          fromMenu(() => {
            const { text, cursor } = here();
            doFormat(text, cursor);
          })}
      >
        <Icon name="format" size={13} /> Formatear el SQL
        <span class="kbd ml-auto">{shortcutStore.label("editor.format")}</span>
      </button>
      <button
        class="row-menu"
        role="menuitem"
        title="Mayúsculas de las palabras clave, sangría, coma al principio y líneas entre sentencias"
        onclick={() => fromMenu(() => (formatOptionsOpen = true))}
      >
        <Icon name="sliders" size={13} /> Opciones del formateador…
      </button>
    {/if}
  </div>
{/if}

{#if tab.editPreview}
  <Modal
    title="Esto es lo que se va a ejecutar"
    subtitle="Van en una sola transacción: si alguna falla, no se aplica ninguna."
    size="xl"
    busy={tab.editBusy}
    onclose={() => (tab.editPreview = null)}
  >
    {#each tab.editPreview as statement, index (index)}
      <div class="mb-2 rounded-md border border-zinc-200 p-2 dark:border-zinc-700">
        <pre class="font-mono text-xs whitespace-pre-wrap select-text">{statement.sql}</pre>
        {#if statement.params.length > 0}
          <div class="mt-1 flex flex-wrap gap-1.5 text-[11px] muted">
            {#each statement.params as param, position (position)}
              <span class="rounded bg-zinc-100 px-1.5 py-px font-mono dark:bg-zinc-700">
                ${position + 1} = {param === null ? "NULL" : param}
              </span>
            {/each}
          </div>
        {/if}
      </div>
    {/each}

    {#snippet footer()}
      <button class="btn ml-auto" onclick={() => (tab.editPreview = null)}>Cerrar</button>
      <button
        class="btn btn-primary"
        disabled={tab.editBusy}
        onclick={() => {
          tab.editPreview = null;
          void tab.saveEdit();
        }}>Guardar</button
      >
    {/snippet}
  </Modal>
{/if}

{#if sizeAsk}
  {@const ask = sizeAsk}
  <SizeDecision
    table={ask.table}
    rows={ask.rows}
    limit={ask.limit}
    onchoose={(choice) => {
      ask.resolve(choice);
      sizeAsk = null;
    }}
  />
{/if}

{#if varsAsk}
  <SnippetVarsDialog
    title="Valores de la consulta"
    description={"Cada {{nombre}} se reemplaza por lo que escribas, tal cual: si va entre comillas en la consulta, no las repitas."}
    submitLabel="Ejecutar"
    variables={varsAsk.variables}
    onsubmit={(values) => varsAsk?.resolve(values)}
    onclose={() => {
      varsAsk?.resolve(null);
      varsAsk = null;
    }}
  />
{/if}

{#if askAuto}
  <TxDecision
    title="Hay una transacción abierta"
    message="Encender el autocommit no confirma ni revierte lo que ya hiciste en esta transacción. ¿Qué hacemos con ella?"
    keepLabel="Dejarla abierta"
    busy={deciding}
    onchoose={decideAuto}
    onclose={() => (askAuto = false)}
  />
{/if}

{#if switching}
  <Confirm
    title="Hay una transacción abierta"
    message="Cambiar de base cierra la sesión de esta pestaña, y con ella se revierte todo lo que no
             se haya confirmado. El texto del editor queda como está."
    confirmLabel="Cambiar igual"
    onconfirm={() => {
      const database = switching;
      switching = null;
      if (database) tab.switchDatabase(database);
    }}
    onclose={() => (switching = null)}
  />
{/if}

{#if newIndex}
  <IndexDialog
    profileId={tab.profileId}
    database={tab.database}
    schema={newIndex.target.schema}
    table={newIndex.target.table}
    columns={newIndex.columns}
    initialColumns={newIndex.target.columns}
    onclose={() => (newIndex = null)}
    oncreated={() => tab.log("info", "El índice quedó creado.")}
  />
{/if}

{#if pending}
  <Confirm
    title="Esto ejecuta la sentencia"
    message={pending.warning}
    confirmLabel="Ejecutar igual"
    onconfirm={() => {
      const target = pending;
      pending = null;
      if (target) tab.explain(target.sql, target.base, target.options);
    }}
    onclose={() => (pending = null)}
  />
{/if}

{#if tab.rehearsalOpen}
  <Modal title="Ensayo" subtitle="BEGIN … ROLLBACK: no queda nada aplicado" size="xl" busy={tab.rehearsing} onclose={() => (tab.rehearsalOpen = false)}>
    {#if tab.rehearsing}
      <p class="muted flex items-center gap-2 text-sm"><span class="spinner"></span> Ensayando…</p>
    {:else if tab.rehearsalError}
      <Alert box tone="bad">{tab.rehearsalError}</Alert>
    {:else if tab.rehearsal}
      <RehearsalResult report={tab.rehearsal} />
    {/if}

    {#snippet footer()}
      <button class="btn ml-auto" disabled={tab.rehearsing} onclick={() => (tab.rehearsalOpen = false)}>Cerrar</button>
    {/snippet}
  </Modal>
{/if}
