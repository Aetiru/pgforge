/**
 * Decodifica columnas `geometry`/`geography` (extensión PostGIS) de hex EWKB a un texto WKT/EWKT
 * legible.
 *
 * `sql::exec` ejecuta con el protocolo simple a propósito, así cualquier tipo de cualquier
 * extensión se puede mostrar sin que el núcleo lo conozca, delegando el formato a la función de
 * salida del servidor (ver comentario de `crates/pgforge-core/src/sql/exec.rs`). Para `geometry` y
 * `geography` esa función de salida es hex EWKB, no WKT, así que el texto que llega («0101...») es
 * correcto pero ilegible; acá se decodifica para mostrarlo, sin tocar lo que viaja de verdad
 * (`Column.raw` sigue siendo el hex completo, igual que documenta `DataGrid.svelte`).
 */

const GEOMETRY_TYPES = new Set(["geometry", "geography"]);

/**
 * `column.typeName` sale de `format_type()` (`crates/pgforge-core/src/data/shape.rs`), que devuelve
 * cosas como `geometry(Point,4326)` —con el modificador que PostGIS le agrega al tipo— o
 * `extensions.geometry` —con el esquema por delante, como en Supabase—. Hay que sacarse los dos
 * antes de comparar contra el nombre pelado.
 */
function unqualifiedTypeName(type: string): string {
  const paren = type.indexOf("(");
  const base = (paren === -1 ? type : type.slice(0, paren)).trim();

  // El esquema puede venir citado y un identificador citado puede traer un punto adentro —de ahí
  // que no alcance un `lastIndexOf(".")` crudo—; se recorre de atrás para adelante y un punto
  // adentro de comillas no cuenta como separador.
  let quoted = false;
  for (let i = base.length - 1; i >= 0; i -= 1) {
    const char = base[i];
    if (char === '"') quoted = !quoted;
    else if (char === "." && !quoted) return base.slice(i + 1);
  }
  return base;
}

export function isGeometryType(type: string | null | undefined): boolean {
  if (type === null || type === undefined) return false;
  return GEOMETRY_TYPES.has(unqualifiedTypeName(type).trim().toLowerCase());
}

// Bits altos del campo «tipo + flags» de EWKB. El tipo base ocupa los 29 bits que quedan.
const Z_FLAG = 0x80000000;
const M_FLAG = 0x40000000;
const SRID_FLAG = 0x20000000;
const BASE_TYPE_MASK = 0x1fffffff;

const TYPE_NAMES: Record<number, string> = {
  1: "POINT",
  2: "LINESTRING",
  3: "POLYGON",
  4: "MULTIPOINT",
  5: "MULTILINESTRING",
  6: "MULTIPOLYGON",
  7: "GEOMETRYCOLLECTION",
};

// Una colección adentro de otra, adentro de otra... una cadena patológica no puede costar una pila
// de miles de cuadros por celda dibujada. PostGIS no genera geometrías así de profundas.
const MAX_DEPTH = 32;

/** Convierte el hex del servidor en bytes. Un largo impar o un carácter no hexadecimal ya dice que
 *  no es EWKB. */
function hexToBytes(hex: string): Uint8Array {
  if (hex.length === 0 || hex.length % 2 !== 0 || !/^[0-9a-fA-F]+$/.test(hex)) {
    throw new Error("no es hexadecimal de largo par");
  }
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i += 1) {
    bytes[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}

/**
 * Lee los campos binarios de EWKB respetando el orden de bytes que indica cada geometría. El orden
 * se fija con `order()` antes de leer nada más, y una sub-geometría anidada (dentro de un
 * `MULTI*`/`GEOMETRYCOLLECTION`) trae el suyo propio, que puede en teoría diferir del externo —en la
 * práctica PostGIS siempre usa el mismo—.
 */
class Reader {
  private readonly view: DataView;
  private pos = 0;
  private littleEndian = true;

  constructor(bytes: Uint8Array) {
    this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  }

  /** Bytes que quedan sin leer. Si al terminar de decodificar la geometría de más afuera sobra
   *  algo, no es EWKB válido — es EWKB válido con basura pegada atrás, y mostrarlo igual sería
   *  esconder que el dato no es el que dice ser. */
  get remaining(): number {
    return this.view.byteLength - this.pos;
  }

  order(): void {
    const byte = this.view.getUint8(this.pos);
    this.pos += 1;
    if (byte !== 0 && byte !== 1) throw new Error(`orden de bytes inválido: ${byte}`);
    this.littleEndian = byte === 1;
  }

  uint32(): number {
    const value = this.view.getUint32(this.pos, this.littleEndian);
    this.pos += 4;
    return value;
  }

  float64(): number {
    const value = this.view.getFloat64(this.pos, this.littleEndian);
    this.pos += 8;
    return value;
  }
}

function typeName(baseType: number): string {
  const name = TYPE_NAMES[baseType];
  if (name === undefined) throw new Error(`tipo de geometría desconocido: ${baseType}`);
  return name;
}

/**
 * El sufijo que EWKT le agrega al nombre del tipo cuando la geometría trae Z y/o M —convención de
 * PostGIS, no la del WKT estándar—: pegado sin espacio cuando es solo M (`POINTM`), con espacio
 * cuando hay Z, con o sin M (`POINT Z`, `POINT ZM`). Mostrar menos dimensiones que las reales sin
 * avisar sería inventar un dato que el hex no dice.
 */
function dimensionSuffix(hasZ: boolean, hasM: boolean): string {
  if (hasZ && hasM) return " ZM";
  if (hasZ) return " Z";
  if (hasM) return "M";
  return "";
}

interface Coordinate {
  x: number;
  y: number;
  /** Texto ya armado: `"x y"` o `"x y z"`. La M se lee para no perder la alineación de lo que
   *  sigue, pero no entra acá — el texto de salida no la necesita. */
  text: string;
}

function readCoordinate(reader: Reader, hasZ: boolean, hasM: boolean): Coordinate {
  const x = reader.float64();
  const y = reader.float64();
  const z = hasZ ? reader.float64() : undefined;
  if (hasM) reader.float64();
  return { x, y, text: z === undefined ? `${x} ${y}` : `${x} ${y} ${z}` };
}

/** La lista de puntos de un `LineString` o de un anillo de `Polygon`, sin los paréntesis que los
 *  agrupan — eso lo decide quien llama, porque un anillo se junta con otros y una lista sola no. */
function readCoordinateList(reader: Reader, hasZ: boolean, hasM: boolean): string[] {
  const count = reader.uint32();
  const points: string[] = [];
  for (let i = 0; i < count; i += 1) points.push(readCoordinate(reader, hasZ, hasM).text);
  return points;
}

interface Body {
  /** Si PostGIS la codificó vacía: un `Point` en `NaN NaN`, o cualquier lista con cero elementos.
   *  En WKT eso se escribe `EMPTY`, no con paréntesis vacíos ni con `NaN` a la vista. */
  empty: boolean;
  /** El texto ya entre paréntesis, listo para pegar después de `NOMBRE[ sufijo]`. Vacío si
   *  `empty` es cierto — ahí no hay nada que mostrar entre paréntesis. */
  text: string;
}

interface ElementHeader {
  baseType: number;
  hasZ: boolean;
  hasM: boolean;
}

/** El encabezado de un elemento de `MULTI*`/`GEOMETRYCOLLECTION`: orden y tipo propios, como
 *  cualquier WKB completo. Nunca trae SRID —eso solo va en la geometría de más afuera—, y si el bit
 *  está prendido es dato corrupto: PostGIS no lo emite así, así que interpretarlo iría desalineado
 *  en vez de avisar. */
function readElementHeader(reader: Reader): ElementHeader {
  reader.order();
  const typeAndFlags = reader.uint32();
  if ((typeAndFlags & SRID_FLAG) !== 0) {
    throw new Error("una sub-geometría no puede traer su propio SRID");
  }
  return {
    baseType: typeAndFlags & BASE_TYPE_MASK,
    hasZ: (typeAndFlags & Z_FLAG) !== 0,
    hasM: (typeAndFlags & M_FLAG) !== 0,
  };
}

function readBody(reader: Reader, baseType: number, hasZ: boolean, hasM: boolean, depth: number): Body {
  if (depth > MAX_DEPTH) throw new Error("geometría anidada demasiado profundo");

  switch (baseType) {
    case 1: {
      const point = readCoordinate(reader, hasZ, hasM);
      const empty = Number.isNaN(point.x) && Number.isNaN(point.y);
      return empty ? { empty: true, text: "" } : { empty: false, text: `(${point.text})` };
    }
    case 2: {
      const points = readCoordinateList(reader, hasZ, hasM);
      return points.length === 0
        ? { empty: true, text: "" }
        : { empty: false, text: `(${points.join(", ")})` };
    }
    case 3: {
      const ringCount = reader.uint32();
      if (ringCount === 0) return { empty: true, text: "" };
      const rings: string[] = [];
      for (let i = 0; i < ringCount; i += 1) {
        rings.push(`(${readCoordinateList(reader, hasZ, hasM).join(", ")})`);
      }
      return { empty: false, text: `(${rings.join(", ")})` };
    }
    // MultiPoint, MultiLineString y MultiPolygon: cada elemento es una geometría WKB completa, pero
    // homogénea con el contenedor —mismo subtipo (Point, LineString, Polygon) y mismas dimensiones—,
    // así que en el WKT solo se escribe su cuerpo, no su nombre. Un elemento que no coincide no es
    // una rareza válida: es un dato que no se puede representar como el tipo dice ser.
    case 4:
    case 5:
    case 6: {
      const expectedSubType = baseType - 3;
      const count = reader.uint32();
      if (count === 0) return { empty: true, text: "" };
      const parts: string[] = [];
      for (let i = 0; i < count; i += 1) {
        const header = readElementHeader(reader);
        if (header.baseType !== expectedSubType || header.hasZ !== hasZ || header.hasM !== hasM) {
          throw new Error(
            `elemento de ${typeName(baseType)} no coincide con el tipo o las dimensiones del contenedor`,
          );
        }
        const sub = readBody(reader, header.baseType, header.hasZ, header.hasM, depth + 1);
        parts.push(sub.empty ? "EMPTY" : sub.text);
      }
      return { empty: false, text: `(${parts.join(", ")})` };
    }
    // Una colección sí puede mezclar tipos distintos, así que cada elemento se escribe con su
    // propio nombre y su propio sufijo de dimensión.
    case 7: {
      const count = reader.uint32();
      if (count === 0) return { empty: true, text: "" };
      const parts: string[] = [];
      for (let i = 0; i < count; i += 1) {
        const header = readElementHeader(reader);
        const sub = readBody(reader, header.baseType, header.hasZ, header.hasM, depth + 1);
        const name = `${typeName(header.baseType)}${dimensionSuffix(header.hasZ, header.hasM)}`;
        parts.push(sub.empty ? `${name} EMPTY` : `${name} ${sub.text}`);
      }
      return { empty: false, text: `(${parts.join(", ")})` };
    }
    default:
      throw new Error(`tipo de geometría no soportado: ${baseType}`);
  }
}

/**
 * Decodifica el EWKB entero, con el SRID de la geometría de más afuera si lo trae. Tira si el hex
 * no es EWKB válido, describe un tipo que esta función no conoce o le sobran bytes al final —el
 * llamador decide qué hacer con eso, nunca corrompe el dato.
 */
function decodeEwkb(hex: string): string {
  const reader = new Reader(hexToBytes(hex));
  reader.order();
  const typeAndFlags = reader.uint32();
  const hasZ = (typeAndFlags & Z_FLAG) !== 0;
  const hasM = (typeAndFlags & M_FLAG) !== 0;
  const hasSrid = (typeAndFlags & SRID_FLAG) !== 0;
  const baseType = typeAndFlags & BASE_TYPE_MASK;
  const srid = hasSrid ? reader.uint32() : undefined;

  const body = readBody(reader, baseType, hasZ, hasM, 0);
  // Sin esto, una geometría válida con basura pegada atrás decodificaba «bien» hasta donde
  // alcanzaba su propia estructura y la basura desaparecía en silencio.
  if (reader.remaining !== 0) throw new Error("sobran bytes después de la geometría");

  const name = `${typeName(baseType)}${dimensionSuffix(hasZ, hasM)}`;
  const wkt = body.empty ? `${name} EMPTY` : `${name} ${body.text}`;
  return srid === undefined ? wkt : `SRID=${srid};${wkt}`;
}

/**
 * Decodifica una sola vez y sirve tanto para saber si el valor es geometría como para el texto a
 * mostrar — llamarlo dos veces (una para preguntar, otra para formatear) decodificaría el mismo hex
 * dos veces por celda dibujada, y eso corre en cada cuadro del desplazamiento. `null` si el hex no
 * decodifica: tipo no soportado, dato corrupto, versión rara de PostGIS o —cuando no se conoce el
 * tipo de columna— un `bytea` cualquiera que solo por casualidad se parece a EWKB.
 */
export function geometryTextOrNull(value: string, max = 200): string | null {
  let decoded: string;
  try {
    decoded = decodeEwkb(value.trim());
  } catch {
    return null;
  }
  return decoded.length > max ? `${decoded.slice(0, max)}…` : decoded;
}

/**
 * Heurística para cuando no se conoce el tipo de columna — el resultado de una consulta no siempre
 * trae tipos, ver comentario de `types` en `ResultGrid.svelte`. No alcanza con «parece hex»: un
 * `bytea` cualquiera también lo es, así que además de la forma (largo par, de al menos dieciocho
 * caracteres, byte de orden válido) se exige que decodifique sin error, byte a byte hasta el final,
 * como geometría de verdad.
 */
export function isGeometryText(value: string): boolean {
  const text = value.trim();
  if (text.length < 18 || text.length % 2 !== 0) return false;
  if (!/^[0-9a-fA-F]+$/.test(text)) return false;
  const order = text.slice(0, 2).toLowerCase();
  if (order !== "00" && order !== "01") return false;

  return geometryTextOrNull(text) !== null;
}

/**
 * El hex EWKB como se lee, no como viaja — mismo espíritu que `boolText` en `format.ts`, pero acá
 * el «como viaja» es del todo ilegible en vez de solo poco cómodo.
 *
 * Nunca tira: si el hex no decodifica (tipo no soportado, dato corrupto, versión rara de PostGIS),
 * devuelve `value` tal como vino. Mismo criterio que `crates/pgforge-core/src/sql/format.rs` — el
 * peor caso es «no formateó», nunca «formateó mal».
 */
export function geometryText(value: string, max = 200): string {
  return geometryTextOrNull(value, max) ?? value;
}
