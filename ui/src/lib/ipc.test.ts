import { describe, expect, it } from "vitest";

import {
  describeError,
  folderOf,
  formatVersion,
  isCanceled,
  sshHostKey,
  type CoreError,
} from "./ipc";

/**
 * `ipc.ts` es la única frontera con Rust, y estos helpers deciden qué ve el usuario cuando algo
 * falla. Un `kind` mal escrito acá no rompe la compilación —el error entra como `unknown`— y se
 * manifiesta como un cartel que dice `[object Object]` justo cuando más se necesita leerlo.
 */

describe("formatVersion", () => {
  it("muestra el número de versión del servidor como lo escribe la gente", () => {
    expect(formatVersion(160004)).toBe("16.4");
    expect(formatVersion(130016)).toBe("13.16");
    expect(formatVersion(170000)).toBe("17.0");
  });
});

describe("folderOf", () => {
  it("distingue una carpeta del árbol de un objeto del catálogo", () => {
    expect(folderOf({ folder: "tables" })).toBe("tables");
    expect(folderOf("table")).toBeNull();
  });
});

describe("isCanceled", () => {
  it("reconoce la cancelación pedida por el usuario, que no es una falla", () => {
    expect(isCanceled({ kind: "canceled" } satisfies CoreError)).toBe(true);
  });

  it("no confunde con una cancelación cualquier otro error", () => {
    expect(isCanceled({ kind: "other", message: "se cayó la red" } satisfies CoreError)).toBe(false);
    expect(isCanceled(new Error("boom"))).toBe(false);
    expect(isCanceled(null)).toBe(false);
    expect(isCanceled("canceled")).toBe(false);
  });
});

describe("describeError", () => {
  it("traduce cada variante del núcleo a un texto legible", () => {
    expect(describeError({ kind: "canceled" })).toBe("Operación cancelada.");
    expect(describeError({ kind: "conflict", message: "La fila cambió." })).toBe(
      "La fila cambió.",
    );
    expect(describeError({ kind: "permission", message: "hace falta ser dueño" })).toBe(
      "Permiso insuficiente: hace falta ser dueño",
    );
    expect(describeError({ kind: "other", message: "se cayó la red" })).toBe("se cayó la red");
  });

  it("suma el hint del servidor cuando viene, y no inventa paréntesis vacíos cuando no", () => {
    const base = { kind: "database", code: "42P01", detail: null, position: null } as const;
    expect(describeError({ ...base, message: "no existe la tabla", hint: "¿pusiste el esquema?" })).toBe(
      "no existe la tabla (¿pusiste el esquema?)",
    );
    expect(describeError({ ...base, message: "no existe la tabla", hint: null })).toBe(
      "no existe la tabla",
    );
  });

  it("avisa distinto si la clave del host SSH cambió que si nunca se vio", () => {
    const unverified = describeError({
      kind: "sshHostKey",
      host: "bastion.ejemplo",
      fingerprint: "SHA256:abc",
      changed: false,
    });
    expect(unverified).toContain("no está verificado");
    expect(unverified).toContain("SHA256:abc");

    const changed = describeError({
      kind: "sshHostKey",
      host: "bastion.ejemplo",
      fingerprint: "SHA256:abc",
      changed: true,
    });
    expect(changed).toContain("cambió");
    expect(changed).toContain("intermediario");
  });

  it("no se rompe con algo que no es un error del núcleo", () => {
    expect(describeError("texto suelto")).toBe("texto suelto");
    expect(describeError(undefined)).toBe("undefined");
  });
});

describe("sshHostKey", () => {
  it("devuelve la huella para que la interfaz pueda pedir confirmación", () => {
    expect(
      sshHostKey({
        kind: "sshHostKey",
        host: "bastion.ejemplo",
        fingerprint: "SHA256:abc",
        changed: true,
      }),
    ).toEqual({ host: "bastion.ejemplo", fingerprint: "SHA256:abc", changed: true });
  });

  it("devuelve null para cualquier otro error, que no se confirma: se muestra", () => {
    expect(sshHostKey({ kind: "other", message: "se cayó la red" })).toBeNull();
    expect(sshHostKey(new Error("boom"))).toBeNull();
  });
});
