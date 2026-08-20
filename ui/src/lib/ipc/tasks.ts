import { invoke, type CoreError } from "./core";

// ---------------------------------------------------------------------------
// Procesos largos
//
// Una sentencia que tarda —el mantenimiento, la creación de un índice— corre del lado del servidor
// con su propia sesión y reporta por un canal. Los eventos son los mismos para las dos, que es lo
// que permite que la vista de procesos las muestre en la misma lista.
// ---------------------------------------------------------------------------

export type TaskEvent =
  | { type: "started"; sql: string }
  | { type: "notice"; severity: string; message: string }
  | { type: "finished"; seconds: number }
  | { type: "failed"; error: CoreError };

/** Corta una sentencia larga en curso, pidiéndoselo al servidor. */
export const taskCancel = (taskId: string) => invoke<void>("task_cancel", { taskId });
