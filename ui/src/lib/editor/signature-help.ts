/**
 * Ayuda de parámetros de funciones: al abrir el paréntesis de `date_trunc(` aparece la firma con el
 * argumento actual en negrita, y al pasar el cursor sobre el nombre de una función, su firma y su
 * comentario.
 *
 * Dónde está el cursor respecto de la llamada lo decide `callAt` (puro, no se confunde con
 * `IN (` ni con `INSERT INTO t (`); las firmas vienen del catálogo por nombre (`routine_signatures`)
 * y se guardan cinco minutos: una función no cambia mientras se escribe una consulta.
 *
 * Es una ayuda, no un cartel: si el servidor no contesta o la función no existe, no aparece nada.
 */

import { t } from "../i18n.svelte";
import { StateEffect, StateField, type EditorState, type Extension } from "@codemirror/state";
import {
  EditorView,
  ViewPlugin,
  hoverTooltip,
  showTooltip,
  type Tooltip,
  type ViewUpdate,
} from "@codemirror/view";
import type { RoutineSignature } from "../ipc";
import { activeParameter, callAt, type CallAt } from "../sql-call";
import { qualifiedNameAt } from "../sql-complete";
import { TtlCache } from "./ttl-cache";

const TTL_MS = 5 * 60_000;
/** Cuántas sobrecargas se listan: más que eso es un muro de texto y la elegida ya está arriba. */
const MAX_LISTED = 4;

// ---------------------------------------------------------------------------------------------
// Lo puro
// ---------------------------------------------------------------------------------------------

interface Shape {
  parts: string[];
  /** Mínimo de argumentos que hay que pasar: los parámetros sin `DEFAULT` y sin ser variádicos. */
  required: number;
  variadic: boolean;
}

function shapeOf(signature: RoutineSignature): Shape {
  const { parts } = activeParameter(signature.arguments, 0);
  const variadic = parts.some((p) => /^\s*VARIADIC\b/i.test(p));
  const required = parts.filter((p) => !/\bDEFAULT\b/i.test(p) && !/^\s*VARIADIC\b/i.test(p)).length;
  return { parts, required, variadic };
}

/** ¿Una llamada con `count` argumentos escritos calza con esta firma? */
function accepts(shape: Shape, count: number): boolean {
  if (shape.variadic) return count >= shape.required;
  return count >= shape.required && count <= shape.parts.length;
}

/**
 * Pone primero la sobrecarga que mejor calza con lo escrito.
 *
 * Con el cursor en el argumento `argIndex` ya hay al menos `argIndex + 1` argumentos, así que
 * calzan las firmas que aceptan esa cantidad; entre ellas, la de menos parámetros (la más
 * específica de lo escrito). Si ninguna calza —se escribió de más— se conserva el orden dado: es
 * mejor mostrar algo que esconder la ayuda justo cuando se equivocó uno.
 */
export function orderOverloads(signatures: RoutineSignature[], argIndex: number): RoutineSignature[] {
  const count = argIndex + 1;
  const scored = signatures.map((signature, order) => {
    const shape = shapeOf(signature);
    return { signature, order, fits: accepts(shape, count), size: shape.parts.length };
  });
  scored.sort((a, b) => Number(b.fits) - Number(a.fits) || (a.fits ? a.size - b.size : 0) || a.order - b.order);
  return scored.map((item) => item.signature);
}

export interface SignaturePart {
  text: string;
  active: boolean;
}

/** La firma lista para dibujar: el nombre, los parámetros con el actual marcado y el resultado. */
export function describeSignature(
  signature: RoutineSignature,
  argIndex: number,
): { name: string; params: SignaturePart[]; result: string | null } {
  const { parts, activeIndex } = activeParameter(signature.arguments, argIndex);
  return {
    name: signature.name,
    params: parts.map((text, index) => ({ text, active: index === activeIndex && argIndex >= 0 })),
    result: signature.result,
  };
}

// ---------------------------------------------------------------------------------------------
// El globo
// ---------------------------------------------------------------------------------------------

function signatureDom(signatures: RoutineSignature[], argIndex: number, listAll: boolean): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "cm-signature";

  const listed = signatures.slice(0, listAll ? MAX_LISTED + 1 : MAX_LISTED);
  listed.forEach((signature, index) => {
    // Solo la primera lleva el parámetro activo: las demás son alternativas, no la que se está llamando.
    const info = describeSignature(signature, index === 0 && !listAll ? argIndex : -1);
    const line = document.createElement("div");
    line.className = index === 0 ? "cm-signature-main" : "cm-signature-other";

    const name = document.createElement("span");
    name.className = "cm-signature-name";
    name.textContent = `${info.name}(`;
    line.appendChild(name);

    info.params.forEach((param, i) => {
      if (i > 0) line.appendChild(document.createTextNode(", "));
      const span = document.createElement("span");
      span.className = param.active ? "cm-signature-active" : "cm-signature-param";
      span.textContent = param.text;
      line.appendChild(span);
    });
    line.appendChild(document.createTextNode(")"));
    if (info.result) {
      const result = document.createElement("span");
      result.className = "cm-signature-result";
      result.textContent = ` → ${info.result}`;
      line.appendChild(result);
    }
    dom.appendChild(line);

    if (signature.comment && (index === 0 || listAll)) {
      const comment = document.createElement("div");
      comment.className = "cm-signature-comment";
      comment.textContent = signature.comment;
      dom.appendChild(comment);
    }
  });

  if (signatures.length > listed.length) {
    const more = document.createElement("div");
    more.className = "cm-signature-comment";
    more.textContent = t("y {signatures} sobrecargas más", { signatures: signatures.length - listed.length });
    dom.appendChild(more);
  }
  return dom;
}

const signatureTheme = EditorView.baseTheme({
  ".cm-signature": { padding: "6px 8px", maxWidth: "520px", fontSize: "12px", fontFamily: "var(--font-mono)" },
  ".cm-signature-other": { marginTop: "4px", opacity: "0.7" },
  ".cm-signature-name": { fontWeight: "600" },
  ".cm-signature-active": { fontWeight: "700", textDecoration: "underline" },
  ".cm-signature-result": { color: "var(--cm-gutter)" },
  ".cm-signature-comment": { marginTop: "4px", fontFamily: "var(--font-sans)", color: "var(--cm-gutter)" },
});

// ---------------------------------------------------------------------------------------------
// Estado
// ---------------------------------------------------------------------------------------------

interface ActiveCall {
  call: CallAt;
  signatures: RoutineSignature[] | null;
  /** El usuario la cerró con Escape: no vuelve a abrirse hasta que la llamada sea otra. */
  dismissed: boolean;
}

const setActive = StateEffect.define<ActiveCall | null>();
const dismissActive = StateEffect.define<null>();

const sameCall = (a: CallAt, b: CallAt) => a.name === b.name && a.schema === b.schema && a.nameFrom === b.nameFrom;

const activeField = StateField.define<ActiveCall | null>({
  create: () => null,
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setActive)) return effect.value;
      if (effect.is(dismissActive) && value) return { ...value, dismissed: true };
    }
    // La llamada empieza en un lugar del texto: si se escribe antes, se corre con él.
    if (value && tr.docChanged) {
      const from = tr.changes.mapPos(value.call.nameFrom, 1);
      return { ...value, call: { ...value.call, nameFrom: from, nameTo: tr.changes.mapPos(value.call.nameTo, -1) } };
    }
    return value;
  },
  provide: (field) =>
    showTooltip.compute([field], (state): Tooltip | null => {
      const active = state.field(field);
      if (!active || active.dismissed || !active.signatures?.length) return null;
      const ordered = orderOverloads(active.signatures, active.call.argIndex);
      return {
        pos: Math.min(active.call.nameFrom, state.doc.length),
        above: true,
        strictSide: true,
        arrow: false,
        create: () => ({ dom: signatureDom(ordered, active.call.argIndex, false) }),
      };
    }),
});

/** Cierra la ayuda si estaba abierta. Devuelve `true` si había algo que cerrar (para el Escape). */
export function closeSignatureHelp(view: EditorView): boolean {
  const active = view.state.field(activeField, false);
  if (!active || active.dismissed || !active.signatures?.length) return false;
  view.dispatch({ effects: dismissActive.of(null) });
  return true;
}

export interface SignatureDeps {
  signatures: (schema: string | null, name: string) => Promise<RoutineSignature[]>;
  enabled?: () => boolean;
}

export function signatureHelp(deps: SignatureDeps): Extension {
  const cache = new TtlCache<RoutineSignature[]>(TTL_MS);
  const load = (schema: string | null, name: string) =>
    cache.get(`${schema ?? ""}.${name}`, () => deps.signatures(schema, name));

  // Cuánto texto se analiza por tecla: en un script enorme, leer todo para encontrar un paréntesis
  // costaría más de lo que la ayuda vale. Se mira la sentencia que rodea al cursor.
  const WINDOW = 4000;

  const around = (state: EditorState) => {
    const head = state.selection.main.head;
    const from = Math.max(0, head - WINDOW);
    return { text: state.sliceDoc(from, Math.min(state.doc.length, head + WINDOW)), base: from, head };
  };

  const plugin = ViewPlugin.fromClass(
    class {
      private timer: ReturnType<typeof setTimeout> | undefined;
      private token = 0;
      constructor(private view: EditorView) {}

      update(update: ViewUpdate) {
        if (update.docChanged || update.selectionSet) {
          clearTimeout(this.timer);
          // Después del cambio y no durante: despachar dentro de `update` está prohibido.
          this.timer = setTimeout(() => this.refresh(), 0);
        }
      }

      destroy() {
        clearTimeout(this.timer);
        this.token += 1;
      }

      private refresh() {
        const { view } = this;
        if (!view.dom.isConnected) return;
        const previous = view.state.field(activeField);

        if ((deps.enabled && !deps.enabled()) || view.state.selection.ranges.length > 1) {
          if (previous) view.dispatch({ effects: setActive.of(null) });
          return;
        }

        const { text, base, head } = around(view.state);
        const found = callAt(text, head - base);
        if (!found) {
          if (previous) view.dispatch({ effects: setActive.of(null) });
          return;
        }
        const call: CallAt = { ...found, nameFrom: found.nameFrom + base, nameTo: found.nameTo + base };

        // Misma llamada: solo cambia el argumento actual (o nada); las firmas ya están.
        if (previous && sameCall(previous.call, call)) {
          if (previous.call.argIndex !== call.argIndex) {
            view.dispatch({ effects: setActive.of({ ...previous, call }) });
          }
          return;
        }

        const token = ++this.token;
        view.dispatch({ effects: setActive.of({ call, signatures: null, dismissed: false }) });
        load(call.schema, call.name)
          .then((signatures) => {
            if (token !== this.token || !view.dom.isConnected) return;
            const current = view.state.field(activeField);
            if (!current || !sameCall(current.call, call)) return;
            view.dispatch({ effects: setActive.of({ ...current, signatures }) });
          })
          // Sin conexión o función inexistente: no aparece nada, ni un error.
          .catch(() => {});
      }
    },
  );

  // Hover sobre el nombre de una función (seguido de `(`): todas las sobrecargas con su comentario.
  const hover = hoverTooltip(
    async (view, pos) => {
      if (deps.enabled && !deps.enabled()) return null;
      const { text, base } = (() => {
        const from = Math.max(0, pos - 200);
        return { text: view.state.sliceDoc(from, Math.min(view.state.doc.length, pos + 200)), base: from };
      })();
      const word = qualifiedNameAt(text, pos - base);
      if (!word || !/^\s*\(/.test(text.slice(word.to))) return null;

      let signatures: RoutineSignature[];
      try {
        signatures = await load(word.schema, word.name);
      } catch {
        return null;
      }
      if (signatures.length === 0) return null;
      return {
        pos: word.from + base,
        end: word.to + base,
        above: true,
        create: () => ({ dom: signatureDom(signatures, -1, true) }),
      };
    },
    { hideOnChange: true },
  );

  return [activeField, signatureTheme, plugin, hover];
}
