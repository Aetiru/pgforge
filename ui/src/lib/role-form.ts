/**
 * Lógica pura del formulario de roles: qué `RoleChange` sale de lo que hay escrito en pantalla.
 *
 * Vive fuera de `RoleDialog.svelte` porque es la única parte del diálogo verificable sin ventana, y
 * es donde un error se propaga callado: un atributo que no se detecta como cambiado no genera su
 * `ALTER ROLE`, y una membresía que no se compara contra la original no se revoca. Compilar no
 * atrapa nada de eso.
 */

import { t } from "./i18n.svelte";
import type { AccessLevel, RoleAttributes, RoleChange, RoleInfo } from "./ipc";

/** Lo que se está creando. Un grupo es un rol que no inicia sesión: reúne permisos y otros roles entran en él. */
export type RoleKind = "user" | "group";

/**
 * La plantilla de acceso de un alta: qué puede hacer el rol sobre un esquema de la base conectada.
 * `none` es no dar nada (es el valor inicial: dar acceso es una decisión, no un valor por omisión).
 */
export interface AccessTemplate {
  schema: string;
  level: "none" | AccessLevel;
  /** `ALTER DEFAULT PRIVILEGES`: solo cubre lo que cree quien ejecuta la sentencia. */
  futureTables: boolean;
}

export interface RoleForm {
  kind: RoleKind;
  access: AccessTemplate;
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
    kind: existing && !existing.login ? "group" : "user",
    access: { schema: "", level: "none", futureTables: false },
    name: existing?.name ?? "",
    superuser: existing?.superuser ?? false,
    createdb: existing?.createdb ?? false,
    createrole: existing?.createrole ?? false,
    inherit: existing?.inherit ?? true,
    // Un rol nuevo arranca como usuario (el caso común), así que con inicio de sesión.
    login: existing?.login ?? true,
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
    const user = form.kind === "user";
    const name = form.name.trim();
    // Un grupo no inicia sesión, así que contraseña, vencimiento y límite de conexiones no le
    // significan nada: no se mandan aunque el formulario los conserve de antes de cambiar de tipo.
    const attributes: RoleAttributes = {
      superuser: form.superuser,
      createdb: form.createdb,
      createrole: form.createrole,
      inherit: form.inherit,
      login: user,
      replication: form.replication,
      bypassRls: form.bypassRls,
      connectionLimit: user ? connectionLimitOf(form) : -1,
      password: user ? form.password || undefined : undefined,
      validUntil: user ? form.validUntil.trim() || undefined : undefined,
    };
    const groups = memberOfList(form);
    // `createRole` no sabe de ADMIN OPTION: con la casilla puesta las membresías salen como
    // `GRANT` aparte, que es lo único que lo expresa.
    const withAdmin = form.adminOption && groups.length > 0;
    const out: RoleChange[] = [
      { kind: "createRole", name, attributes, memberOf: withAdmin ? [] : groups },
    ];
    if (withAdmin) {
      for (const role of groups) {
        out.push({ kind: "grantMembership", role, member: name, adminOption: true });
      }
    }
    if (form.access.level !== "none" && form.access.schema) {
      out.push({
        kind: "grantSchemaAccess",
        role: name,
        schema: form.access.schema,
        level: form.access.level,
        futureTables: form.access.futureTables,
      });
    }
    return out;
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
  if (!form.name.trim()) return t("Poné un nombre para el rol.");
  if (
    form.connectionLimit !== null &&
    (!Number.isInteger(form.connectionLimit) || form.connectionLimit < 0)
  ) {
    return t("El límite de conexiones tiene que ser un número entero no negativo.");
  }
  return null;
}

// --- Ayudas del diálogo de alta -------------------------------------------------------------------

/** Cuánto dura la cuenta, en los atajos que se ofrecen; `custom` es una fecha elegida a mano. */
export type ExpiryPreset = "never" | "30" | "90" | "365" | "custom";

export const EXPIRY_DAYS: Record<Exclude<ExpiryPreset, "never" | "custom">, number> = {
  "30": 30,
  "90": 90,
  "365": 365,
};

/**
 * La fecha de dentro de `days` días como `YYYY-MM-DD`, con las partes **locales** del día: con
 * `toISOString` una cuenta creada de noche vencería un día antes, porque eso devuelve UTC.
 */
export function addDays(days: number, now: Date = new Date()): string {
  const date = new Date(now.getFullYear(), now.getMonth(), now.getDate() + days);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** Qué atajo corresponde a una fecha: sirve para marcar el botón correcto al abrir un rol que ya vence. */
export function expiryPresetOf(validUntil: string, now: Date = new Date()): ExpiryPreset {
  if (!validUntil.trim()) return "never";
  for (const [preset, days] of Object.entries(EXPIRY_DAYS)) {
    if (addDays(days, now) === validUntil.trim()) return preset as ExpiryPreset;
  }
  return "custom";
}

/**
 * Letras y cifras sin las que se confunden (`0/O`, `1/l/I`) más dos símbolos que sobreviven a
 * cualquier cadena de conexión y a una terminal: una contraseña que se lee por teléfono o se pega en
 * una URL no tiene que traer sorpresas.
 */
const PASSWORD_ALPHABET = "abcdefghjkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789-_";

/**
 * Una contraseña aleatoria. `random` se inyecta para poder probar la distribución; por omisión es
 * `crypto.getRandomValues`, nunca `Math.random`: es un secreto.
 *
 * Se descartan los valores que caen en el resto del módulo: `byte % 58` favorecería a los primeros
 * caracteres del alfabeto, que es sesgo chico pero gratis de evitar.
 */
export function generatePassword(
  length = 18,
  random: (size: number) => Uint8Array = (size) => crypto.getRandomValues(new Uint8Array(size)),
): string {
  const limit = 256 - (256 % PASSWORD_ALPHABET.length);
  let out = "";
  while (out.length < length) {
    for (const byte of random((length - out.length) * 2)) {
      if (byte >= limit) continue;
      out += PASSWORD_ALPHABET[byte % PASSWORD_ALPHABET.length];
      if (out.length === length) break;
    }
  }
  return out;
}

/** Los atributos que no son el inicio de sesión, con lo que cada uno permite dicho en cristiano. */
export interface RoleAttributeInfo {
  key: "createdb" | "createrole" | "superuser" | "replication" | "bypassRls" | "inherit";
  label: string;
  help: string;
  /** Los de riesgo alto llevan el aviso en rojo y no se tildan por descuido. */
  risk: "normal" | "high";
}

/** Una función y no una constante: los textos siguen el idioma de la aplicación. */
export function roleAttributes(): RoleAttributeInfo[] {
  return [
    {
      key: "createdb",
      label: t("Puede crear bases de datos"),
      help: t("Crea y borra las bases que él mismo cree."),
      risk: "normal",
    },
    {
      key: "createrole",
      label: t("Puede crear y modificar otros roles"),
      help: t("Con esto puede darse acceso a sí mismo a casi todo."),
      risk: "high",
    },
    {
      key: "superuser",
      label: t("Superusuario"),
      help: t("Salta todos los permisos del servidor. Rara vez se justifica para una persona."),
      risk: "high",
    },
    {
      key: "replication",
      label: t("Puede replicar el servidor"),
      help: t("Lo usan las réplicas y las copias en caliente, no las personas."),
      risk: "high",
    },
    {
      key: "bypassRls",
      label: t("Ignora la seguridad por filas"),
      help: t("Ve todas las filas aunque la tabla las restrinja por política."),
      risk: "high",
    },
    {
      key: "inherit",
      label: t("Hereda los permisos de sus grupos"),
      help: t("Si se apaga, tiene que usar SET ROLE para valerse de lo que dan sus grupos."),
      risk: "normal",
    },
  ];
}

/** Algún atributo de riesgo alto tildado: lo que dispara el aviso rojo del diálogo. */
export function dangerousAttributes(form: RoleForm): RoleAttributeInfo[] {
  return roleAttributes().filter((info) => info.risk === "high" && info.key !== "inherit" && form[info.key]);
}

/**
 * Qué va a pasar, en frases. Es lo que hace legible el SQL para quien no lo lee de corrido, y sale
 * del mismo formulario que arma los cambios, así que no puede contar otra cosa.
 */
export function roleSummary(form: RoleForm): string[] {
  const name = form.name.trim() || t("(sin nombre)");
  const out: string[] = [];
  if (form.kind === "user") {
    out.push(t("Se crea el usuario {name}, que puede iniciar sesión.", { name }));
    const until = form.validUntil.trim();
    out.push(until ? t("Vence el {date}.", { date: until }) : t("No vence."));
    out.push(
      form.password
        ? t("Tiene contraseña: cópiala ahora, no se vuelve a mostrar.")
        : t("Todavía no tiene contraseña: no podrá entrar hasta que se la pongas."),
    );
  } else {
    out.push(t("Se crea el grupo {name}. No inicia sesión: sirve para reunir permisos.", { name }));
  }

  const groups = memberOfList(form);
  out.push(
    groups.length
      ? t("Hereda los permisos de {groups}.", { groups: groups.join(", ") })
      : t("No pertenece a ningún grupo."),
  );

  if (form.access.level !== "none" && form.access.schema) {
    const schema = form.access.schema;
    const what: Record<AccessLevel, string> = {
      read: t("Puede leer todas las tablas."),
      write: t("Puede leer y modificar los datos de todas las tablas."),
      admin: t("Puede además crear y borrar objetos."),
    };
    out.push(
      t("En {schema}: {what}", { schema, what: what[form.access.level] }) +
        (form.access.futureTables ? " " + t("Incluye las tablas que se creen después.") : ""),
    );
  }
  return out;
}
