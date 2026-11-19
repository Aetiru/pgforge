import { describe, expect, it } from "vitest";
import { parsePins } from "./pins.svelte";

describe("parsePins", () => {
  it("lee una lista de identificadores y descarta lo que no es texto", () => {
    expect(parsePins('["a","b",3,null]')).toEqual(["a", "b"]);
  });

  it("tolera lo vacío y lo roto", () => {
    expect(parsePins(null)).toEqual([]);
    expect(parsePins("{")).toEqual([]);
    expect(parsePins('{"a":1}')).toEqual([]);
  });
});
