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
  { action: "expandStar", label: "Expandir el * a las columnas", shortcut: "editor.expandStar" },
  { action: "renameAlias", label: "Renombrar el alias bajo el cursor", shortcut: "editor.rename" },
  { action: "runToCursor", label: "Ejecutar hasta el cursor", shortcut: "editor.runToCursor" },
  { action: "runFromCursor", label: "Ejecutar desde el cursor", shortcut: "editor.runFromCursor" },
  { action: "outline", label: "Mostrar u ocultar el esquema del script", shortcut: "editor.outline" },
  { action: "insertObject", label: "Insertar un objeto buscándolo", shortcut: "editor.insertObject" },
  { action: "pasteList", label: "Pegar el portapapeles como lista IN", shortcut: "editor.pasteList" },
  { action: "saveSnippet", label: "Guardar la selección como abreviatura…", shortcut: "editor.saveSnippet" },
  { action: "snippetPicker", label: "Abreviaturas: elegir…", shortcut: "editor.snippetPicker" },
  { action: "text:upper", label: "Texto: a mayúsculas" },
  { action: "text:lower", label: "Texto: a minúsculas" },
  { action: "text:joinComma", label: "Texto: unir líneas con comas" },
  { action: "text:quoteLines", label: "Texto: citar cada línea" },
  { action: "text:unquote", label: "Texto: quitar comillas" },
  { action: "text:inList", label: "Texto: convertir en lista IN" },
  { action: "text:sortLines", label: "Texto: ordenar líneas" },
  { action: "text:uniqueLines", label: "Texto: quitar líneas repetidas" },
  { action: "text:trim", label: "Texto: recortar espacios" },
];
