/**
 * Diagnósticos dentro del editor: el error que el servidor encuentra al revisar la sentencia sin
 * ejecutarla, y los avisos de `sql-lint`.
 *
 * Son **dos fuentes separadas** con su propio ciclo: la revisión del servidor tarda una vuelta y
 * puede no correr (transacción abierta, pestaña ocupada), el aviso local es instantáneo. Cada una
 * reemplaza solo lo suyo, así que un aviso que llega no borra el error del servidor ni al revés.
 *
 * Como el subrayado del error de ejecución (`sql-error-mark`), viven en un campo de estado que se
 * **mapea** con cada cambio del documento: un rango instalado una vez queda clavado en un
 * desplazamiento que el próximo borrado deja afuera del texto.
 *
 * La revisión es una ayuda, jamás un cartel: `skipped`, un error de red o una desconexión no pintan
 * nada y no muestran ningún mensaje. El error real de la ejecución sigue siendo el de
 * `errorMarkField`, que convive con esto.
 */

import { StateEffect, StateField, type ChangeDesc, type Extension } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  GutterMarker,
  ViewPlugin,
  gutter,
  hoverTooltip,
  type ViewUpdate,
} from "@codemirror/view";
import type { CheckResult } from "../ipc";
import { lintStatement } from "../sql-lint";
import { toEditorIndex } from "../sql-error-mark";

export type Severity = "error" | "warning" | "info";
export type Source = "server" | "lint";

export interface Diagnostic {
  from: number;
  to: number;
  severity: Severity;
  message: string;
  hint?: string | null;
}

const RANK: Record<Severity, number> = { info: 0, warning: 1, error: 2 };

export const worstSeverity = (items: Diagnostic[]): Severity | null =>
  items.reduce<Severity | null>(
    (worst, item) => (worst === null || RANK[item.severity] > RANK[worst] ? item.severity : worst),
    null,
  );

/**
 * Lleva los rangos al documento nuevo. Un rango que se queda sin texto (se borró lo que subrayaba)
 * se va con él: es lo que uno espera al corregir el error.
 */
export function mapDiagnostics(items: Diagnostic[], changes: ChangeDesc): Diagnostic[] {
  const out: Diagnostic[] = [];
  for (const item of items) {
    const from = changes.mapPos(item.from, 1);
    const to = changes.mapPos(item.to, -1);
    if (to > from) out.push({ ...item, from, to });
  }
  return out;
}

/**
 * El error del servidor como diagnóstico.
 *
 * `position` cuenta caracteres Unicode desde el principio de lo que se mandó (base 0) y el editor
 * cuenta unidades UTF-16: se convierte con las mismas utilidades que el error de ejecución. Sin
 * posición el servidor no dijo dónde, y se subraya la primera palabra de la sentencia en vez de
 * esconder el problema.
 *
 * `statementStart` es el índice del editor donde empieza lo enviado.
 */
export function serverDiagnostic(
  sent: string,
  statementStart: number,
  result: CheckResult,
): Diagnostic | null {
  if (result.status !== "error") return null;

  const local = result.position === null ? 0 : toEditorIndex(sent, result.position);
  if (local >= sent.length) return null;

  // La palabra entera se ve mejor que un solo carácter. Sin posición no se sabe qué es: la primera.
  let end = local + 1;
  while (end < sent.length && /[\w$."]/.test(sent[end])) end += 1;

  return {
    from: statementStart + local,
    to: statementStart + end,
    severity: "error",
    message: result.message,
    hint: result.hint,
  };
}

/** Los avisos locales de una sentencia como diagnósticos. */
export function lintDiagnostics(text: string, statementStart: number): Diagnostic[] {
  return lintStatement(text, statementStart).map((finding) => ({
    from: finding.from,
    to: finding.to,
    severity: finding.severity,
    message: finding.message,
  }));
}

// ---------------------------------------------------------------------------------------------
// Estado
// ---------------------------------------------------------------------------------------------

interface Diagnostics {
  server: Diagnostic[];
  lint: Diagnostic[];
}

/** Reemplaza lo de una fuente; una lista vacía la limpia. */
export const setDiagnostics = StateEffect.define<{ source: Source; items: Diagnostic[] }>();

export const diagnosticsField = StateField.define<Diagnostics>({
  create: () => ({ server: [], lint: [] }),
  update(value, tr) {
    let next = value;
    if (tr.docChanged) {
      next = {
        server: mapDiagnostics(next.server, tr.changes),
        lint: mapDiagnostics(next.lint, tr.changes),
      };
    }
    for (const effect of tr.effects) {
      if (effect.is(setDiagnostics)) {
        // Un diagnóstico fuera del documento rompería cada transacción siguiente.
        const length = tr.newDoc.length;
        const items = effect.value.items.filter((d) => d.from >= 0 && d.to <= length && d.to > d.from);
        next = { ...next, [effect.value.source]: items };
      }
    }
    return next;
  },
  provide: (field) =>
    EditorView.decorations.compute([field], (state) => {
      const { server, lint } = state.field(field);
      // Primero el error: con rangos iguales el más grave queda adentro y gana el color.
      const all = [...lint, ...server].sort((a, b) => a.from - b.from || RANK[a.severity] - RANK[b.severity]);
      return Decoration.set(
        all.map((d) =>
          Decoration.mark({ class: `cm-diag cm-diag-${d.severity}` }).range(d.from, d.to),
        ),
        true,
      );
    }),
});

/** Todos los diagnósticos vigentes, del más grave al más leve. */
export function allDiagnostics(diagnostics: Diagnostics): Diagnostic[] {
  return [...diagnostics.server, ...diagnostics.lint].sort(
    (a, b) => RANK[b.severity] - RANK[a.severity],
  );
}

// ---------------------------------------------------------------------------------------------
// Margen y globo
// ---------------------------------------------------------------------------------------------

class DiagnosticMarker extends GutterMarker {
  constructor(readonly severity: Severity) {
    super();
  }
  eq(other: DiagnosticMarker) {
    return other.severity === this.severity;
  }
  toDOM() {
    const dot = document.createElement("span");
    dot.className = `cm-diag-dot cm-diag-dot-${this.severity}`;
    return dot;
  }
}

const markers: Record<Severity, DiagnosticMarker> = {
  error: new DiagnosticMarker("error"),
  warning: new DiagnosticMarker("warning"),
  info: new DiagnosticMarker("info"),
};

const diagnosticGutter = gutter({
  class: "cm-diag-gutter",
  lineMarker(view, line) {
    const here = allDiagnostics(view.state.field(diagnosticsField)).filter(
      (d) => d.from <= line.to && d.to >= line.from,
    );
    const worst = worstSeverity(here);
    return worst ? markers[worst] : null;
  },
  lineMarkerChange: (update) =>
    update.transactions.some((tr) => tr.effects.some((e) => e.is(setDiagnostics))) || update.docChanged,
});

const diagnosticHover = hoverTooltip(
  (view, pos, side) => {
    const found = allDiagnostics(view.state.field(diagnosticsField)).filter(
      (d) => d.from <= pos && d.to >= pos && !(d.to === pos && side > 0),
    );
    if (found.length === 0) return null;

    return {
      pos: Math.min(...found.map((d) => d.from)),
      end: Math.max(...found.map((d) => d.to)),
      above: true,
      create: () => {
        const dom = document.createElement("div");
        dom.className = "cm-diag-tooltip";
        for (const d of found) {
          const row = document.createElement("div");
          row.className = `cm-diag-message cm-diag-message-${d.severity}`;
          row.textContent = d.message;
          dom.appendChild(row);
          if (d.hint) {
            const hint = document.createElement("div");
            hint.className = "cm-diag-hint";
            hint.textContent = d.hint;
            dom.appendChild(hint);
          }
        }
        return { dom };
      },
    };
  },
  { hideOnChange: true },
);

const diagnosticTheme = EditorView.baseTheme({
  ".cm-diag": { textUnderlineOffset: "3px", textDecorationThickness: "1px" },
  ".cm-diag-error": { textDecoration: "underline wavy var(--cm-error)" },
  ".cm-diag-warning": { textDecoration: "underline wavy var(--cm-warning)" },
  ".cm-diag-info": { textDecoration: "underline dotted var(--cm-info)" },
  ".cm-diag-gutter": { width: "10px" },
  ".cm-diag-dot": {
    display: "block",
    width: "6px",
    height: "6px",
    borderRadius: "50%",
    margin: "0 auto",
    transform: "translateY(0.55em)",
  },
  ".cm-diag-dot-error": { backgroundColor: "var(--cm-error)" },
  ".cm-diag-dot-warning": { backgroundColor: "var(--cm-warning)" },
  ".cm-diag-dot-info": { backgroundColor: "var(--cm-info)" },
  ".cm-diag-tooltip": { padding: "6px 8px", maxWidth: "420px", fontSize: "12px" },
  ".cm-diag-message + .cm-diag-message, .cm-diag-hint + .cm-diag-message": { marginTop: "6px" },
  ".cm-diag-message-error": { color: "var(--cm-error)" },
  ".cm-diag-message-warning": { color: "var(--cm-warning)" },
  ".cm-diag-hint": { marginTop: "2px", color: "var(--cm-gutter)" },
});

// ---------------------------------------------------------------------------------------------
// Quién revisa y cuándo
// ---------------------------------------------------------------------------------------------

export interface DiagnosticsDeps {
  /** La sentencia del cursor, según el núcleo (el mismo que usa «ejecutar»). `offset` en caracteres. */
  statementAt: (text: string, cursor: number) => Promise<{ text: string; offset: number } | null>;
  /** Revisa con el servidor sin ejecutar. Puede rechazar: no se muestra. */
  check: (sql: string) => Promise<CheckResult>;
  /** La pestaña puede revisar ahora: no está ejecutando, no hay transacción abierta, hay sesión. */
  canCheck: () => boolean;
  checkEnabled: () => boolean;
  lintEnabled: () => boolean;
}

/** Pausa antes de resolver la sentencia y avisar; y pausa extra, ya sumada, antes de ir al servidor. */
export const LINT_DELAY_MS = 300;
export const CHECK_DELAY_MS = 700;

/** `{{nombre}}` es una variable de pgForge que el servidor no entiende: revisarla daría un error falso. */
export const hasVariables = (sql: string) => /\{\{[^{}]+\}\}/.test(sql);

class DiagnosticsPlugin {
  private timer: ReturnType<typeof setTimeout> | undefined;
  private checkTimer: ReturnType<typeof setTimeout> | undefined;
  private serial = 0;
  /** El rango de la última sentencia revisada: mientras el cursor siga adentro no hay que repetir. */
  private range: { from: number; to: number } | null = null;

  constructor(
    private view: EditorView,
    private deps: DiagnosticsDeps,
  ) {
    this.schedule();
  }

  update(update: ViewUpdate) {
    if (update.docChanged) {
      this.schedule();
      return;
    }
    if (update.selectionSet) {
      // Mover el cursor adentro de la misma sentencia no cambia nada de lo que se mostró.
      const head = update.state.selection.main.head;
      if (!this.range || head < this.range.from || head > this.range.to) this.schedule();
    }
  }

  destroy() {
    clearTimeout(this.timer);
    clearTimeout(this.checkTimer);
    this.serial += 1;
  }

  private publish(source: Source, items: Diagnostic[]) {
    // Con la vista ya destruida (se cerró la pestaña a mitad de la revisión) no hay a quién avisar.
    if (!this.view.dom.isConnected) return;
    const current = this.view.state.field(diagnosticsField)[source];
    if (items.length === 0 && current.length === 0) return;
    this.view.dispatch({ effects: setDiagnostics.of({ source, items }) });
  }

  private clearAll() {
    this.range = null;
    this.publish("server", []);
    this.publish("lint", []);
  }

  private schedule() {
    clearTimeout(this.timer);
    clearTimeout(this.checkTimer);
    const serial = ++this.serial;
    this.timer = setTimeout(() => void this.run(serial), LINT_DELAY_MS);
  }

  private async run(serial: number) {
    const { view, deps } = this;
    const lint = deps.lintEnabled();
    const check = deps.checkEnabled();
    if (!lint && !check) return this.clearAll();
    if (view.state.readOnly) return this.clearAll();

    const doc = view.state.doc;
    const text = doc.toString();
    const head = view.state.selection.main.head;
    if (text.trim() === "") return this.clearAll();

    let statement: { text: string; offset: number } | null;
    try {
      statement = await deps.statementAt(text, head);
    } catch {
      return;
    }
    // Un resultado que llega tarde se descarta: el texto o el cursor ya son otros.
    if (serial !== this.serial || view.state.doc !== doc) return;
    if (!statement || statement.text.trim() === "") return this.clearAll();

    const start = toEditorIndex(text, statement.offset);
    this.range = { from: start, to: start + statement.text.length };

    this.publish("lint", lint ? lintDiagnostics(statement.text, start) : []);

    if (!check || hasVariables(statement.text) || !deps.canCheck()) {
      this.publish("server", []);
      return;
    }

    // El resto de la espera hasta los 700 ms: el aviso local ya salió, el viaje al servidor no corre
    // para cada pausa breve entre dos teclas.
    const sentText = statement.text;
    this.checkTimer = setTimeout(
      () => void this.checkServer(serial, doc, sentText, start),
      CHECK_DELAY_MS - LINT_DELAY_MS,
    );
  }

  private async checkServer(serial: number, doc: unknown, sent: string, start: number) {
    if (serial !== this.serial || this.view.state.doc !== doc) return;
    // Se vuelve a preguntar: pasó medio segundo y la pestaña pudo empezar a ejecutar.
    if (!this.deps.checkEnabled() || !this.deps.canCheck()) return this.publish("server", []);

    let result: CheckResult;
    try {
      result = await this.deps.check(sent);
    } catch {
      // Desconexión, pestaña cerrada, error de red: la revisión calla, no es un cartel.
      return this.publish("server", []);
    }
    if (serial !== this.serial || this.view.state.doc !== doc) return;

    const diagnostic = serverDiagnostic(sent, start, result);
    this.publish("server", diagnostic ? [diagnostic] : []);
  }
}

export function diagnosticsExtension(deps: DiagnosticsDeps): Extension {
  return [
    diagnosticsField,
    diagnosticGutter,
    diagnosticHover,
    diagnosticTheme,
    ViewPlugin.define((view) => new DiagnosticsPlugin(view, deps)),
  ];
}
