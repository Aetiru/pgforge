import type { Backend } from "./ipc";

/**
 * Cómo se ordenan, filtran y agrupan las sesiones del monitoreo.
 *
 * Vive suelto y es puro porque cada una de estas decisiones se nota a ojo y se rompe callada: una
 * sesión bloqueada que cae en «inactivas» se esconde justo de quien la busca. El `Dashboard` y la
 * línea de tiempo leen las mismas funciones, así que una sesión no puede ser «activa» en una vista
 * y «bloqueada» en otra.
 */

/** Lo que le pasa a la sesión, en el orden en que pide atención. */
export type SessionKind = "blocked" | "itx" | "active" | "idle";

export const KIND_LABEL: Record<SessionKind, string> = {
  blocked: "Bloqueada",
  itx: "Inactiva en transacción",
  active: "Activa",
  idle: "Inactiva",
};

/**
 * Bloqueada gana sobre todo lo demás: una sesión puede estar «activa» y esperando un candado a la
 * vez, y lo que importa de ella es lo segundo. Todo lo que no es activa ni inactiva en transacción
 * (`idle`, `fastpath function call`, un estado que el rol no puede ver) cuenta como inactiva.
 */
export function kindOf(backend: Backend): SessionKind {
  if (backend.blockedBy.length > 0) return "blocked";
  if (backend.state?.startsWith("idle in transaction")) return "itx";
  if (backend.state === "active") return "active";
  return "idle";
}

export type SessionCounts = Record<SessionKind | "all", number>;

export function countSessions(list: Backend[]): SessionCounts {
  const counts: SessionCounts = { all: list.length, blocked: 0, itx: 0, active: 0, idle: 0 };
  for (const backend of list) counts[kindOf(backend)] += 1;
  return counts;
}

export interface SessionFilter {
  kind: SessionKind | "all";
  text: string;
}

/** El texto busca en lo que uno recuerda de una sesión: quién es, desde dónde y qué corre. */
export function filterSessions(list: Backend[], filter: SessionFilter): Backend[] {
  const needle = filter.text.trim().toLowerCase();
  return list.filter((backend) => {
    if (filter.kind !== "all" && kindOf(backend) !== filter.kind) return false;
    if (needle === "") return true;
    const haystack = [
      backend.pid,
      backend.user,
      backend.database,
      backend.applicationName,
      backend.clientAddr,
      backend.query,
    ]
      .filter((part) => part !== null && part !== undefined)
      .join(" ")
      .toLowerCase();
    return haystack.includes(needle);
  });
}

export type SortKey = "pid" | "user" | "app" | "duration" | "wait";

/** La duración que se dibuja: la de la consulta, y si no hay, la de la transacción. */
export function secondsOf(backend: Backend): number {
  return backend.querySeconds ?? backend.transactionSeconds ?? 0;
}

export function sortSessions(list: Backend[], key: SortKey, descending: boolean): Backend[] {
  const value = (backend: Backend): string | number => {
    switch (key) {
      case "pid":
        return backend.pid;
      case "user":
        return (backend.user ?? "").toLowerCase();
      case "app":
        return (backend.applicationName ?? "").toLowerCase();
      case "wait":
        return backend.waitEvent ?? "";
      case "duration":
        return secondsOf(backend);
    }
  };
  const sign = descending ? -1 : 1;
  // El PID desempata: sin un orden total, dos sesiones con la misma duración intercambian lugar en
  // cada muestra y la fila que se está leyendo salta.
  return [...list].sort((a, b) => {
    const left = value(a);
    const right = value(b);
    if (left < right) return -1 * sign;
    if (left > right) return 1 * sign;
    return a.pid - b.pid;
  });
}

export type GroupBy = "none" | "app" | "user";

export interface SessionGroup {
  key: string;
  label: string;
  items: Backend[];
  /** La duración más larga del grupo: lo que ordena los grupos entre sí. */
  longest: number;
}

const NO_NAME = "(sin nombre)";

/**
 * Agrupa por aplicación o por usuario, con el grupo de la sesión más larga primero. Con `none`
 * devuelve un solo grupo sin rótulo, para que quien dibuja no tenga dos caminos.
 */
export function groupSessions(list: Backend[], by: GroupBy): SessionGroup[] {
  if (by === "none") {
    return [{ key: "", label: "", items: list, longest: Math.max(0, ...list.map(secondsOf)) }];
  }
  const groups = new Map<string, Backend[]>();
  for (const backend of list) {
    const name = (by === "app" ? backend.applicationName : backend.user)?.trim() || NO_NAME;
    const bucket = groups.get(name);
    if (bucket) bucket.push(backend);
    else groups.set(name, [backend]);
  }
  return [...groups.entries()]
    .map(([label, items]) => ({
      key: label,
      label,
      items,
      longest: Math.max(0, ...items.map(secondsOf)),
    }))
    .sort((a, b) => b.longest - a.longest || a.label.localeCompare(b.label));
}

/**
 * El largo de una barra de duración, de 0 a 1, en escala logarítmica: con la lineal, una consulta
 * de siete minutos deja a todas las demás en un píxel y la barra no dice nada de ellas. El `1 +` es
 * lo que hace que 0 s dé 0 y no `-Infinity`.
 */
export function barShare(seconds: number, longest: number): number {
  if (seconds <= 0 || longest <= 0) return 0;
  return Math.min(1, Math.log10(1 + seconds) / Math.log10(1 + longest));
}

/** `threshold` en segundos; `0` es «no resaltar». */
export function isLong(backend: Backend, threshold: number): boolean {
  return threshold > 0 && backend.state === "active" && secondsOf(backend) >= threshold;
}
