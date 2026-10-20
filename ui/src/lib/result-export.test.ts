import { describe, expect, it } from "vitest";
import { jsonValue, toJson, toMarkdown, uniqueNames } from "./result-export";

describe("toJson", () => {
  it("conserva NULL distinto de la cadena vacía", () => {
    const out = JSON.parse(toJson({ columns: ["a"], rows: [[null], [""]] }));
    expect(out).toEqual([{ a: null }, { a: "" }]);
  });

  it("usa los tipos cuando los hay", () => {
    const out = JSON.parse(
      toJson(
        { columns: ["n", "b", "j", "big"], rows: [["3", "t", '{"x":1}', "9007199254740993"]] },
        ["int4", "bool", "jsonb", "int8"],
      ),
    );
    expect(out).toEqual([{ n: 3, b: true, j: { x: 1 }, big: "9007199254740993" }]);
  });

  it("no pierde una columna repetida", () => {
    expect(uniqueNames(["a", "a", "a"])).toEqual(["a", "a_2", "a_3"]);
  });

  it("un json inválido queda como texto", () => {
    expect(jsonValue("{", "json")).toBe("{");
  });
});

describe("toMarkdown", () => {
  it("escapa barras y saltos de línea", () => {
    const md = toMarkdown({ columns: ["x|y"], rows: [["a|b\nc"], [null]] });
    expect(md).toBe("| x\\|y |\n| --- |\n| a\\|b<br>c |\n| NULL |\n");
  });
});
