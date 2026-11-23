import { t } from "../i18n.svelte";
import type { TextTransform } from "../sql-text-tools";

/**
 * Lo que la paleta de comandos puede pedirle al editor activo. La paleta no tiene el `EditorView` a
 * mano —vive en otro componente y hay uno por pestaña—, así que pide por nombre y `QueryPanel`
 * traduce el pedido (ver `QueryTab.requestEditor`).
 */
export type EditorAction =
  | "expandStar"
  | "renameAlias"
  | "runToCursor"
  | "runFromCursor"
  | "outline"
  | "insertObject"
  | "pasteList"
  | "saveSnippet"
  | "snippetPicker"
  | `text:${TextTransform}`;

export interface EditorActionDef {
  action: EditorAction;
  label: string;
  /** Id del atajo en `shortcuts.ts`, para mostrar su combinación en la paleta. */
  shortcut?: string;
}

export const EDITOR_ACTIONS: EditorActionDef[] = [
  { action: "expandStar", get label() { return t("Expandir el * a las columnas"); }, shortcut: "editor.expandStar" },
  { action: "renameAlias", get label() { return t("Renombrar el alias bajo el cursor"); }, shortcut: "editor.rename" },
  { action: "runToCursor", get label() { return t("Ejecutar hasta el cursor"); }, shortcut: "editor.runToCursor" },
  { action: "runFromCursor", get label() { return t("Ejecutar desde el cursor"); }, shortcut: "editor.runFromCursor" },
  { action: "outline", get label() { return t("Mostrar u ocultar el esquema del script"); }, shortcut: "editor.outline" },
  { action: "insertObject", get label() { return t("Insertar un objeto buscándolo"); }, shortcut: "editor.insertObject" },
  { action: "pasteList", get label() { return t("Pegar el portapapeles como lista IN"); }, shortcut: "editor.pasteList" },
  { action: "saveSnippet", get label() { return t("Guardar la selección como abreviatura…"); }, shortcut: "editor.saveSnippet" },
  { action: "snippetPicker", get label() { return t("Abreviaturas: elegir…"); }, shortcut: "editor.snippetPicker" },
  { action: "text:upper", get label() { return t("Texto: a mayúsculas"); } },
  { action: "text:lower", get label() { return t("Texto: a minúsculas"); } },
  { action: "text:joinComma", get label() { return t("Texto: unir líneas con comas"); } },
  { action: "text:quoteLines", get label() { return t("Texto: citar cada línea"); } },
  { action: "text:unquote", get label() { return t("Texto: quitar comillas"); } },
  { action: "text:inList", get label() { return t("Texto: convertir en lista IN"); } },
  { action: "text:sortLines", get label() { return t("Texto: ordenar líneas"); } },
  { action: "text:uniqueLines", get label() { return t("Texto: quitar líneas repetidas"); } },
  { action: "text:trim", label: "Texto: recortar espacios" },
];
