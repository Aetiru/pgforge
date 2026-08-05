import { describe, expect, it } from "vitest";

import { envLook, GROUP_LOOK, kindLabel, lookOf, tagLook } from "./badges";
import type { Environment, FolderKind, NodeKind, NodeTag } from "./ipc";

/**
 * Un tipo de objeto nuevo se agrega en dos lugares (`lookOf` y `kindLabel`) y es fácil olvidarse de
 * uno: el árbol lo dibuja igual, con el ícono de carpeta y el nombre crudo del enum. El olvido no
 * rompe nada, solo se ve mal, así que hay que buscarlo a propósito.
 *
 * Las tablas de acá son `Record` de los tipos del núcleo: agregar un `NodeKind` o un `NodeTag` sin
 * pasar por este archivo es un error de tipos en `pnpm ui:check`, antes de que el test corra.
 */

type ObjectKind = Exclude<NodeKind, { folder: FolderKind }>;

const KINDS: Record<ObjectKind, true> = {
  database: true,
  schema: true,
  table: true,
  partitionedTable: true,
  foreignTable: true,
  view: true,
  materializedView: true,
  sequence: true,
  function: true,
  procedure: true,
  type: true,
  column: true,
  index: true,
  constraint: true,
  trigger: true,
  policy: true,
  role: true,
  extension: true,
  foreignDataWrapper: true,
  foreignServer: true,
};

const TAGS: Record<NodeTag, true> = {
  login: true,
  group: true,
  superuser: true,
  partition: true,
  rowSecurity: true,
};

describe("lookOf", () => {
  it("le da ícono propio a todo tipo de objeto del catálogo", () => {
    const sinIcono = (Object.keys(KINDS) as ObjectKind[]).filter(
      (kind) => lookOf(kind).icon === "folder",
    );
    expect(sinIcono).toEqual([]);
  });

  it("dibuja como carpeta gris a las carpetas del árbol y como servidor a la raíz", () => {
    expect(lookOf({ folder: "tables" }).icon).toBe("folder");
    expect(lookOf(null).icon).toBe("server");
    // La carpeta de conexiones es amarilla justamente para no confundirse con las del servidor.
    expect(GROUP_LOOK.tone).not.toBe(lookOf({ folder: "tables" }).tone);
  });

  it("agrupa por familia: las relaciones comparten color y no lo comparten con el código", () => {
    expect(lookOf("view").tone).toBe(lookOf("materializedView").tone);
    expect(lookOf("table").tone).toBe(lookOf("partitionedTable").tone);
    expect(lookOf("table").tone).not.toBe(lookOf("function").tone);
  });
});

describe("kindLabel", () => {
  it("nombra en español a todo tipo de objeto, sin caer en el nombre del enum", () => {
    const crudos = (Object.keys(KINDS) as ObjectKind[]).filter((kind) => kindLabel(kind) === kind);
    expect(crudos).toEqual([]);
  });

  it("nombra también a la raíz y a las carpetas", () => {
    expect(kindLabel(null)).toBe("Servidor");
    expect(kindLabel({ folder: "indexes" })).toBe("Carpeta");
  });
});

const ENVIRONMENTS: Record<Environment, true> = { dev: true, test: true, prod: true };

describe("envLook", () => {
  it("le da texto, tono y explicación a cada entorno", () => {
    for (const environment of Object.keys(ENVIRONMENTS) as Environment[]) {
      const look = envLook(environment);
      expect(look.label.length).toBeGreaterThan(0);
      expect(look.tone.startsWith("tag-")).toBe(true);
      expect(look.title.length).toBeGreaterThan(0);
    }
  });

  it("le da a producción un color que no comparte con nadie", () => {
    expect(envLook("prod").tone).not.toBe(envLook("dev").tone);
    expect(envLook("prod").tone).not.toBe(envLook("test").tone);
  });
});

describe("tagLook", () => {
  it("le da texto, tono y explicación a cada rasgo del vocabulario cerrado", () => {
    for (const tag of Object.keys(TAGS) as NodeTag[]) {
      const look = tagLook(tag);
      expect(look.label.length).toBeGreaterThan(0);
      expect(look.tone.startsWith("tag-")).toBe(true);
      // El título es lo que explica la pastilla: sin él, un color no dice nada.
      expect(look.title.length).toBeGreaterThan(0);
    }
  });
});
