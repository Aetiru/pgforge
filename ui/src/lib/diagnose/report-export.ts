import { t } from "../i18n.svelte";
import { describeError } from "../ipc";
import {
  autovacuumStatus,
  cacheStats,
  checkpointStats,
  connectionReport,
  sequenceUsage,
  serverConfigReview,
  spaceOverview,
  wraparoundStatus,
} from "../ipc";
import { assessHealth } from "../health";
import type { Metrics } from "../ipc";
import type { ReportInput } from "../health-report";
import { loadRam } from "./ram";

/**
 * Junta todo lo que el informe necesita. Cada lectura falla por su cuenta: un rol sin permiso para
 * una vista no tiene que dejar sin informe a las demás, y lo que no se pudo leer queda escrito en
 * el informe mismo (ver `buildReport`) en vez de desaparecer.
 */
export async function collectReport(
  profileId: string,
  server: string,
  database: string,
  metrics: Metrics | null,
): Promise<ReportInput> {
  const unavailable: [string, string][] = [];
  const ram = Number(loadRam(profileId).replace(",", "."));
  const ramBytes = Number.isFinite(ram) && ram > 0 ? Math.round(ram * 1024 ** 3) : null;

  async function read<T>(what: string, call: () => Promise<T>): Promise<T | null> {
    try {
      return await call();
    } catch (error) {
      unavailable.push([what, describeError(error)]);
      return null;
    }
  }

  const [connections, sequences, wraparound, autovacuum, cache, space, checkpoints, config] =
    await Promise.all([
      read(t("Conexiones"), () => connectionReport(profileId)),
      read(t("Secuencias"), () => sequenceUsage(profileId)),
      read(t("Edad de las transacciones"), () => wraparoundStatus(profileId)),
      read(t("Autovacuum"), () => autovacuumStatus(profileId)),
      read(t("Caché"), () => cacheStats(profileId)),
      read(t("Espacio"), () => spaceOverview(profileId)),
      read(t("Checkpoints"), () => checkpointStats(profileId)),
      read(t("Configuración"), () => serverConfigReview(profileId, ramBytes)),
    ]);

  return {
    generatedAt: new Date(),
    server,
    database,
    health: metrics ? assessHealth(metrics) : null,
    metrics,
    connections,
    sequences,
    wraparound,
    autovacuum,
    cache,
    space,
    checkpoints,
    config,
    unavailable,
  };
}
