import { describe, expect, it } from "vitest";
import { toggleEvent, triggerChanges, triggerForm, validateTrigger } from "./trigger-form";
import type { TriggerInfo } from "./ipc";

const TARGET = { schema: "public", table: "facturas" };

const existing = {
  oid: 1,
  name: "audita_facturas",
  timing: "before",
  events: ["insert"],
  level: "row",
  when: null,
  functionSchema: "auditoria",
  functionName: "registrar",
} as TriggerInfo;

describe("triggerForm", () => {
  it("un trigger nuevo hereda el esquema de la tabla para su función", () => {
    expect(triggerForm(null, "ventas").functionSchema).toBe("ventas");
  });

  it("editar arranca de lo que hay en el servidor", () => {
    expect(triggerForm(existing, "public")).toMatchObject({
      name: "audita_facturas",
      functionSchema: "auditoria",
      events: ["insert"],
    });
  });
});

describe("validateTrigger", () => {
  it("no deja crear uno sin nombre, sin eventos ni sin función", () => {
    const form = triggerForm(null, "public");
    expect(validateTrigger(form)).toBe("Poné un nombre para el trigger.");
    expect(validateTrigger({ ...form, name: "t", events: [] })).toBe("Elegí al menos un evento.");
    expect(validateTrigger({ ...form, name: "t" })).toBe("Poné la función que va a ejecutar.");
    expect(validateTrigger({ ...form, name: "t", functionName: "registrar" })).toBeNull();
  });
});

describe("triggerChanges", () => {
  it("crear es una sola sentencia", () => {
    const form = { ...triggerForm(null, "public"), name: "t", functionName: "registrar" };
    const changes = triggerChanges(form, TARGET, null);
    expect(changes).toHaveLength(1);
    expect(changes[0]).toMatchObject({ kind: "createTrigger", name: "t" });
  });

  it("editar borra y crea, porque PostgreSQL no puede alterar lo que define a un trigger", () => {
    const form = { ...triggerForm(existing, "public"), timing: "after" as const };
    const changes = triggerChanges(form, TARGET, existing);
    expect(changes.map((change) => change.kind)).toEqual(["dropTrigger", "createTrigger"]);
  });

  it("el borrado nombra al trigger como estaba, no como quedó", () => {
    const form = { ...triggerForm(existing, "public"), name: "audita_v2" };
    const [drop, create] = triggerChanges(form, TARGET, existing);
    expect(drop).toMatchObject({ name: "audita_facturas" });
    expect(create).toMatchObject({ name: "audita_v2" });
  });

  it("un WHEN en blanco viaja como null y no como cadena vacía", () => {
    const form = { ...triggerForm(existing, "public"), when: "   " };
    const [, create] = triggerChanges(form, TARGET, existing);
    expect(create).toMatchObject({ definition: { when: null } });
  });
});

describe("toggleEvent", () => {
  it("agrega el que falta y saca el que estaba", () => {
    expect(toggleEvent(["insert"], "update")).toEqual(["insert", "update"]);
    expect(toggleEvent(["insert", "update"], "insert")).toEqual(["update"]);
  });
});
