/**
 * Los procesos largos que corren mientras se sigue usando la aplicación.
 *
 * Antes cada uno vivía adentro del diálogo que lo había lanzado: el `VACUUM`, el backup y el
 * `CREATE INDEX CONCURRENTLY` se seguían desde una ventana modal que no se podía cerrar sin
 * cancelarlos, así que la aplicación entera quedaba tomada por algo que tarda media hora. Después
 * el diálogo pasó a lanzar y cerrarse, y lo que corre quedó en esta lista, que es lo que dibuja la
 * vista de procesos.
 *
 * Lo que cambió ahora es **quién es el dueño**. Antes lo era esta lista: el canal de cada proceso
 * lo tenía la interfaz y el avance, el resultado y el registro de lo que fue informando vivían solo
 * acá. Eso hacía que recargar la ventana los borrara a todos aunque siguieran corriendo del otro
 * lado, y que un backup que terminaba justo durante la recarga no dejara rastro de si había salido
 * bien. Ahora el dueño es Rust —el récord de cada proceso vive en `process.rs`— y esto es un
 * espejo: se engancha con un solo canal, el primer mensaje trae todo lo que hay y después llegan
 * las novedades.
 *
 * Lo que corre del lado del servidor no se toca al cerrar la ventana ni al recargarla: cancelar es
 * explícito.
 */

import { explorer } from "./explorer.svelte";
import { notify } from "./notify.svelte";
import { outcomeText, progressText, type TaskKind } from "./task-format";
import {
  Channel,
  backupRun,
  dataExportRun,
  dataImportRun,
  describeError,
  indexCreate,
  maintenanceRun,
  processCancel,
  processClear,
  processRemove,
  processWatch,
  restoreRun,
  type BackupOptions,
  type ExportSpec,
  type ImportSpec,
  type IndexDef,
  type Operation,
  type ProcessEvent,
  type ProcessRecord,
  type ProcessStatus,
  type RestoreOptions,
  type Target,
} from "./ipc";

export type TaskStatus = ProcessStatus;

/** Un proceso, tal como lo cuenta Rust. La interfaz solo le agrega lo que se dibuja. */
export class TaskRun {
  readonly taskId: string;
  readonly kind: TaskKind;
  readonly profileId: string;
  readonly database: string;
  /** Sobre qué corre: `public.pedidos`, `base app`, el archivo de un backup. */
  readonly target: string;
  readonly startedAt: number;

  /** El SQL o la línea de comando que se está ejecutando. */
  command = $state("");
  status = $state<TaskStatus>("running");
  /** Lo que el servidor o la herramienta fueron contando: `NOTICE`, salida de `pg_dump`. */
  log = $state<string[]>([]);
  progress = $state<string | null>(null);
  outcome = $state<string | null>(null);
  finishedAt = $state<number | null>(null);
  canceling = $state(false);

  constructor(record: ProcessRecord) {
    this.taskId = record.taskId;
    this.kind = record.kind;
    this.profileId = record.profile;
    this.database = record.database;
    this.target = record.target;
    this.startedAt = record.startedMs;
    this.apply(record);
  }

  /**
   * El nombre del servidor sale del perfil y no de un campo propio: se puede cambiar sin cerrar
   * nada, y es rótulo y no identidad.
   */
  get server(): string {
    return explorer.profiles.find((profile) => profile.id === this.profileId)?.name ?? this.profileId;
  }

  /** Copia lo que dice el récord. Se usa al crear la fila y al reengancharse tras una recarga. */
  apply(record: ProcessRecord) {
    this.command = record.command;
    this.status = record.status;
    this.log = record.log;
    this.progress = record.progress === null ? null : progressText(record.progress);
    this.finishedAt = record.finishedMs;
    if (record.outcome) this.outcome = outcomeText({ kind: this.kind, ...record.outcome });
    else if (record.error) this.outcome = describeError(record.error);
  }

  note(line: string) {
    this.log = [...this.log, line];
  }

  async cancel() {
    if (this.status !== "running") return;
    this.canceling = true;
    try {
      await processCancel(this.taskId);
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

  /**
   * Qué hacer cuando termina bien cada proceso, por identificador. Lo pone quien lo lanzó para
   * releer lo que cambió —la lista de índices de una tabla, la grilla de datos—: así el objeto
   * aparece cuando existe de verdad y no cuando se apretó el botón.
   *
   * No vive en el `TaskRun` porque el récord lo arma Rust y una recarga lo vuelve a traer: una
   * función no cruza el canal, y releer una grilla que ya no está abierta no tendría sentido.
   */
  private readonly pending = new Map<string, () => void>();
  /** Los que terminaron antes de que su `onDone` llegara a anotarse (ver `follow`). */
  private readonly doneEarly = new Set<string>();

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

  async remove(run: TaskRun) {
    this.all = this.all.filter((item) => item.taskId !== run.taskId);
    await processRemove(run.taskId);
  }

  async clearFinished() {
    this.all = this.all.filter((run) => run.status === "running");
    await processClear();
  }

  /**
   * Se engancha al registro de procesos. Se llama una vez, al arrancar la interfaz.
   *
   * El primer mensaje trae todo lo que hay, así que después de recargar la ventana la lista vuelve
   * con lo que siguió corriendo mientras tanto —y con lo que terminó sin nadie mirando—.
   */
  async watch() {
    const channel = new Channel<ProcessEvent>();
    channel.onmessage = (event) => this.receive(event);
    await processWatch(channel);
  }

  private find(taskId: string): TaskRun | undefined {
    return this.all.find((run) => run.taskId === taskId);
  }

  private receive(event: ProcessEvent) {
    switch (event.type) {
      case "snapshot":
        this.all = event.records.map((record) => new TaskRun(record));
        break;

      case "started":
        this.all.push(new TaskRun(event.record));
        break;

      case "log":
        this.find(event.taskId)?.note(event.message);
        break;

      case "progress": {
        const run = this.find(event.taskId);
        if (run) run.progress = progressText(event.bytes);
        break;
      }

      case "finished": {
        const run = this.find(event.taskId);
        if (run) {
          run.status = "done";
          run.outcome = outcomeText({ kind: run.kind, ...event.outcome });
          run.finishedAt = Date.now();
          this.announce(run);
        }
        this.settle(event.taskId);
        break;
      }

      case "failed": {
        const run = this.find(event.taskId);
        if (run) {
          run.status = "failed";
          run.outcome = describeError(event.error);
          run.finishedAt = Date.now();
          this.announce(run);
        }
        // Lo que había que releer al terminar bien ya no corresponde, y dejarlo anotado sería
        // guardarlo para siempre: el identificador no vuelve a aparecer.
        this.pending.delete(event.taskId);
        break;
      }
    }
  }

  /** Cuenta que terminó: el contador de la barra y, si está encendido, el aviso del sistema. */
  private announce(run: TaskRun) {
    this.unseen += 1;
    void notify.taskEnded(run);
  }

  /** Dispara el `onDone` de un proceso que terminó bien, incluso si todavía no llegó a anotarse. */
  private settle(taskId: string) {
    const done = this.pending.get(taskId);
    if (done) {
      this.pending.delete(taskId);
      done();
    } else {
      this.doneEarly.add(taskId);
    }
  }

  /**
   * Anota qué releer cuando el proceso termine bien.
   *
   * El identificador llega recién cuando el comando responde, y una tarea corta puede terminar
   * antes: por eso se mira primero si ya terminó, en vez de anotar algo que nunca se va a disparar.
   */
  private follow(taskId: string, onDone?: () => void) {
    if (!onDone) return;
    if (this.doneEarly.delete(taskId)) onDone();
    else this.pending.set(taskId, onDone);
  }

  /**
   * Lo que hacen todos: lanzar y anotar qué releer al terminar.
   *
   * Si el lanzamiento falla no hay proceso que mostrar —falló antes de existir del lado de Rust, por
   * una opción inválida o un servidor caído—, así que el error sube al diálogo, que es el que sigue
   * abierto y puede explicarlo.
   */
  private async launch(start: () => Promise<string>, onDone?: () => void) {
    this.follow(await start(), onDone);
  }

  /** VACUUM, ANALYZE o REINDEX sobre una tabla, un índice o una base. */
  async maintenance(init: {
    profileId: string;
    database: string;
    target: string;
    operation: Operation;
    on: Target;
  }) {
    // Vacío es «la base por omisión del servidor», que es lo que decide el lado de Rust: mandar
    // una cadena vacía haría que la tarea corriera contra una base que no existe.
    await this.launch(() =>
      maintenanceRun(
        init.profileId,
        init.operation,
        init.on,
        init.target,
        init.database || undefined,
      ),
    );
  }

  /** La creación de un índice, que con `CONCURRENTLY` es de lo más largo que hay. */
  async index(init: {
    profileId: string;
    database: string;
    def: IndexDef;
    onDone?: () => void;
  }) {
    await this.launch(() => indexCreate(init.profileId, init.def, init.database), init.onDone);
  }

  async backup(init: { profileId: string; options: BackupOptions }) {
    await this.launch(() => backupRun(init.profileId, init.options));
  }

  async restore(init: { profileId: string; options: RestoreOptions }) {
    await this.launch(() => restoreRun(init.profileId, init.options));
  }

  async export(init: {
    profileId: string;
    database: string;
    target: string;
    spec: ExportSpec;
    path: string;
  }) {
    await this.launch(() =>
      dataExportRun(init.profileId, init.spec, init.path, init.target, init.database),
    );
  }

  async import(init: {
    profileId: string;
    database: string;
    spec: ImportSpec;
    path: string;
    onDone?: () => void;
  }) {
    await this.launch(
      () => dataImportRun(init.profileId, init.spec, init.path, init.database),
      init.onDone,
    );
  }
}

export const tasks = new Tasks();
