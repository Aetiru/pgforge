import { describe, expect, it } from "vitest";
import { cellOf, indexEffective, objectsOf, PRIVILEGES_OF } from "./permission-matrix";
import type { EffectivePrivilege } from "./ipc";

function row(
  object: string,
  role: string,
  privilege: string,
  granted: boolean,
  direct = granted,
): EffectivePrivilege {
  return { object, role, privilege, granted, direct };
}

describe("objectsOf", () => {
  it("lista cada objeto una sola vez, en el orden en que aparece", () => {
    const effective = [
      row("clientes", "ana", "SELECT", true),
      row("clientes", "ana", "INSERT", false),
      row("pedidos", "ana", "SELECT", true),
      row("clientes", "beto", "SELECT", false),
    ];
    expect(objectsOf(effective)).toEqual(["clientes", "pedidos"]);
  });

  it("una lista vacía no lista ningún objeto", () => {
    expect(objectsOf([])).toEqual([]);
  });
});

describe("indexEffective / cellOf", () => {
  it("encuentra la celda exacta por objeto, rol y privilegio", () => {
    const effective = [
      row("clientes", "ana", "SELECT", true),
      row("clientes", "ana", "INSERT", false),
      row("clientes", "beto", "SELECT", true, false),
    ];
    const index = indexEffective(effective);

    expect(cellOf(index, "clientes", "ana", "SELECT")).toEqual(
      row("clientes", "ana", "SELECT", true),
    );
    expect(cellOf(index, "clientes", "beto", "SELECT")?.direct).toBe(false);
  });

  it("una combinación que no vino en la lectura devuelve null, no un error", () => {
    const index = indexEffective([row("clientes", "ana", "SELECT", true)]);
    expect(cellOf(index, "clientes", "ana", "DELETE")).toBeNull();
    expect(cellOf(index, "pedidos", "ana", "SELECT")).toBeNull();
  });

  it("un nombre citado con espacios no choca con otra combinación", () => {
    // "a b" + "c" no puede confundirse con "a" + "b c": si la clave usara un separador que
    // pudiera aparecer en un identificador, estas dos entradas colisionarían.
    const effective = [row("a b", "c", "SELECT", true), row("a", "b c", "SELECT", false)];
    const index = indexEffective(effective);

    expect(cellOf(index, "a b", "c", "SELECT")?.granted).toBe(true);
    expect(cellOf(index, "a", "b c", "SELECT")?.granted).toBe(false);
  });
});

describe("PRIVILEGES_OF", () => {
  it("trae el vocabulario completo por familia de objeto", () => {
    expect(PRIVILEGES_OF.table).toContain("TRUNCATE");
    expect(PRIVILEGES_OF.sequence).toEqual(["USAGE", "SELECT", "UPDATE"]);
    expect(PRIVILEGES_OF.function).toEqual(["EXECUTE"]);
  });
});
