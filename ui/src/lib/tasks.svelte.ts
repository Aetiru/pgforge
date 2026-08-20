/**
 * Los procesos largos que corren mientras se sigue usando la aplicación.
 *
 * Antes cada uno vivía adentro del diálogo que lo había lanzado: el `VACUUM`, el backup y el
 * `CREATE INDEX CONCURRENTLY` se seguían desde una ventana modal que no se podía cerrar sin
 * cancelarlos, así que la aplicación entera quedaba tomada por algo que tarda media hora. Acá el
 * diálogo lanza y se cierra, y lo que corre queda en esta lista, que es lo que dibuja la vista de
 * procesos.
 *
 * El dueño del canal es este registro y no el componente: un `Channel` de Tauri sigue recibiendo
 * eventos aunque nadie lo mire, pero si lo tuviera un diálogo desmontado no habría dónde anotarlos.
 * Lo que corre del lado del servidor no se toca al cerrar la ventana —cancelar es explícito—.
 */

import { explorer } from "./explorer.svelte";
import { outcomeText, progressText, type TaskKind } from "./task-format";
import {
  Channel,
  backupCancel,
  backupRun,
  dataCopyCancel,
  dataExportRun,
  dataImportRun,
  describeError,
  indexCreate,
  maintenanceRun,
  restoreCancel,
  restoreRun,
  taskCancel,
  type BackupEvent,
  type BackupOptions,
  type ExportEvent,
  type ExportSpec,
  type ImportEvent,
  type ImportSpec,
  type IndexDef,
  type Operation,
  type RestoreEvent,
  type RestoreOptions,
  type Target,
  type TaskEvent,
} from "./ipc";

export type TaskStatus = "running" | "done" | "failed";

let sequence = 0;

/** Un proceso lanzado, con lo que se sabe de él hasta ahora. */
export class TaskRun {
  /** Identificador local. Existe desde antes que el del servidor, que llega recién al arrancar. */
  readonly key = `run-${++sequence}`;
  readonly kind: TaskKind;
  readonly profileId: string;
  /** Nombre del servidor, copiado al lanzar: es el rótulo, no la identidad. */
  readonly server: string;
  readonly database: string;
  /** Sobre qué corre: `public.pedidos`, `base app`, el archivo de un backup. */
  readonly target: string;
  readonly startedAt = Date.now();

  /** El SQL o la línea de comando que se está ejecutando. Llega con el primer evento. */
  command = $state("");
  status = $state<TaskStatus>("running");
  /** Lo que el servidor o la herramienta fueron contando: `NOTICE`, salida de `pg_dump`. */
  log = $state<string[]>([]);
  progress = $state<string | null>(null);
  outcome = $state<string | null>(null);
  finishedAt = $state<number | null>(null);
  canceling = $state(false);

  /** El identificador del lado de Rust, con el que se cancela. */
  taskId = $state<string | null>(null);
  private readonly cancelWith: (taskId: string) => Promise<void>;
  /**
   * Qué hacer cuando termina bien. Lo usa quien lanzó el proceso para releer lo que cambió —la
   * lista de índices de una tabla, la grilla de datos—: ahora el objeto aparece cuando existe de
   * verdad y no cuando se apretó el botón.
   */
  private readonly onDone?: () => void;

  constructor(init: {
    kind: TaskKind;
    profileId: string;
    database: string;
    target: string;
    cancelWith: (taskId: string) => Promise<void>;
    onDone?: () => void;
  }) {
    this.kind = init.kind;
    this.profileId = init.profileId;
    this.database = init.database;
    this.target = init.target;
    this.cancelWith = init.cancelWith;
    this.onDone = init.onDone;
    this.server =
      explorer.profiles.find((profile) => profile.id === init.profileId)?.name ?? init.profileId;
  }

  note(line: string) {
    this.log = [...this.log, line];
  }

  finish(text: string) {
    this.status = "done";
    this.outcome = text;
    this.finishedAt = Date.now();
    this.taskId = null;
    this.onDone?.();
  }

  fail(error: unknown) {
    this.status = "failed";
    this.outcome = typeof error === "string" ? error : describeError(error);
    this.finishedAt = Date.now();
    this.taskId = null;
  }

  async cancel() {
    if (!this.taskId) return;
    this.canceling = true;
    try {
      await this.cancelWith(this.taskId);
    } catch (error) {
      this.note(describeError(error));
    } finally {
      this.canceling = false;
    }
  }
}

class Tasks {
  all = $state<TaskRun[]>([]);
  /**
   * Procesos terminados que el usuario todavía no vio. La vista de procesos no está a la vista
   * mientras uno trabaja —de eso se trata—, así que el aviso de que algo terminó tiene que
   * sobrevivir hasta que la mire.
   */
  unseen = $state(0);

  get running(): TaskRun[] {
    return this.all.filter((run) => run.status === "running");
  }

  get finished(): TaskRun[] {
    return this.all.filter((run) => run.status !== "running");
  }

  /** Los más nuevos arriba: lo que se acaba de lanzar es lo que se va a mirar. */
  get list(): TaskRun[] {
    return [...this.all].reverse();
  }

  seen() {
    this.unseen = 0;
  }

  remove(run: TaskRun) {
    this.all = this.all.filter((item) => item.key !== run.key);
  }

  clearFinished() {
    this.all = this.all.filter((run) => run.status === "running");
  }

  /** Cierra el proceso y descuenta el aviso si estaba sin ver. */
  private done(run: TaskRun, text: string) {
    run.finish(text);
    this.unseen += 1;
  }

  private failed(run: TaskRun, error: unknown) {
    run.fail(error);
    this.unseen += 1;
  }

  private add(run: TaskRun): TaskRun {
    this.all.push(run);
    return run;
  }

  /** VACUUM, ANALYZE o REINDEX sobre una tabla, un índice o una base. */
  maintenance(init: {
    profileId: string;
    database: string;
    target: string;
    operation: Operation;
    on: Target;
  }): TaskRun {
    const run = this.add(
      new TaskRun({
        kind: "maintenance",
        profileId: init.profileId,
        database: init.database,
        target: init.target,
        cancelWith: taskCancel,
      }),
    );

    const channel = new Channel<TaskEvent>();
    channel.onmessage = (event) => this.onStatement(run, event);

    // Vacío es «la base por omisión del servidor», que es lo que decide el lado de Rust: mandar
    // una cadena vacía haría que la tarea corriera contra una base que no existe.
    maintenanceRun(init.profileId, init.operation, init.on, channel, init.database || undefined)
      .then((taskId) => (run.taskId = taskId))
      .catch((error) => this.failed(run, error));

    return run;
  }

  /** La creación de un índice, que con `CONCURRENTLY` es de lo más largo que hay. */
  index(init: {
    profileId: string;
    database: string;
    target: string;
    def: IndexDef;
    onDone?: () => void;
  }): TaskRun {
    const run = this.add(
      new TaskRun({
        kind: "index",
        profileId: init.profileId,
        database: init.database,
        target: init.target,
        cancelWith: taskCancel,
        onDone: init.onDone,
      }),
    );

    const channel = new Channel<TaskEvent>();
    channel.onmessage = (event) => this.onStatement(run, event);

    indexCreate(init.profileId, init.def, channel, init.database)
      .then((taskId) => (run.taskId = taskId))
      .catch((error) => this.failed(run, error));

    return run;
  }

  backup(init: { profileId: string; options: BackupOptions }): TaskRun {
    const run = this.add(
      new TaskRun({
        kind: "backup",
        profileId: init.profileId,
        database: init.options.database,
        target: init.options.path,
        cancelWith: backupCancel,
      }),
    );

    const channel = new Channel<BackupEvent>();
    channel.onmessage = (event) => {
      switch (event.type) {
        case "started":
          run.command = event.command.join(" ");
          break;
        case "progress":
          run.note(event.message);
          break;
        case "finished":
          this.done(run, outcomeText({ kind: "backup", seconds: event.seconds, bytes: event.bytes }));
          break;
        case "failed":
          this.failed(run, event.error);
          break;
      }
    };

    backupRun(init.profileId, init.options, channel)
      .then((taskId) => (run.taskId = taskId))
      .catch((error) => this.failed(run, error));

    return run;
  }

  restore(init: { profileId: string; options: RestoreOptions }): TaskRun {
    const run = this.add(
      new TaskRun({
        kind: "restore",
        profileId: init.profileId,
        database: init.options.database,
        target: init.options.source,
        cancelWith: restoreCancel,
      }),
    );

    const channel = new Channel<RestoreEvent>();
    channel.onmessage = (event) => {
      switch (event.type) {
        case "started":
          run.command = event.command.join(" ");
          break;
        case "progress":
          run.note(event.message);
          break;
        case "finished":
          this.done(
            run,
            outcomeText({
              kind: "restore",
              seconds: event.seconds,
              ignoredErrors: event.ignoredErrors,
            }),
          );
          break;
        case "failed":
          this.failed(run, event.error);
          break;
      }
    };

    restoreRun(init.profileId, init.options, channel)
      .then((taskId) => (run.taskId = taskId))
      .catch((error) => this.failed(run, error));

    return run;
  }

  export(init: {
    profileId: string;
    database: string;
    target: string;
    spec: ExportSpec;
    path: string;
  }): TaskRun {
    const run = this.add(
      new TaskRun({
        kind: "export",
        profileId: init.profileId,
        database: init.database,
        target: init.target,
        cancelWith: dataCopyCancel,
      }),
    );

    const channel = new Channel<ExportEvent>();
    channel.onmessage = (event) => {
      switch (event.type) {
        case "started":
          run.command = event.command;
          break;
        case "progress":
          run.progress = progressText(event.bytes);
          break;
        case "finished":
          this.done(
            run,
            outcomeText({ kind: "export", seconds: event.seconds, bytes: event.bytes }),
          );
          break;
        case "failed":
          this.failed(run, event.error);
          break;
      }
    };

    dataExportRun(init.profileId, init.spec, init.path, channel, init.database)
      .then((taskId) => (run.taskId = taskId))
      .catch((error) => this.failed(run, error));

    return run;
  }

  import(init: {
    profileId: string;
    database: string;
    target: string;
    spec: ImportSpec;
    path: string;
    onDone?: () => void;
  }): TaskRun {
    const run = this.add(
      new TaskRun({
        kind: "import",
        profileId: init.profileId,
        database: init.database,
        target: init.target,
        cancelWith: dataCopyCancel,
        onDone: init.onDone,
      }),
    );

    const channel = new Channel<ImportEvent>();
    channel.onmessage = (event) => {
      switch (event.type) {
        case "started":
          run.command = event.command;
          break;
        case "progress":
          run.progress = progressText(event.bytes);
          break;
        case "finished":
          this.done(
            run,
            outcomeText({
              kind: "import",
              seconds: event.seconds,
              bytes: event.bytes,
              rows: event.rows,
            }),
          );
          break;
        case "failed":
          this.failed(run, event.error);
          break;
      }
    };

    dataImportRun(init.profileId, init.spec, init.path, channel, init.database)
      .then((taskId) => (run.taskId = taskId))
      .catch((error) => this.failed(run, error));

    return run;
  }

  /** El mantenimiento y el índice mandan los mismos eventos: los dos son una sentencia larga. */
  private onStatement(run: TaskRun, event: TaskEvent) {
    switch (event.type) {
      case "started":
        run.command = event.sql;
        break;
      case "notice":
        run.note(event.message);
        break;
      case "finished":
        this.done(run, outcomeText({ kind: run.kind, seconds: event.seconds }));
        break;
      case "failed":
        this.failed(run, event.error);
        break;
    }
  }
}

export const tasks = new Tasks();
