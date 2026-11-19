import { describe, expect, it } from "vitest";
import { aliasAt } from "../sql-rename";
import { linesTouched, previewLines } from "./rename-alias";

const sql = "SELECT c.id, c.nombre\nFROM clientes c\nJOIN pedidos p ON p.cliente_id = c.id";

describe("previewLines", () => {
  it("muestra cada línea afectada antes y después", () => {
    const found = aliasAt(sql, sql.indexOf("c.id"))!;
    const lines = previewLines(sql, found.occurrences, "cli");
    expect(lines.map((l) => l.line)).toEqual([1, 2, 3]);
    expect(lines[0].after).toBe("SELECT cli.id, cli.nombre");
    expect(lines[1].after).toBe("FROM clientes cli");
    expect(lines[2].after).toContain("= cli.id");
  });

  it("corta la lista en el máximo pedido", () => {
    const found = aliasAt(sql, sql.indexOf("c.id"))!;
    expect(previewLines(sql, found.occurrences, "x", 2)).toHaveLength(2);
  });

  it("cuenta las líneas distintas que tocaría el cambio", () => {
    const found = aliasAt(sql, sql.indexOf("c.id"))!;
    expect(linesTouched(sql, found.occurrences)).toBe(3);
  });
});
