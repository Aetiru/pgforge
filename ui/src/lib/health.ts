import type { Metrics } from "./ipc";

/**
 * Un veredicto del monitoreo en una palabra: ¿hay algo que mirar ahora?
 *
 * Las seis fichas del dashboard informan cada una lo suyo, pero para saber si todo está bien había
 * que leerlas todas. Acá se juntan en un solo nivel y en la lista de lo que lo causa. Los umbrales
 * son altos a propósito —una lista de avisos tibios se aprende a ignorar—: cada uno señala algo que
 * pide una decisión, no una cifra que merecía un color.
 */
export type HealthLevel = "ok" | "warn" | "bad";

export interface HealthIssue {
  level: Exclude<HealthLevel, "ok">;
  text: string;
}

export interface Health {
  level: HealthLevel;
  issues: HealthIssue[];
}

/** De qué tan llena está la tabla de conexiones. Lo comparte la ficha de «Conexiones». */
export function connectionLevel(total: number, max: number): HealthLevel {
  if (max <= 0) return "ok";
  const ratio = total / max;
  if (ratio > 0.9) return "bad";
  if (ratio > 0.7) return "warn";
  return "ok";
}

export function assessHealth(metrics: Metrics): Health {
  const issues: HealthIssue[] = [];

  const connections = connectionLevel(metrics.totalConnections, metrics.maxConnections);
  if (connections !== "ok") {
    const percent = Math.round((metrics.totalConnections / metrics.maxConnections) * 100);
    issues.push({
      level: connections,
      text: `Conexiones al ${percent} % del máximo (${metrics.totalConnections} de ${metrics.maxConnections})`,
    });
  }

  if (metrics.waitingConnections > 0) {
    const n = metrics.waitingConnections;
    issues.push({
      level: "bad",
      text: `${n} ${n === 1 ? "sesión espera" : "sesiones esperan"} a otra`,
    });
  }

  if (metrics.idleInTransaction > 0) {
    const n = metrics.idleInTransaction;
    issues.push({
      level: "warn",
      text: `${n} ${n === 1 ? "sesión" : "sesiones"} inactiva${n === 1 ? "" : "s"} en transacción: retienen candados sin trabajar`,
    });
  }

  const longest = metrics.longestTransactionSeconds;
  if (longest !== null) {
    if (longest > 3600) {
      issues.push({ level: "bad", text: "Hay una transacción abierta hace más de una hora" });
    } else if (longest > 300) {
      issues.push({ level: "warn", text: "Hay una transacción abierta hace más de cinco minutos" });
    }
  }

  // Solo el aviso, nunca «mal»: un caché bajo es normal justo después de arrancar el servidor o
  // sobre una base chica que nunca llenó el buffer.
  if (metrics.cacheHitRatio !== null && metrics.cacheHitRatio < 0.9) {
    issues.push({
      level: "warn",
      text: `Aciertos de caché en ${(metrics.cacheHitRatio * 100).toFixed(1)} %`,
    });
  }

  const level: HealthLevel = issues.some((issue) => issue.level === "bad")
    ? "bad"
    : issues.length > 0
      ? "warn"
      : "ok";
  return { level, issues };
}
