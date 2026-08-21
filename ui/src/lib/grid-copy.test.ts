import { describe, expect, it } from "vitest";

import { asJson, delimited, parseDelimited, pretty, quoted } from "./grid-copy";

/**
 * Lo que se copia de la grilla termina pegado en otra herramienta, así que un valor mal encerrado no
 * se ve acá sino allá, con las columnas corridas. Estos casos son los que rompen: la coma adentro de
 * un texto, la comilla, el salto de línea y el NULL.
 */

describe("quoted", () => {
  it("deja en paz lo que no tiene el separador adentro", () => {
    expect(quoted("clientes", ",")).toBe("clientes");
    expect(quoted("hola mundo", "\t")).toBe("hola mundo");
  });

  it("encierra lo que llevaría el separador a otra celda", () => {
    expect(quoted("Rosario, Santa Fe", ",")).toBe('"Rosario, Santa Fe"');
    expect(quoted("uno\tdos", "\t")).toBe('"uno\tdos"');
  });

  it("duplica las comillas, que es como se escapan en una planilla", () => {
    expect(quoted('dijo "hola"', ",")).toBe('"dijo ""hola"""');
  });

  it("encierra el salto de línea aunque no haya separador", () => {
    expect(quoted("primera\nsegunda", ",")).toBe('"primera\nsegunda"');
  });
});

describe("delimited", () => {
  const cells = [
    ["1", "Ana", null],
    ["2", "Luis, hijo", "x"],
  ];

  it("sin encabezados y con tabulaciones es lo que espera una planilla al pegar", () => {
    expect(delimited(null, cells, "\t")).toBe("1\tAna\t\n2\tLuis, hijo\tx");
  });

  it("con encabezados y comas es lo que se guarda en un archivo", () => {
    expect(delimited(["id", "nombre", "nota"], cells, ",")).toBe(
      'id,nombre,nota\n1,Ana,\n2,"Luis, hijo",x',
    );
  });

  it("el NULL se copia como celda vacía y no como el texto de la pantalla", () => {
    expect(delimited(null, [[null]], ",")).toBe("");
  });
});

describe("parseDelimited", () => {
  it("separa filas y celdas sin comillas", () => {
    expect(parseDelimited("1\tAna\n2\tLuis", "\t")).toEqual([
      ["1", "Ana"],
      ["2", "Luis"],
    ]);
  });

  it("deshace las comillas del separador adentro de un valor", () => {
    expect(parseDelimited('id,nombre\n1,"Rosario, Santa Fe"', ",")).toEqual([
      ["id", "nombre"],
      ["1", "Rosario, Santa Fe"],
    ]);
  });

  it("deshace la comilla escapada", () => {
    expect(parseDelimited('"dijo ""hola"""', ",")).toEqual([['dijo "hola"']]);
  });

  it("un salto de línea adentro de comillas no corta la fila", () => {
    expect(parseDelimited('"primera\nsegunda"\tx', "\t")).toEqual([["primera\nsegunda", "x"]]);
  });

  it("el salto de línea final del portapapeles no cuenta como fila vacía pegada", () => {
    expect(parseDelimited("1\tAna\n2\tLuis\n", "\t")).toEqual([
      ["1", "Ana"],
      ["2", "Luis"],
    ]);
  });

  it("da la vuelta completa con `delimited` para los casos que rompen", () => {
    const cells = [
      ["1", "Ana", "dijo \"hola\""],
      ["2", "Luis, hijo", "primera\nsegunda"],
    ];
    expect(parseDelimited(delimited(null, cells, ","), ",")).toEqual(cells);
  });
});

describe("asJson", () => {
  it("arma un objeto por fila y conserva el NULL", () => {
    const text = asJson(["id", "nombre"], [["1", null]]);
    expect(JSON.parse(text)).toEqual([{ id: "1", nombre: null }]);
  });
});

describe("pretty", () => {
  it("indenta el JSON que viene en una sola línea", () => {
    expect(pretty('{"a":1}')).toBe('{\n  "a": 1\n}');
  });

  it("no toca lo que no es JSON, ni siquiera si empieza parecido", () => {
    expect(pretty("hola")).toBe("hola");
    expect(pretty("{esto no cierra")).toBe("{esto no cierra");
  });
});
