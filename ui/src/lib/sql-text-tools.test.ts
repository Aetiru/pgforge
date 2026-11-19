import { describe, expect, it } from "vitest";
import { pasteAsList, transformText } from "./sql-text-tools";

describe("pasteAsList", () => {
  it("convierte una columna de texto en una lista entre comillas", () => {
    expect(pasteAsList("Ana\nLuis\nMaría\n")).toBe("('Ana', 'Luis', 'María')");
  });

  it("deja los números sin comillas solo si todos lo son", () => {
    expect(pasteAsList("1\n2\n3.5\n-4")).toBe("(1, 2, 3.5, -4)");
    expect(pasteAsList("00123\n456")).toBe("('00123', '456')");
    expect(pasteAsList("1\nabc")).toBe("('1', 'abc')");
  });

  it("escapa las comillas simples y descarta líneas vacías sin quitar duplicados", () => {
    expect(pasteAsList("O'Brien\n\nO'Brien\r\nAna")).toBe("('O''Brien', 'O''Brien', 'Ana')");
  });

  it("toma una fila separada por tabuladores como una lista", () => {
    expect(pasteAsList("a\tb\tc")).toBe("('a', 'b', 'c')");
  });

  it("no toca una sola línea, una tabla de dos dimensiones ni SQL", () => {
    expect(pasteAsList("hola mundo")).toBeNull();
    expect(pasteAsList("a\tb\nc\td")).toBeNull();
    expect(pasteAsList("SELECT 1\nFROM t")).toBeNull();
    expect(pasteAsList("a;\nb")).toBeNull();
    expect(pasteAsList("-- nota\nb")).toBeNull();
  });
});

describe("transformText", () => {
  it("pasa a mayúsculas y minúsculas texto común sin más", () => {
    expect(transformText("upper", "hola mundo")).toBe("HOLA MUNDO");
    expect(transformText("lower", "HOLA MUNDO")).toBe("hola mundo");
    // Un apóstrofe de una frase no es una cadena SQL: no protege el resto.
    expect(transformText("upper", "don't stop")).toBe("DON'T STOP");
  });

  it("sobre SQL no toca literales, identificadores con comillas, comentarios ni $$", () => {
    const sql = "select \"Mi Col\", 'Ana' from t -- Nota\n";
    expect(transformText("upper", sql)).toBe("SELECT \"Mi Col\", 'Ana' FROM T -- Nota\n");
    expect(transformText("lower", "SELECT 'Ana' FROM T WHERE X = $$AbC$$")).toBe(
      "select 'Ana' from t where x = $$AbC$$",
    );
  });

  it("une las líneas con coma", () => {
    expect(transformText("joinComma", "a\n\n b \nc")).toBe("a, b, c");
  });

  it("cita cada línea y conserva las vacías", () => {
    expect(transformText("quoteLines", "a\n\nO'B")).toBe("'a'\n\n'O''B'");
  });

  it("quita las comillas simples y deja las dobles", () => {
    expect(transformText("unquote", "'a', 'O''B', \"x\"")).toBe("a, O'B, \"x\"");
  });

  it("arma una lista IN desde líneas o desde una línea con comas", () => {
    expect(transformText("inList", "a\nb")).toBe("('a', 'b')");
    expect(transformText("inList", "1, 2, 3")).toBe("(1, 2, 3)");
    expect(transformText("inList", "'a', 'b'")).toBe("('a', 'b')");
    expect(transformText("inList", "solo")).toBe("('solo')");
  });

  it("recorta el final de cada línea y los saltos de los extremos, no la sangría", () => {
    expect(transformText("trim", "\n\n  a  \n  b\t\n\n")).toBe("  a\n  b");
  });

  it("ordena sin distinguir mayúsculas y con números naturales", () => {
    expect(transformText("sortLines", "b\nA\na10\na2")).toBe("A\na2\na10\nb");
  });

  it("quita las líneas repetidas y deja la primera aparición", () => {
    expect(transformText("uniqueLines", "a\nb\na\nc\nb")).toBe("a\nb\nc");
  });

  it("respeta el fin de línea del texto", () => {
    expect(transformText("sortLines", "b\r\na")).toBe("a\r\nb");
  });
});
