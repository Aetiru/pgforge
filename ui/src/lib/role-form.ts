/**
 * Lógica pura del formulario de roles: qué `RoleChange` sale de lo que hay escrito en pantalla.
 *
 * Vive fuera de `RoleDialog.svelte` porque es la única parte del diálogo verificable sin ventana, y
 * es donde un error se propaga callado: un atributo que no se detecta como cambiado no genera su
 * `ALTER ROLE`, y una membresía que no se compara contra la original no se revoca. Compilar no
 * atrapa nada de eso.
 */

import type { RoleAttributes, RoleChange, RoleInfo } from "./ipc";

export interface RoleForm {
  name: string;
  superuser: boolean;
  createdb: boolean;
  createrole: boolean;
  inherit: boolean;
  login: boolean;
  replication: boolean;
  bypassRls: boolean;
  /** `null` es «sin límite», que en Postgres es -1. */
  connectionLimit: number | null;
  /** Siempre arranca vacía: Postgres nunca devuelve la contraseña para precargarla. */
  password: string;
  /** `YYYY-MM-DD`; vacío es «sin vencimiento». */
  validUntil: string;
  /** Los roles tildados en el selector. Antes era el texto crudo separado por comas, y un nombre
   *  mal tipeado se descubría recién al fallar el `GRANT`. */
  memberOf: string[];
  adminOption: boolean;
}

/** La copia editable inicial. El diálogo la toma una sola vez, con `untrack`. */
export function roleForm(existing: RoleInfo | null): RoleForm {
  return {
    name: existing?.name ?? "",
    superuser: existing?.superuser ?? false,
    createdb: existing?.createdb ?? false,
    createrole: existing?.createrole ?? false,
    inherit: existing?.inherit ?? true,
    login: existing?.login ?? false,
    replication: existing?.replication ?? false,
    bypassRls: existing?.bypassRls ?? false,
    connectionLimit: existing && existing.connectionLimit !== -1 ? existing.connectionLimit : null,
    password: "",
    validUntil: validUntilDate(existing?.validUntil),
    memberOf: [],
    adminOption: false,
  };
}

/**
 * El vencimiento recortado a la fecha; vacío para «infinity», null o sin fecha.
 *
 * `rolvaliduntil` llega como timestamptz en texto ("2025-12-31 00:00:00+00") o como "infinity", y el
 * campo de fecha del formulario solo maneja `YYYY-MM-DD`.
 */
export function validUntilDate(value: string | null | undefined): string {
  const text = (value ?? "").trim();
  return /^\d{4}-\d{2}-\d{2}/.test(text) ? text.slice(0, 10) : "";
}

/** Las membresías elegidas, sin vacíos ni repetidas: un `GRANT` duplicado no falla, pero ensucia el
 *  SQL de la vista previa con una línea que no hace nada. */
export function memberOfList(form: RoleForm): string[] {
  const seen = new Set<string>();
  for (const role of form.memberOf) {
    const name = role.trim();
    if (name) seen.add(name);
  }
  return [...seen];
}

function connectionLimitOf(form: RoleForm): number {
  return form.connectionLimit ?? -1;
}

/**
 * Los cambios pendientes. Sin `existing` da de alta; con `existing` arma el diff contra el rol que
 * hay en el servidor y contra `originalMemberOf`, las membresías que tenía al abrir el diálogo.
 */
export function roleChanges(
  form: RoleForm,
  existing: RoleInfo | null,
  originalMemberOf: string[],
): RoleChange[] {
  if (!existing) {
    const attributes: RoleAttributes = {
      superuser: form.superuser,
      createdb: form.createdb,
      createrole: form.createrole,
      inherit: form.inherit,
      login: form.login,
      replication: form.replication,
      bypassRls: form.bypassRls,
      connectionLimit: connectionLimitOf(form),
      password: form.password || undefined,
      validUntil: form.validUntil.trim() || undefined,
    };
    return [
      { kind: "createRole", name: form.name.trim(), attributes, memberOf: memberOfList(form) },
    ];
  }

  const out: RoleChange[] = [];
  let currentName = existing.name;
  if (form.name.trim() !== existing.name) {
    out.push({ kind: "renameRole", name: currentName, newName: form.name.trim() });
    currentName = form.name.trim();
  }

  const attrs: RoleAttributes = {};
  let anyAttr = false;
  const setIfChanged = <K extends keyof RoleAttributes>(
    key: K,
    value: RoleAttributes[K],
    original: RoleAttributes[K],
  ) => {
    if (value !== original) {
      attrs[key] = value;
      anyAttr = true;
    }
  };
  setIfChanged("superuser", form.superuser, existing.superuser);
  setIfChanged("createdb", form.createdb, existing.createdb);
  setIfChanged("createrole", form.createrole, existing.createrole);
  setIfChanged("inherit", form.inherit, existing.inherit);
  setIfChanged("login", form.login, existing.login);
  setIfChanged("replication", form.replication, existing.replication);
  setIfChanged("bypassRls", form.bypassRls, existing.bypassRls);
  setIfChanged("connectionLimit", connectionLimitOf(form), existing.connectionLimit);
  if (form.password.trim()) {
    attrs.password = form.password;
    anyAttr = true;
  }
  const until = form.validUntil.trim();
  if (until !== validUntilDate(existing.validUntil)) {
    // Vaciar el campo vuelve el rol a "sin vencimiento", que en Postgres es 'infinity'.
    attrs.validUntil = until || "infinity";
    anyAttr = true;
  }
  if (anyAttr) out.push({ kind: "alterRole", name: currentName, attributes: attrs });

  const current = memberOfList(form);
  for (const role of current.filter((r) => !originalMemberOf.includes(r))) {
    out.push({ kind: "grantMembership", role, member: currentName, adminOption: form.adminOption });
  }
  for (const role of originalMemberOf.filter((r) => !current.includes(r))) {
    out.push({ kind: "revokeMembership", role, member: currentName });
  }

  return out;
}

export function validateRole(form: RoleForm): string | null {
  if (!form.name.trim()) return "Poné un nombre para el rol.";
  if (
    form.connectionLimit !== null &&
    (!Number.isInteger(form.connectionLimit) || form.connectionLimit < 0)
  ) {
    return "El límite de conexiones tiene que ser un número entero no negativo.";
  }
  return null;
}
