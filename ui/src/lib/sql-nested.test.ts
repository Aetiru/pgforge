import { PostgreSQL } from "@codemirror/lang-sql";
import { EditorState } from "@codemirror/state";
import { Tree, TreeFragment } from "@lezer/common";
import { describe, expect, it } from "vitest";
import { dollarBlocks, localChanges, pairBodies } from "./sql-nested";

/** El cuerpo tal como sale del texto, que es lo que después se vuelve a colorear. */
function bodies(sql: string): string[] {
  return dollarBlocks(sql).map((block) => sql.slice(block.from, block.to));
}

describe("dollarBlocks", () => {
  it("encuentra el cuerpo de una función delimitado con $$", () => {
    const sql = "CREATE FUNCTION f() RETURNS int AS $$ SELECT 1 $$ LANGUAGE sql;";
    expect(bodies(sql)).toEqual([" SELECT 1 "]);
  });

  it("respeta la etiqueta: un $$ adentro de un $body$ no lo cierra", () => {
    const sql = "AS $body$ SELECT '$$'; $body$";
    expect(dollarBlocks(sql)).toEqual([{ from: 9, to: 23, tag: "body" }]);
    expect(bodies(sql)).toEqual([" SELECT '$$'; "]);
  });

  it("toma hasta el final lo que quedó sin cerrar, que es la función a medio escribir", () => {
    const sql = "AS $$ SELECT ";
    expect(bodies(sql)).toEqual([" SELECT "]);
  });

  it("no abre un cuerpo con el $$ que hay adentro de una cadena", () => {
    expect(dollarBlocks("SELECT 'precio en $$' FROM caja")).toEqual([]);
  });

  it("no abre un cuerpo con el $$ de un comentario", () => {
    expect(dollarBlocks("-- ojo con $$\nSELECT 1")).toEqual([]);
    expect(dollarBlocks("/* $$ */ SELECT 1")).toEqual([]);
  });

  it("atraviesa los comentarios de bloque anidados, que en PostgreSQL sí anidan", () => {
    const sql = "/* a /* b */ $$ */ AS $$ SELECT 1 $$";
    expect(bodies(sql)).toEqual([" SELECT 1 "]);
  });

  it("no confunde un parámetro posicional con una etiqueta", () => {
    expect(dollarBlocks("SELECT * FROM t WHERE id = $1 AND n = $2")).toEqual([]);
  });

  it("encuentra varios cuerpos en el mismo script", () => {
    const sql = "AS $$ uno $$;\nAS $tag$ dos $tag$;";
    expect(bodies(sql)).toEqual([" uno ", " dos "]);
  });

  it("las comillas dobles duplicadas no cierran el identificador", () => {
    expect(dollarBlocks('SELECT "raro""$$" FROM t')).toEqual([]);
  });
});

describe("dollarBlocks sobre lo que devuelve pg_get_functiondef", () => {
  // Tal cual sale de `pg_get_functiondef`, que es lo que muestra el panel de DDL: la etiqueta es
  // `$function$` y no `$$`, y antes del cuerpo hay comillas —el `DEFAULT` de un argumento, el
  // `SET search_path`— que el escaneo tiene que atravesar sin perder el hilo.
  const ddl = `CREATE OR REPLACE FUNCTION public.probe_fn(p_id integer, p_txt text DEFAULT 'x'::text)
 RETURNS TABLE(id integer, saldo numeric)
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public'
AS $function$
DECLARE
  v text := 'hola';
BEGIN
  RETURN QUERY SELECT 1 WHERE p_txt = v;
END
$function$`;

  it("encuentra el cuerpo entero y solo el cuerpo", () => {
    const blocks = dollarBlocks(ddl);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].tag).toBe("function");
    expect(ddl.slice(blocks[0].from, blocks[0].to)).toBe(
      "\nDECLARE\n  v text := 'hola';\nBEGIN\n  RETURN QUERY SELECT 1 WHERE p_txt = v;\nEND\n",
    );
  });
});

/** Un cambio de texto tal como lo entrega `ChangeSet.iterChangedRanges`. */
function edit(text: string, from: number, to: number, insert: string) {
  const newText = text.slice(0, from) + insert + text.slice(to);
  return { newText, change: { fromA: from, toA: to, fromB: from, toB: from + insert.length } };
}

describe("localChanges", () => {
  // Cuerpo de dos letras: alcanza para distinguir "antes", "adentro" y "después" sin ruido.
  const oldText = "AS $$ab$$ Z";
  const oldBlock = dollarBlocks(oldText)[0];

  it("un cambio adentro del cuerpo queda en coordenadas del cuerpo", () => {
    const { newText, change } = edit(oldText, 6, 6, "X"); // entre la 'a' y la 'b'
    const nextBlock = dollarBlocks(newText)[0];
    expect(localChanges([change], oldBlock, nextBlock)).toEqual([
      { fromA: 1, toA: 1, fromB: 1, toB: 2 },
    ]);
  });

  it("escribir al principio del cuerpo cuenta como adentro", () => {
    const { newText, change } = edit(oldText, oldBlock.from, oldBlock.from, "X");
    const nextBlock = dollarBlocks(newText)[0];
    expect(localChanges([change], oldBlock, nextBlock)).toEqual([
      { fromA: 0, toA: 0, fromB: 0, toB: 1 },
    ]);
  });

  it("un cambio antes del bloque corre las posiciones y no deja rangos locales", () => {
    const { newText, change } = edit(oldText, 0, 0, "XYZ");
    const nextBlock = dollarBlocks(newText)[0];
    expect(localChanges([change], oldBlock, nextBlock)).toEqual([]);
  });

  it("un cambio después del bloque no deja rangos locales", () => {
    const { newText, change } = edit(oldText, oldText.length, oldText.length, " END");
    const nextBlock = dollarBlocks(newText)[0];
    expect(localChanges([change], oldBlock, nextBlock)).toEqual([]);
  });

  it("un cambio que cruza el $tag$ de apertura obliga a parsear de cero", () => {
    // Reemplaza el segundo '$' de apertura junto con la primera letra del cuerpo: empieza antes
    // de `oldBlock.from` y termina adentro. `next` no llega a leerse: el cruce corta antes.
    const { newText, change } = edit(oldText, 4, 6, "Q");
    const nextBlock = dollarBlocks(newText)[0];
    expect(localChanges([change], oldBlock, nextBlock)).toBeNull();
  });

  it("un cambio que cruza el $tag$ de cierre obliga a parsear de cero", () => {
    // Reemplaza la última letra del cuerpo junto con el primer '$' de cierre.
    const { newText, change } = edit(oldText, 6, 8, "Q");
    const nextBlock = dollarBlocks(newText)[0];
    expect(localChanges([change], oldBlock, nextBlock)).toBeNull();
  });

  it("escribir un $$ adentro que corta el cuerpo obliga a parsear de cero", () => {
    // La 'a' se reemplaza por '$$': el cambio en sí cae adentro del cuerpo, pero el '$$' nuevo
    // abre y cierra ahí mismo — el cuerpo que de verdad queda no es el viejo estirado, y el
    // chequeo de largo lo nota aunque el cambio, mirado solo, parecía calzar adentro.
    const { newText, change } = edit(oldText, oldBlock.from, oldBlock.from + 1, "$$");
    const nextBlock = dollarBlocks(newText)[0];
    expect(localChanges([change], oldBlock, nextBlock)).toBeNull();
  });

  it("borrar el $$ de cierre deja el cuerpo sin cerrar, y lo nota el chequeo de largo y no una rama de cruce", () => {
    // El cambio (borrar las dos `$`) empieza justo en `old.to`, así que la clasificación por
    // posición lo cuenta como "después" y no como cruce —el cuerpo sin cerrar ahora llega hasta
    // el final del texto, y ese largo no lo explica ningún cambio "adentro" de la lista.
    const { newText, change } = edit(oldText, oldBlock.to, oldBlock.to + 2, "");
    const nextBlock = dollarBlocks(newText)[0];
    expect(localChanges([change], oldBlock, nextBlock)).toBeNull();
  });

  it("borrar contenido adentro del cuerpo también queda en coordenadas locales", () => {
    const { newText, change } = edit(oldText, 5, 6, ""); // saca la 'a'
    const nextBlock = dollarBlocks(newText)[0];
    expect(localChanges([change], oldBlock, nextBlock)).toEqual([
      { fromA: 0, toA: 1, fromB: 0, toB: 0 },
    ]);
  });

  it("varios cambios en la misma transacción: solo el que cae adentro deja rango local", () => {
    // Dos cambios del mismo `docChanged`: uno antes del bloque (solo corre posiciones) y uno
    // adentro. `fromB`/`toB` del segundo ya vienen corridos por el primero, tal como los entrega
    // `ChangeSet.iterChangedRanges`.
    const newText = "QAS $$aXb$$ Z"; // "Q" antes del bloque, "X" adentro (entre 'a' y 'b')
    const nextBlock = dollarBlocks(newText)[0];
    const change1 = { fromA: 0, toA: 0, fromB: 0, toB: 1 }; // inserta "Q" antes del bloque
    const change2 = { fromA: 6, toA: 6, fromB: 7, toB: 8 }; // inserta "X" adentro
    expect(localChanges([change1, change2], oldBlock, nextBlock)).toEqual([
      { fromA: 1, toA: 1, fromB: 1, toB: 2 },
    ]);
  });
});

describe("pairBodies", () => {
  it("empareja un cuerpo nuevo con su viejo por misma etiqueta y posición mapeada", () => {
    const old = { from: 5, to: 7, tag: "" };
    const next = { from: 5, to: 9, tag: "" };
    expect(pairBodies([old], [next], (from) => from)).toEqual([old]);
  });

  it("no empareja cuerpos con etiquetas distintas aunque la posición mapeada coincida", () => {
    const old = { from: 5, to: 7, tag: "func" };
    const next = { from: 5, to: 9, tag: "" };
    expect(pairBodies([old], [next], (from) => from)).toEqual([null]);
  });

  it("sin cuerpos viejos no hay par", () => {
    const next = { from: 5, to: 9, tag: "" };
    expect(pairBodies([], [next], (from) => from)).toEqual([null]);
  });

  it("colisión: si dos cuerpos viejos mapean al mismo punto se prueba uno solo, y si no calza localChanges fuerza parseo de cero", () => {
    // El caso real pediría una edición patológica para que dos cuerpos distintos mapeen al mismo
    // lugar; acá se fuerza la colisión a mano para probar que el desempate no inventa un
    // resultado, solo elige un candidato que el chequeo de largo de `localChanges` puede rechazar.
    const oldA = { from: 5, to: 7, tag: "" }; // cuerpo de largo 2
    const oldB = { from: 100, to: 130, tag: "" }; // cuerpo de largo 30, en otro lugar del documento
    const next = { from: 5, to: 7, tag: "" }; // el cuerpo nuevo, del mismo largo que `oldA`

    const collide = () => 5; // las dos "viejas" mapean al mismo punto que el cuerpo nuevo

    const [paired] = pairBodies([oldA, oldB], [next], collide);
    expect(paired).toBe(oldA); // gana la primera en el orden de `oldBodies`

    // Si en cambio ganara `oldB` —de largo muy distinto—, el chequeo de largo de `localChanges` lo
    // rechaza: no hay cambio que explique que un cuerpo de 30 pase a medir 2.
    expect(localChanges([], oldB, next)).toBeNull();
  });
});

/**
 * Un cuerpo de varios KB y sin repetición real: cada sentencia difiere en el número, así que un
 * árbol con un solo nodo corrido de posición no puede pasar por casualidad como si fuera el mismo
 * que un parseo completo. Con un cuerpo chico, `TreeFragment.applyChanges` tira el fragmento entero
 * igual —el `minGap` por omisión son 128 caracteres— y el reparseo incremental nunca se ejercita de
 * verdad.
 */
function bigBody(n: number): string {
  const parts: string[] = [];
  for (let i = 0; i < n; i++) {
    parts.push(
      `SELECT col_${i}, 'valor ${i}' AS etiqueta_${i} FROM tabla_${i % 37} WHERE id = ${i};`,
    );
  }
  return parts.join("\n");
}

/**
 * Los nodos de un árbol, con posición. `Tree.toString()` no alcanza para esto: solo anida nombres
 * de tipo, sin `from`/`to`, así que dos árboles con los mismos nodos pero corridos de lugar salen
 * iguales.
 */
function nodePositions(tree: Tree): string[] {
  const out: string[] = [];
  tree.iterate({
    enter: (node) => {
      out.push(`${node.name}@${node.from}-${node.to}`);
    },
  });
  return out;
}

describe("parseo incremental", () => {
  const bodyText = bigBody(300);
  const before = `CREATE FUNCTION f() RETURNS void AS $$\n${bodyText}\n$$ LANGUAGE sql;`;
  const oldBlock = dollarBlocks(before)[0];

  // Inserta bien adentro del cuerpo, lejos de los dos bordes: con un cuerpo de este tamaño hay
  // fragmento de sobra a cada lado del cambio como para que `applyChanges` los reuse de verdad.
  const insertAt = before.indexOf("SELECT col_150");
  const tr = EditorState.create({ doc: before }).update({
    changes: { from: insertAt, insert: "-- una línea nueva\n" },
  });
  const after = tr.state.doc.toString();
  const nextBlock = dollarBlocks(after)[0];

  const changes: { fromA: number; toA: number; fromB: number; toB: number }[] = [];
  tr.changes.iterChangedRanges((fromA, toA, fromB, toB) => {
    changes.push({ fromA, toA, fromB, toB });
  });

  const local = localChanges(changes, oldBlock, nextBlock);
  const oldTree = PostgreSQL.language.parser.parse(before.slice(oldBlock.from, oldBlock.to));
  const fullTree = PostgreSQL.language.parser.parse(after.slice(nextBlock.from, nextBlock.to));

  it("encuentra un rango local para reusar", () => {
    expect(local).not.toBeNull();
  });

  it("da el mismo árbol —nodo por nodo, con posición— que parsear de cero", () => {
    const fragments = TreeFragment.applyChanges(TreeFragment.addTree(oldTree), local!);
    const incrementalTree = PostgreSQL.language.parser.parse(
      after.slice(nextBlock.from, nextBlock.to),
      fragments,
    );

    expect(nodePositions(incrementalTree)).toEqual(nodePositions(fullTree));
  });

  it("caso de control: si no se tradujeran las posiciones, la comparación de arriba lo nota", () => {
    // Simula el bug que la comparación de arriba tiene que atrapar: reusar el árbol viejo tal cual,
    // sin pasar por `applyChanges` — como si `localChanges` hubiera dicho que no hacía falta
    // reaplicar nada, a pesar de que sí hubo un cambio adentro. El árbol sale corrido a partir de la
    // inserción, y `nodePositions` lo diferencia del parseo completo.
    const rawFragments = TreeFragment.addTree(oldTree);
    const wrongTree = PostgreSQL.language.parser.parse(
      after.slice(nextBlock.from, nextBlock.to),
      rawFragments,
    );

    expect(nodePositions(wrongTree)).not.toEqual(nodePositions(fullTree));
  });
});
