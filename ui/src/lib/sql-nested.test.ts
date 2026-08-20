import { describe, expect, it } from "vitest";
import { dollarBlocks } from "./sql-nested";

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
