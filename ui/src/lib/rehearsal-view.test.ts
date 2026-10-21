import { describe, expect, it } from "vitest";
import type { RehearsalReport, RehearsalStep } from "./ipc";
import {
  groupLocks,
  isHeavyLock,
  outcomeText,
  rowsTouched,
  stepText,
  verdict,
} from "./rehearsal-view";

const command = (tag: string, affected: number): RehearsalStep["outcome"] => ({
  kind: "command",
  tag,
  affected,
  seconds: 0,
});

const step = (index: number, outcome: RehearsalStep["outcome"], error = null): RehearsalStep => ({
  index,
  line: index + 1,
  statement: "x",
  outcome,
  error,
});

const report = (patch: Partial<RehearsalReport> = {}): RehearsalReport => ({
  total: 2,
  steps: [step(0, command("UPDATE", 2)), step(1, command("DELETE", 3))],
  locks: [],
  locksAvailable: true,
  caveats: [],
  blocked: [],
  rolledBack: true,
  rollbackError: null,
  seconds: 0.01,
  ...patch,
});

describe("outcomeText", () => {
  it("cuenta filas donde el número dice algo", () => {
    expect(outcomeText(command("UPDATE", 1)!)).toBe("UPDATE: 1 fila");
    expect(outcomeText(command("DELETE", 3)!)).toBe("DELETE: 3 filas");
    expect(outcomeText(command("CREATE TABLE", 0)!)).toBe("CREATE TABLE");
  });

  it("avisa cuando las filas devueltas se recortaron", () => {
    expect(
      outcomeText({ kind: "rows", columns: ["a"], rows: [], rowCount: 80, truncated: true, seconds: 0 }),
    ).toBe("80 filas (se muestran las primeras)");
  });

  it("un error es el texto del paso", () => {
    const failed = step(0, null, { kind: "database", code: "42P01", message: "no existe", detail: null, hint: null, position: null } as never);
    expect(stepText(failed)).toBe("no existe");
  });
});

describe("rowsTouched", () => {
  it("suma lo que modifican las sentencias y no lo que leen", () => {
    expect(rowsTouched(report())).toBe(5);
    expect(rowsTouched(report({ steps: [step(0, command("SELECT", 7))] }))).toBe(0);
  });
});

describe("verdict", () => {
  it("un ensayo limpio sale en verde", () => {
    expect(verdict(report()).tone).toBe("ok");
  });

  it("con efectos que no se revierten sale en aviso", () => {
    const caveat = { kind: "sequence" as const, index: 0, message: "x" };
    expect(verdict(report({ caveats: [caveat] })).tone).toBe("warn");
  });

  it("dice cuál sentencia falló", () => {
    const failed = step(1, null, { kind: "other", message: "boom" } as never);
    const result = verdict(report({ steps: [step(0, command("UPDATE", 1)), failed] }));
    expect(result.tone).toBe("bad");
    expect(result.title).toBe("Falló la sentencia 2 de 2");
  });

  it("si hay sentencias que no se pueden ensayar, no se ejecutó nada", () => {
    const result = verdict(
      report({
        steps: [],
        rolledBack: false,
        blocked: [{ index: 0, line: 1, statement: "VACUUM", reason: "x" }],
      }),
    );
    expect(result.title).toMatch(/No se puede ensayar/);
  });

  it("un ROLLBACK que no se confirmó es lo más grave", () => {
    const result = verdict(report({ rolledBack: false, rollbackError: "se cayó" }));
    expect(result.tone).toBe("bad");
    expect(result.title).toMatch(/ROLLBACK/);
  });
});

describe("groupLocks", () => {
  const lock = (relation: string | null, mode: string, granted = true) => ({
    relation,
    lockType: "relation",
    mode,
    granted,
  });

  it("junta los modos de cada tabla, el más restrictivo primero", () => {
    const groups = groupLocks([
      lock("public.a", "RowExclusiveLock"),
      lock("public.a", "AccessExclusiveLock"),
      lock("public.b", "AccessShareLock"),
    ]);
    expect(groups.map((group) => group.relation)).toEqual(["public.a", "public.b"]);
    expect(groups[0].modes).toEqual(["AccessExclusiveLock", "RowExclusiveLock"]);
    expect(groups[0].heaviest).toBe("AccessExclusiveLock");
  });

  it("marca el que no se llegó a obtener", () => {
    expect(groupLocks([lock("public.a", "ShareLock", false)])[0].waiting).toBe(true);
  });

  it("distingue los que frenan a los demás", () => {
    expect(isHeavyLock("AccessExclusiveLock")).toBe(true);
    expect(isHeavyLock("ShareLock")).toBe(true);
    expect(isHeavyLock("RowExclusiveLock")).toBe(false);
  });
});
