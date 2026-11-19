import { describe, expect, it } from "vitest";
import type { RoutineSignature } from "../ipc";
import { describeSignature, orderOverloads } from "./signature-help";

const sig = (name: string, args: string, result = "text"): RoutineSignature => ({
  schema: "pg_catalog",
  name,
  arguments: args,
  result,
  comment: null,
  kind: "function",
});

describe("orderOverloads", () => {
  const trunc = [
    sig("date_trunc", "text, timestamp without time zone"),
    sig("date_trunc", "text, timestamp with time zone, text"),
    sig("date_trunc", "text, interval"),
  ];

  it("con un argumento escrito calza cualquiera que acepte uno o más: gana la de menos parámetros entre las que calzan", () => {
    const [first] = orderOverloads(trunc, 0);
    expect(first.arguments).toBe("text, timestamp without time zone");
  });

  it("con tres argumentos solo calza la de tres", () => {
    const [first] = orderOverloads(trunc, 2);
    expect(first.arguments).toBe("text, timestamp with time zone, text");
  });

  it("si ninguna calza conserva el orden dado en vez de esconder la ayuda", () => {
    const ordered = orderOverloads(trunc, 9);
    expect(ordered.map((s) => s.arguments)).toEqual(trunc.map((s) => s.arguments));
  });

  it("los parámetros con DEFAULT y los variádicos no son obligatorios", () => {
    const concat = [sig("concat_ws", "text, VARIADIC \"any\""), sig("f", "a integer, b integer DEFAULT 1")];
    expect(orderOverloads([concat[1], concat[0]], 0)[0].name).toBe("f");
    expect(orderOverloads(concat, 7)[0].name).toBe("concat_ws");
  });
});

describe("describeSignature", () => {
  it("marca el parámetro actual y deja el resto", () => {
    const info = describeSignature(sig("round", "numeric, integer", "numeric"), 1);
    expect(info.params.map((p) => [p.text, p.active])).toEqual([
      ["numeric", false],
      ["integer", true],
    ]);
    expect(info.result).toBe("numeric");
  });

  it("sin argumento actual no marca ninguno (la lista de sobrecargas)", () => {
    const info = describeSignature(sig("round", "numeric, integer"), -1);
    expect(info.params.every((p) => !p.active)).toBe(true);
  });

  it("el tipo con coma adentro no parte el parámetro", () => {
    const info = describeSignature(sig("f", "a numeric(10,2), b text"), 0);
    expect(info.params).toHaveLength(2);
  });
});
