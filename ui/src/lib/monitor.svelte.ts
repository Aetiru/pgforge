import {
  Channel,
  describeError,
  monitorConfigure,
  monitorStart,
  monitorStop,
  type ActivityFilter,
  type MonitorEvent,
  type Snapshot,
} from "./ipc";

/** Un punto de las series temporales del dashboard. */
export interface Sample {
  time: number;
  connections: number;
  active: number;
  transactionsPerSecond: number | null;
  cacheHitRatio: number | null;
}

/** Seis minutos a dos segundos. Acotado a propósito: la memoria del gráfico no puede crecer sola. */
const HISTORY_LIMIT = 180;

class MonitorStore {
  profileId = $state<string | null>(null);
  /** Base cuyas estadísticas por-base (tablas, índices, sentencias) se están mirando. Las
   *  sesiones y las métricas del clúster no dependen de ella; el resto sí. */
  database = $state<string | null>(null);
  starting = $state(false);
  error = $state<string | null>(null);

  /**
   * `$state.raw` en vez de `$state`: la muestra se reemplaza entera en cada ciclo y puede traer
   * cientos de sesiones. Envolverla en un proxy reactivo profundo cada dos segundos sería trabajo
   * puro para nada, porque nadie muta la muestra: se descarta y llega otra.
   */
  snapshot = $state.raw<Snapshot | null>(null);
  history = $state.raw<Sample[]>([]);

  filter = $state<ActivityFilter>({
    includeIdle: false,
    includeBackground: false,
    database: null,
  });
  intervalMs = $state(2000);

  private channel: Channel<MonitorEvent> | null = null;

  async start(profileId: string, database: string | null = null) {
    if (this.profileId === profileId && this.database === database) return;
    await this.stop();

    this.starting = true;
    this.error = null;
    try {
      const channel = new Channel<MonitorEvent>();
      channel.onmessage = (event) => this.receive(event);
      await monitorStart(
        profileId,
        channel,
        { intervalMs: this.intervalMs, filter: $state.snapshot(this.filter) },
        database ?? undefined,
      );
      this.channel = channel;
      this.profileId = profileId;
      this.database = database;
    } catch (error) {
      this.error = describeError(error);
    } finally {
      this.starting = false;
    }
  }

  async stop() {
    const previous = this.profileId;
    // Se desengancha el manejador antes de soltar el canal: una muestra en vuelo llegaría después
    // de haber limpiado el estado y volvería a poblarlo.
    if (this.channel) {
      this.channel.onmessage = () => {};
      this.channel = null;
    }
    this.profileId = null;
    this.database = null;
    this.snapshot = null;
    this.history = [];
    if (previous) {
      await monitorStop(previous).catch(() => {});
    }
  }

  private receive(event: MonitorEvent) {
    if (event.type === "error") {
      this.error = describeError(event.error);
      return;
    }

    this.error = null;
    this.snapshot = event.snapshot;

    const metrics = event.snapshot.metrics;
    const sample: Sample = {
      time: Date.now() / 1000,
      connections: metrics.totalConnections,
      active: metrics.activeConnections,
      transactionsPerSecond: metrics.transactionsPerSecond,
      cacheHitRatio: metrics.cacheHitRatio,
    };

    const history = this.history.concat(sample);
    this.history = history.length > HISTORY_LIMIT ? history.slice(-HISTORY_LIMIT) : history;
  }

  private async configure(options: { intervalMs?: number; paused?: boolean; filter?: ActivityFilter }) {
    if (!this.profileId) return;
    await monitorConfigure(this.profileId, options).catch(
      (error) => (this.error = describeError(error)),
    );
  }

  async setFilter(filter: ActivityFilter) {
    this.filter = filter;
    await this.configure({ filter });
  }

  async setInterval(intervalMs: number) {
    this.intervalMs = intervalMs;
    await this.configure({ intervalMs });
  }

  /**
   * Pausa el sondeo cuando la ventana deja de estar visible.
   *
   * Refrescar cada dos segundos contra un servidor de producción con la ventana minimizada es
   * consumo puro, del lado de la aplicación y del servidor, sin nadie mirando el resultado.
   */
  watchVisibility() {
    const onChange = () => this.configure({ paused: document.hidden });
    document.addEventListener("visibilitychange", onChange);
    return () => document.removeEventListener("visibilitychange", onChange);
  }

  /** Sesión por PID, para resolver el árbol de bloqueo contra los datos de cada sesión. */
  backendOf(pid: number) {
    return this.snapshot?.backends.find((backend) => backend.pid === pid) ?? null;
  }
}

export const monitor = new MonitorStore();
