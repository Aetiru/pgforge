import { describe, expect, it } from "vitest";

import { geometryText, geometryTextOrNull, isGeometryText, isGeometryType } from "./geometry";

describe("isGeometryType", () => {
  it("reconoce geometry y geography, sin distinguir mayúsculas ni espacios", () => {
    expect(isGeometryType("geometry")).toBe(true);
    expect(isGeometryType("Geometry")).toBe(true);
    expect(isGeometryType(" geography ")).toBe(true);
    expect(isGeometryType("GEOGRAPHY")).toBe(true);
  });

  it("le saca el modificador que agrega PostGIS al tipo", () => {
    // `format_type()` (crates/pgforge-core/src/data/shape.rs) escribe el modificador de la columna
    // pegado al tipo — es el caso más común, una columna con SRID y forma tipados.
    expect(isGeometryType("geometry(Point,4326)")).toBe(true);
    expect(isGeometryType("geography(Point)")).toBe(true);
  });

  it("le saca el esquema, con o sin comillas", () => {
    // `extensions.geometry` es como aparece en Supabase, que no instala PostGIS en `public`.
    expect(isGeometryType("extensions.geometry")).toBe(true);
    expect(isGeometryType("public.geography")).toBe(true);
    expect(isGeometryType(`"My Schema".geometry`)).toBe(true);
  });

  it("las dos cosas juntas: esquema y modificador", () => {
    expect(isGeometryType("extensions.geometry(Point,4326)")).toBe(true);
  });

  it("no reconoce ningún otro tipo", () => {
    expect(isGeometryType("text")).toBe(false);
    expect(isGeometryType("public.estado")).toBe(false);
    expect(isGeometryType(null)).toBe(false);
    expect(isGeometryType(undefined)).toBe(false);
  });
});

describe("geometryText — vectores reales de PostGIS", () => {
  it("decodifica un POINT sin SRID", () => {
    expect(geometryText("0101000000000000000000F03F000000000000F03F")).toBe("POINT (1 1)");
  });

  it("decodifica un POINT con SRID como EWKT", () => {
    expect(
      geometryText("0101000020E6100000000000000000F03F000000000000F03F"),
    ).toBe("SRID=4326;POINT (1 1)");
  });

  it("decodifica un LINESTRING", () => {
    expect(
      geometryText("01020000000200000000000000000000000000000000000000000000000000F03F000000000000F03F"),
    ).toBe("LINESTRING (0 0, 1 1)");
  });

  it("un hex inválido o truncado se devuelve tal como vino", () => {
    const truncado = "0101000000000000000000F03F0000000000";
    expect(geometryText(truncado)).toBe(truncado);
  });

  it("recorta el texto largo con puntos suspensivos, igual que format.oneLine", () => {
    const hex = lineStringHex(50);
    const full = geometryText(hex, 100_000);
    expect(full.length).toBeGreaterThan(40);
    expect(geometryText(hex, 40)).toBe(`${full.slice(0, 40)}…`);
  });

  it("bytes de más después de una geometría válida no decodifican en silencio", () => {
    // Mismo hex del primer POINT, con un byte pegado atrás: la estructura de la geometría termina
    // bien, pero sobra algo que no debería estar — no es un caso de «se ve raro pero se entiende».
    const conBasura = "0101000000000000000000F03F000000000000F03F" + "00";
    expect(geometryText(conBasura)).toBe(conBasura);
    expect(geometryTextOrNull(conBasura)).toBeNull();
    expect(isGeometryText(conBasura)).toBe(false);
  });
});

describe("geometryText — dimensiones (Z, M, ZM)", () => {
  it("un POINT con Z lleva la tercera coordenada y el sufijo con espacio", () => {
    const hex = toHex(geometryBytes(1, pointBody(1, 1, 3), { z: true }));
    expect(geometryText(hex)).toBe("POINT Z (1 1 3)");
  });

  it("un POINT con M no muestra la M —no hace falta— pero sí avisa en el nombre, pegada sin espacio", () => {
    const hex = toHex(geometryBytes(1, pointBody(1, 1, undefined, 9), { m: true }));
    expect(geometryText(hex)).toBe("POINTM (1 1)");
  });

  it("un POINT con Z y M muestra la Z y avisa las dos dimensiones en el nombre", () => {
    const hex = toHex(geometryBytes(1, pointBody(1, 1, 3, 9), { z: true, m: true }));
    expect(geometryText(hex)).toBe("POINT ZM (1 1 3)");
  });
});

describe("geometryText — geometrías vacías", () => {
  it("un POINT en NaN, NaN es POINT EMPTY, no POINT (NaN NaN)", () => {
    const hex = toHex(geometryBytes(1, pointBody(NaN, NaN)));
    expect(geometryText(hex)).toBe("POINT EMPTY");
  });

  it("un MULTIPOINT sin elementos es MULTIPOINT EMPTY, no MULTIPOINT ()", () => {
    const hex = toHex(geometryBytes(4, multiElementsBody([])));
    expect(geometryText(hex)).toBe("MULTIPOINT EMPTY");
  });

  it("una GEOMETRYCOLLECTION sin elementos es GEOMETRYCOLLECTION EMPTY", () => {
    const hex = toHex(geometryBytes(7, multiElementsBody([])));
    expect(geometryText(hex)).toBe("GEOMETRYCOLLECTION EMPTY");
  });
});

describe("geometryText — MULTI* y GEOMETRYCOLLECTION", () => {
  it("un MULTIPOINT válido junta el cuerpo de cada Point, sin repetir el nombre", () => {
    const elements = [geometryBytes(1, pointBody(1, 2)), geometryBytes(1, pointBody(3, 4))];
    const hex = toHex(geometryBytes(4, multiElementsBody(elements)));
    expect(geometryText(hex)).toBe("MULTIPOINT ((1 2), (3 4))");
  });

  it("una GEOMETRYCOLLECTION puede mezclar tipos, cada uno con su propio nombre", () => {
    const elements = [
      geometryBytes(1, pointBody(1, 1)),
      geometryBytes(2, lineStringBody([[0, 0], [1, 1]])),
    ];
    const hex = toHex(geometryBytes(7, multiElementsBody(elements)));
    expect(geometryText(hex)).toBe("GEOMETRYCOLLECTION (POINT (1 1), LINESTRING (0 0, 1 1))");
  });

  it("un MULTIPOINT con un elemento que no es Point no se muestra como si lo fuera", () => {
    // Un LINESTRING adentro de un MULTIPOINT no es una rareza válida: PostGIS nunca lo emite así.
    const elements = [geometryBytes(2, lineStringBody([[0, 0], [1, 1]]))];
    const hex = toHex(geometryBytes(4, multiElementsBody(elements)));
    expect(geometryTextOrNull(hex)).toBeNull();
    expect(geometryText(hex)).toBe(hex);
    expect(isGeometryText(hex)).toBe(false);
  });

  it("un MULTIPOINT cuyo elemento tiene Z y el contenedor no, tampoco se acepta", () => {
    const elements = [geometryBytes(1, pointBody(1, 1, 3), { z: true })];
    const hex = toHex(geometryBytes(4, multiElementsBody(elements)));
    expect(geometryTextOrNull(hex)).toBeNull();
  });

  it("una sub-geometría con su propio SRID es dato corrupto y no se interpreta desalineada", () => {
    // PostGIS nunca emite SRID en un elemento anidado —solo la geometría de más afuera lo trae—,
    // así que un elemento con ese bit prendido es corrupción, no una variante rara a tolerar.
    const elementConSrid = geometryBytes(1, pointBody(1, 1), { srid: 4326 });
    const hex = toHex(geometryBytes(4, multiElementsBody([elementConSrid])));
    expect(geometryTextOrNull(hex)).toBeNull();
  });

  it("una colección anidada razonable decodifica bien", () => {
    const hex = toHex(nestedCollection(3));
    expect(geometryText(hex)).toBe(
      "GEOMETRYCOLLECTION (GEOMETRYCOLLECTION (GEOMETRYCOLLECTION (POINT (1 1))))",
    );
  });

  it("una colección anidada patológica no cuelga la interfaz: hay un tope de profundidad", () => {
    const hex = toHex(nestedCollection(50));
    expect(geometryTextOrNull(hex)).toBeNull();
  });
});

describe("isGeometryText", () => {
  it("acepta el hex de un POINT válido", () => {
    expect(isGeometryText("0101000000000000000000F03F000000000000F03F")).toBe(true);
  });

  it("rechaza un hex corto o de largo impar", () => {
    expect(isGeometryText("01F03F")).toBe(false);
    expect(isGeometryText("0101000000000000000000F03F000000000000F0")).toBe(false);
  });

  it("rechaza un hex que no empieza con un byte de orden válido", () => {
    // "DE" no es 00 ni 01: mismo caso que un bytea cualquiera cuyo primer byte no coincide.
    expect(isGeometryText("DEADBEEFDEADBEEFDEADBEEF")).toBe(false);
  });

  it("rechaza un bytea que parece EWKB pero no decodifica como geometría real", () => {
    // Mismo byte de orden y largo par que una geometría válida, pero con un tipo que no existe
    // (99): pasa el filtro rápido y hace falta el decodificador entero para descartarlo.
    expect(isGeometryText("0163000000000000000000F03F000000000000F03F")).toBe(false);
  });

  it("rechaza texto que no es hexadecimal", () => {
    expect(isGeometryText("no es hex")).toBe(false);
  });
});

/** Arma el EWKB (hex, LE, sin SRID) de un `LINESTRING` con `count` puntos, para probar el recorte
 *  de `geometryText` sin escribir a mano un hex larguísimo. */
function lineStringHex(count: number): string {
  const points: [number, number][] = Array.from({ length: count }, (_, i) => [i, i]);
  return toHex(geometryBytes(2, lineStringBody(points)));
}

// ---------------------------------------------------------------------------
// Un armador de EWKB minimalista para los casos que no vienen de un hex real conocido —cada
// función arma exactamente los bytes que describe el comentario de cabecera de `geometry.ts`,
// deliberadamente sin reusar el decodificador: si los dos coincidieran por compartir código, un
// error de lectura y uno de escritura podrían cancelarse entre sí sin que el test lo note.
// ---------------------------------------------------------------------------

function u32le(value: number): number[] {
  const buffer = new ArrayBuffer(4);
  new DataView(buffer).setUint32(0, value >>> 0, true);
  return Array.from(new Uint8Array(buffer));
}

function f64le(value: number): number[] {
  const buffer = new ArrayBuffer(8);
  new DataView(buffer).setFloat64(0, value, true);
  return Array.from(new Uint8Array(buffer));
}

function toHex(bytes: number[]): string {
  return bytes.map((byte) => (byte & 0xff).toString(16).padStart(2, "0")).join("");
}

/** Cabecera + cuerpo de una geometría completa: orden, tipo con sus flags, el SRID si se pide, y el
 *  cuerpo ya armado. Sirve tanto para la geometría de más afuera como para un elemento anidado —el
 *  único que nunca debería llevar `srid` es el anidado, y por eso algún test lo fuerza a propósito. */
function geometryBytes(
  baseType: number,
  body: number[],
  opts: { z?: boolean; m?: boolean; srid?: number } = {},
): number[] {
  let type = baseType;
  if (opts.z) type |= 0x80000000;
  if (opts.m) type |= 0x40000000;
  if (opts.srid !== undefined) type |= 0x20000000;

  const bytes = [1, ...u32le(type >>> 0)];
  if (opts.srid !== undefined) bytes.push(...u32le(opts.srid));
  bytes.push(...body);
  return bytes;
}

function pointBody(x: number, y: number, z?: number, m?: number): number[] {
  const bytes = [...f64le(x), ...f64le(y)];
  if (z !== undefined) bytes.push(...f64le(z));
  if (m !== undefined) bytes.push(...f64le(m));
  return bytes;
}

function lineStringBody(points: [number, number][]): number[] {
  return [...u32le(points.length), ...points.flatMap(([x, y]) => [...f64le(x), ...f64le(y)])];
}

/** El cuerpo de un `MULTI*`/`GEOMETRYCOLLECTION`: la cantidad seguida de cada elemento, ya
 *  serializado con `geometryBytes` (con su propio orden y tipo). */
function multiElementsBody(elements: number[][]): number[] {
  return [...u32le(elements.length), ...elements.flat()];
}

/** `levels` `GEOMETRYCOLLECTION` anidadas, cada una con un solo elemento, hasta llegar a un
 *  `POINT (1 1)` en el centro — para probar tanto el caso razonable como el tope de profundidad. */
function nestedCollection(levels: number): number[] {
  let inner = geometryBytes(1, pointBody(1, 1));
  for (let i = 0; i < levels; i += 1) {
    inner = geometryBytes(7, multiElementsBody([inner]));
  }
  return inner;
}
