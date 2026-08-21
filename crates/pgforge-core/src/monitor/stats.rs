//! Estadísticas de uso de tablas e índices.

use std::collections::BTreeMap;

use serde::Serialize;
use tokio_postgres::Client;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableStat {
    pub schema: String,
    pub table: String,
    pub live_tuples: i64,
    pub dead_tuples: i64,
    /// Proporción de tuplas muertas sobre el total.
    ///
    /// Es una **estimación** basada en los contadores de `pg_stat_user_tables`, no una medición
    /// del espacio desperdiciado: los contadores se reinician con las estadísticas y no ven el
    /// espacio libre real de cada página. Para un número exacto hay que usar `pgstattuple`, que
    /// recorre la tabla entera. Sirve para saber a qué tabla mirarle el vacuum, no para decidir un
    /// `VACUUM FULL` a ciegas.
    pub dead_ratio: Option<f64>,
    pub total_bytes: i64,
    pub table_bytes: i64,
    pub index_bytes: i64,
    pub sequential_scans: i64,
    pub index_scans: Option<i64>,
    pub last_vacuum_seconds: Option<f64>,
    pub last_autovacuum_seconds: Option<f64>,
    pub last_analyze_seconds: Option<f64>,
}

pub async fn tables(client: &Client, limit: i64) -> Result<Vec<TableStat>> {
    let rows = client
        .query(
            "SELECT s.schemaname::text,
                    s.relname::text,
                    s.n_live_tup,
                    s.n_dead_tup,
                    CASE WHEN s.n_live_tup + s.n_dead_tup > 0
                         THEN s.n_dead_tup::float8 / (s.n_live_tup + s.n_dead_tup) END,
                    pg_catalog.pg_total_relation_size(s.relid),
                    pg_catalog.pg_table_size(s.relid),
                    pg_catalog.pg_indexes_size(s.relid),
                    s.seq_scan,
                    s.idx_scan,
                    extract(epoch from (now() - s.last_vacuum))::float8,
                    extract(epoch from (now() - s.last_autovacuum))::float8,
                    extract(epoch from (now() - greatest(s.last_analyze, s.last_autoanalyze)))::float8
               FROM pg_catalog.pg_stat_user_tables s
              ORDER BY s.n_dead_tup DESC, pg_catalog.pg_total_relation_size(s.relid) DESC
              LIMIT $1",
            &[&limit],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| TableStat {
            schema: row.get(0),
            table: row.get(1),
            live_tuples: row.get(2),
            dead_tuples: row.get(3),
            dead_ratio: row.get(4),
            total_bytes: row.get(5),
            table_bytes: row.get(6),
            index_bytes: row.get(7),
            sequential_scans: row.get(8),
            index_scans: row.get(9),
            last_vacuum_seconds: row.get(10),
            last_autovacuum_seconds: row.get(11),
            last_analyze_seconds: row.get(12),
        })
        .collect())
}

/// Lo que hace que un índice no sea de quien mira la lista, aunque nadie lo consulte y aunque otro
/// responda las mismas preguntas.
///
/// Está escrito una sola vez porque las dos listas de índices que sobran —los que nunca se usaron y
/// los que otro ya cubre— tienen que frenar ante lo mismo. Cuando cada una tenía su propia idea de
/// qué está protegido, una proponía borrar justo lo que la otra conservaba.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Guards {
    /// El nombre de la restricción que sostiene, si sostiene una (`PRIMARY KEY`, `UNIQUE` o
    /// `EXCLUDE`). Un `EXCLUDE` no es único ni primario, así que sin esto pasaba las guardas.
    pub constraint: Option<String>,
    /// `true` si una clave foránea apunta a él.
    pub referenced_by_fk: bool,
    /// `true` si es la identidad de réplica de su tabla.
    pub replica_identity: bool,
    /// `true` si es el índice con el que se hizo el último `CLUSTER`.
    pub clustered: bool,
    /// `true` si lo instaló una extensión.
    pub from_extension: bool,
}

impl Guards {
    /// Si se le puede proponer el borrado al usuario. Lo que sostiene algo —una restricción, una
    /// clave foránea, la identidad de réplica, el orden de un `CLUSTER`— no sobra aunque nadie lo
    /// consulte, y lo que instaló una extensión no es de quien mira la lista.
    ///
    /// Las primeras las rechaza el servidor con un error claro; la identidad de réplica **no**: el
    /// `DROP INDEX` funciona y la tabla queda con `relreplident = 'i'` apuntando a nada, así que la
    /// replicación lógica se rompe recién en el próximo `UPDATE`, lejos del botón que lo causó.
    pub fn droppable(&self) -> bool {
        self.constraint.is_none()
            && !self.referenced_by_fk
            && !self.replica_identity
            && !self.clustered
            && !self.from_extension
    }
}

/// Las cinco columnas que llenan un [`Guards`], para interpolar en las consultas que lo necesitan.
/// Esperan que el índice se llame `i` y sean el mismo texto en las dos, o las listas volverían a
/// discrepar por una diferencia de SQL en vez de una de criterio.
const GUARD_COLUMNS: &str = "(SELECT co.conname::text
                                   FROM pg_catalog.pg_constraint co
                                  WHERE co.conindid = i.indexrelid
                                    AND co.contype IN ('p', 'u', 'x')
                                  LIMIT 1),
                                EXISTS (SELECT 1
                                          FROM pg_catalog.pg_constraint fk
                                         WHERE fk.conindid = i.indexrelid
                                           AND fk.contype = 'f'),
                                i.indisreplident,
                                i.indisclustered,
                                EXISTS (SELECT 1
                                          FROM pg_catalog.pg_depend d
                                         WHERE d.classid = 'pg_catalog.pg_class'::regclass
                                           AND d.objid = i.indexrelid
                                           AND d.deptype = 'e')";

/// Lee las cinco columnas de [`GUARD_COLUMNS`] a partir de la posición `first`.
fn guards_at(row: &tokio_postgres::Row, first: usize) -> Guards {
    Guards {
        constraint: row.get(first),
        referenced_by_fk: row.get(first + 1),
        replica_identity: row.get(first + 2),
        clustered: row.get(first + 3),
        from_extension: row.get(first + 4),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStat {
    pub schema: String,
    pub table: String,
    pub index: String,
    pub scans: i64,
    pub bytes: i64,
    pub is_unique: bool,
    pub is_primary: bool,
    pub is_valid: bool,
    /// Lo que sostiene, si sostiene algo. No cruza el IPC: la interfaz muestra `unused`, y qué lo
    /// protege es el porqué de esa decisión, no otra columna de la grilla.
    #[serde(skip)]
    pub guards: Guards,
    /// El resultado de [`IndexStat::is_unused`], calculado al leer.
    ///
    /// Viaja como dato en vez de quedarse como método porque la interfaz tenía la regla copiada a
    /// mano en dos lugares y sin las guardas: un índice de `EXCLUDE` que nadie consulta salía
    /// marcado «nunca se usó».
    pub unused: bool,
}

impl IndexStat {
    /// Un índice que nunca se usó y que no sostiene nada es espacio y trabajo de escritura a cambio
    /// de nada. Los únicos y los de clave primaria quedan afuera aunque no haya una restricción
    /// detrás —un `CREATE UNIQUE INDEX` suelto no deja fila en `pg_constraint`— porque su razón de
    /// ser no es acelerar consultas.
    pub fn is_unused(&self) -> bool {
        self.scans == 0 && !self.is_unique && !self.is_primary && self.guards.droppable()
    }
}

pub async fn indexes(client: &Client, limit: i64) -> Result<Vec<IndexStat>> {
    let rows = client
        .query(
            &format!(
                "SELECT s.schemaname::text,
                        s.relname::text,
                        s.indexrelname::text,
                        s.idx_scan,
                        pg_catalog.pg_relation_size(s.indexrelid),
                        i.indisunique,
                        i.indisprimary,
                        i.indisvalid,
                        {GUARD_COLUMNS}
                   FROM pg_catalog.pg_stat_user_indexes s
                   JOIN pg_catalog.pg_index i ON i.indexrelid = s.indexrelid
                  ORDER BY s.idx_scan ASC, pg_catalog.pg_relation_size(s.indexrelid) DESC
                  LIMIT $1"
            ),
            &[&limit],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let stat = IndexStat {
                schema: row.get(0),
                table: row.get(1),
                index: row.get(2),
                scans: row.get::<_, Option<i64>>(3).unwrap_or(0),
                bytes: row.get(4),
                is_unique: row.get(5),
                is_primary: row.get(6),
                is_valid: row.get(7),
                guards: guards_at(&row, 8),
                unused: false,
            };
            IndexStat {
                unused: stat.is_unused(),
                ..stat
            }
        })
        .collect())
}

/// La forma de un índice, que es lo que hace falta para saber si sobra.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexShape {
    pub schema: String,
    pub table: String,
    pub name: String,
    /// Las columnas o expresiones de la clave, en orden. Sin las de un `INCLUDE`: esas acompañan al
    /// índice pero no se puede buscar por ellas, así que no cuentan para decidir si uno cubre a otro.
    pub columns: Vec<String>,
    /// Las columnas de un `INCLUDE`, por nombre. No se puede buscar por ellas, pero sí leerlas sin
    /// ir a la tabla, así que un índice solo cubre a otro si arrastra todas las que el otro arrastra.
    pub included: Vec<String>,
    pub method: String,
    /// La clase de operadores de cada columna de la clave. Dos índices con las mismas columnas y
    /// distinta clase no responden lo mismo: `text_pattern_ops` es lo único que resuelve un
    /// `LIKE 'x%'` fuera de la colación C.
    pub opclasses: Vec<u32>,
    /// La colación de cada columna de la clave: otra colación es otro orden.
    pub collations: Vec<u32>,
    /// El `indoption` de cada columna de la clave, con el bit 1 en `DESC` y el bit 2 en
    /// `NULLS FIRST`. Ver [`same_order`] para por qué no alcanza con ignorarlo.
    pub options: Vec<i16>,
    pub unique: bool,
    pub primary: bool,
    /// Lo que sostiene, si sostiene algo. Ver [`Guards`]. No cruza el IPC: de la forma de un índice
    /// la interfaz solo ve lo que termina en un [`Redundancy`].
    #[serde(skip)]
    pub guards: Guards,
    /// `true` si es un índice particionado (`relkind = 'I'`), que no se puede borrar con
    /// `CONCURRENTLY` y cuyo tamaño es el de sus partes.
    pub partitioned: bool,
    /// El predicado, si es parcial. Dos índices con predicados distintos no se cubren entre sí.
    pub predicate: Option<String>,
    pub bytes: i64,
    pub scans: i64,
}

impl IndexShape {
    /// `true` si arrastra columnas de un `INCLUDE`.
    pub fn covering(&self) -> bool {
        !self.included.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RedundancyKind {
    /// Las mismas columnas, en el mismo orden: uno de los dos es una copia del otro.
    Duplicate,
    /// Sus columnas son el principio de las del otro, así que el otro sirve para lo mismo.
    Prefix,
}

/// Un índice que otro ya cubre.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Redundancy {
    pub schema: String,
    pub table: String,
    /// El que sobra.
    pub index: String,
    /// El que ya hace su trabajo.
    pub covered_by: String,
    pub kind: RedundancyKind,
    pub bytes: i64,
    pub scans: i64,
    /// La sentencia exacta que lo borraría, para que se vea antes de decidir.
    pub drop_sql: String,
}

/// Todos los índices válidos de las tablas del usuario, con su forma.
///
/// **Los índices de una partición no entran**: cuelgan del de su madre, así que el mismo par
/// duplicado aparecería una vez por partición, y borrarlos por separado lo rechaza el servidor
/// («cannot drop index … because index … requires it»). La madre sí entra, con el tamaño de todo su
/// árbol, porque `pg_relation_size` de un índice particionado da 0.
pub async fn index_shapes(client: &Client) -> Result<Vec<IndexShape>> {
    let rows = client
        .query(
            &format!(
                "SELECT n.nspname::text,
                    t.relname::text,
                    ix.relname::text,
                    am.amname::text,
                    ix.relkind = 'I',
                    i.indisunique,
                    i.indisprimary,
                    {GUARD_COLUMNS},
                    pg_catalog.pg_get_expr(i.indpred, i.indrelid),
                    (CASE WHEN ix.relkind = 'I'
                          THEN COALESCE((SELECT sum(pg_catalog.pg_relation_size(p.relid))
                                           FROM pg_catalog.pg_partition_tree(i.indexrelid) p), 0)
                          ELSE pg_catalog.pg_relation_size(i.indexrelid)
                     END)::bigint,
                    COALESCE(s.idx_scan, 0),
                    (SELECT array_agg(pg_catalog.pg_get_indexdef(i.indexrelid, k.ord::int, true)
                                      ORDER BY k.ord)
                       FROM generate_series(1, i.indnkeyatts) AS k(ord)),
                    (SELECT array_agg(pg_catalog.pg_get_indexdef(i.indexrelid, k.ord::int, true)
                                      ORDER BY k.ord)
                       FROM generate_series(i.indnkeyatts + 1, i.indnatts) AS k(ord)),
                    i.indclass::oid[],
                    i.indcollation::oid[],
                    i.indoption::smallint[]
               FROM pg_catalog.pg_index i
               JOIN pg_catalog.pg_class ix ON ix.oid = i.indexrelid
               JOIN pg_catalog.pg_class t ON t.oid = i.indrelid
               JOIN pg_catalog.pg_namespace n ON n.oid = t.relnamespace
               JOIN pg_catalog.pg_am am ON am.oid = ix.relam
               LEFT JOIN pg_catalog.pg_stat_user_indexes s ON s.indexrelid = i.indexrelid
              WHERE i.indisvalid
                AND t.relkind IN ('r', 'm', 'p')
                AND n.nspname NOT IN ('pg_catalog', 'information_schema')
                AND NOT EXISTS (SELECT 1
                                  FROM pg_catalog.pg_inherits h
                                 WHERE h.inhrelid = i.indexrelid)
              ORDER BY n.nspname, t.relname, ix.relname"
            ),
            &[],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| IndexShape {
            schema: row.get(0),
            table: row.get(1),
            name: row.get(2),
            method: row.get(3),
            partitioned: row.get(4),
            unique: row.get(5),
            primary: row.get(6),
            guards: guards_at(&row, 7),
            predicate: row.get(12),
            bytes: row.get(13),
            scans: row.get::<_, Option<i64>>(14).unwrap_or(0),
            // `pg_get_indexdef` por columna devuelve la expresión ya escrita: sirve igual para una
            // columna suelta que para un índice por expresión. Lo que **no** devuelve es la clase
            // de operadores, la colación ni el orden, que por eso se leen aparte de `pg_index`.
            columns: row.get::<_, Option<Vec<String>>>(15).unwrap_or_default(),
            included: row.get::<_, Option<Vec<String>>>(16).unwrap_or_default(),
            opclasses: row.get::<_, Option<Vec<u32>>>(17).unwrap_or_default(),
            collations: row.get::<_, Option<Vec<u32>>>(18).unwrap_or_default(),
            options: row.get::<_, Option<Vec<i16>>>(19).unwrap_or_default(),
        })
        .collect())
}

/// Los bits de `indoption` que hablan del orden: 1 es `DESC` y 2 es `NULLS FIRST`.
const ORDER_BITS: i16 = 3;

/// Lo que hace que dos índices puedan llegar a cubrirse: mismo esquema, misma tabla, mismo método y
/// mismo predicado. Con distinto valor en cualquiera de los cuatro, el par ni se compara.
type GroupKey<'a> = (&'a str, &'a str, &'a str, Option<&'a str>);

/// Los índices que otro ya cubre, del más grande al más chico.
///
/// Es una función pura sobre lo que se leyó del catálogo: la regla es la parte que se puede
/// equivocar —y equivocarse acá significa proponer borrar un índice que hace falta—, así que se
/// prueba sin servidor.
///
/// Lo que **nunca** se propone borrar: lo que no pasa [`Guards::droppable`], un índice único
/// porque otro más largo lo empiece (sostiene la unicidad, no una consulta), uno que arrastra en su
/// `INCLUDE` una columna que el otro no tiene (se perdería el recorrido solo por índice) y cualquier
/// par que no coincida en método, predicado, clase de operadores, colación y orden.
pub fn redundancies(indexes: &[IndexShape]) -> Vec<Redundancy> {
    // Agrupar primero es lo que evita comparar cada índice de la base contra todos los demás: el par
    // solo puede estar adentro de esta clave, que antes se verificaba de a un par por vez.
    let mut groups: BTreeMap<GroupKey<'_>, Vec<&IndexShape>> = BTreeMap::new();
    for index in indexes.iter().filter(|index| !index.columns.is_empty()) {
        groups
            .entry((
                index.schema.as_str(),
                index.table.as_str(),
                index.method.as_str(),
                index.predicate.as_deref(),
            ))
            .or_default()
            .push(index);
    }

    let mut out: Vec<Redundancy> = Vec::new();
    for group in groups.values() {
        for (i, candidate) in group.iter().enumerate() {
            if !candidate.guards.droppable() {
                continue;
            }

            for (j, other) in group.iter().enumerate() {
                if i == j {
                    continue;
                }

                let duplicate = candidate.columns == other.columns;
                let prefix = !duplicate
                    && candidate.method == "btree"
                    && other.columns.starts_with(&candidate.columns);
                if !duplicate && !prefix {
                    continue;
                }

                // Las columnas se leen ya escritas, así que dos índices con la misma columna y
                // distinta clase de operadores, distinta colación o distinto orden llegan hasta acá
                // con las mismas `columns`. Se comparan solo las posiciones que el candidato usa.
                let width = candidate.columns.len();
                if !same_semantics(candidate, other, width) || !same_order(candidate, other, width)
                {
                    continue;
                }

                // Con las mismas columnas hay que elegir cuál se queda, y decidirlo dos veces —una
                // por cada orden del par— marcaría los dos. Gana el que sostiene una restricción,
                // después el que cubre, después el más usado, y al final el nombre, que siempre
                // desempata.
                if duplicate && keeps(candidate, other) {
                    continue;
                }
                if (candidate.unique || candidate.primary) && !duplicate {
                    continue;
                }
                if !candidate
                    .included
                    .iter()
                    .all(|column| other.included.contains(column))
                {
                    continue;
                }

                out.push(Redundancy {
                    schema: candidate.schema.clone(),
                    table: candidate.table.clone(),
                    index: candidate.name.clone(),
                    covered_by: other.name.clone(),
                    kind: if duplicate {
                        RedundancyKind::Duplicate
                    } else {
                        RedundancyKind::Prefix
                    },
                    bytes: candidate.bytes,
                    scans: candidate.scans,
                    // Un índice particionado no se puede borrar con `CONCURRENTLY`: el servidor
                    // contesta «cannot drop partitioned index … concurrently».
                    drop_sql: crate::ddl::index::drop_sql(
                        &candidate.schema,
                        &candidate.name,
                        false,
                        !candidate.partitioned,
                    )
                    .map(|statement| statement.sql)
                    .unwrap_or_default(),
                });
                break;
            }
        }
    }

    // Del que más ocupa al que menos: es el orden en que uno decide qué borrar primero. El nombre
    // desempata para que dos índices del mismo tamaño no salgan en un orden distinto cada vez.
    out.sort_by(|a, b| {
        b.bytes
            .cmp(&a.bytes)
            .then_with(|| a.schema.cmp(&b.schema))
            .then_with(|| a.index.cmp(&b.index))
    });
    out
}

/// Si las primeras `width` posiciones de los dos índices usan la misma clase de operadores y la
/// misma colación. Un `text_pattern_ops` y el `text_ops` de siempre se leen igual del catálogo y
/// responden preguntas distintas, así que sin esto se propondría borrar el que resuelve los `LIKE`.
fn same_semantics(a: &IndexShape, b: &IndexShape, width: usize) -> bool {
    let enough = |values: usize| values >= width;
    if !enough(a.opclasses.len())
        || !enough(b.opclasses.len())
        || !enough(a.collations.len())
        || !enough(b.collations.len())
    {
        // Un catálogo que no trajo estos vectores no alcanza para decidir: mejor no proponer nada.
        return false;
    }
    a.opclasses[..width] == b.opclasses[..width] && a.collations[..width] == b.collations[..width]
}

/// Si el orden de las primeras `width` posiciones es el mismo o el exactamente inverso.
///
/// El inverso también sirve porque un btree se recorre para atrás: `(a DESC)` responde igual que
/// `(a)`. Lo que **no** sirve es invertir una parte, y ahí está la diferencia que importa:
/// `(a NULLS FIRST)` no es ni igual ni el inverso de `(a)` —el inverso de `(a)` es
/// `(a DESC NULLS FIRST)`—, así que un `ORDER BY a NULLS FIRST` solo lo resuelve el primero.
fn same_order(a: &IndexShape, b: &IndexShape, width: usize) -> bool {
    if a.options.len() < width || b.options.len() < width {
        return false;
    }
    let bits = |values: &[i16], k: usize| values[k] & ORDER_BITS;
    let forward = (0..width).all(|k| bits(&a.options, k) == bits(&b.options, k));
    let backward = (0..width).all(|k| bits(&a.options, k) == bits(&b.options, k) ^ ORDER_BITS);
    forward || backward
}

/// Con columnas idénticas, cuál de los dos se conserva.
///
/// Lo que no se puede borrar va primero, y no solo porque gane: si el desempate por nombre se lo
/// diera al otro, el duplicado de verdad se quedaría sin proponer y la copia inútil viviría para
/// siempre al lado del índice de la restricción.
fn keeps(candidate: &IndexShape, other: &IndexShape) -> bool {
    let rank = |index: &IndexShape| {
        (
            !index.guards.droppable(),
            index.primary,
            index.unique,
            index.covering(),
            index.scans,
            std::cmp::Reverse(index.name.clone()),
        )
    };
    rank(candidate) > rank(other)
}

/// Bloat estimado de una tabla: espacio que ocupa de más por tuplas muertas y huecos que el vacuum
/// no devolvió al disco.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableBloat {
    pub schema: String,
    pub table: String,
    pub total_bytes: i64,
    /// Espacio libre estimado dentro de la tabla, en bytes: reutilizable por filas nuevas, pero no
    /// devuelto al sistema operativo hasta un `VACUUM FULL`.
    pub free_bytes: i64,
    /// Fracción del espacio que está libre (0 a 1), no el porcentaje: se normaliza acá para que la
    /// interfaz la trate igual que el `dead_ratio` de `TableStat`.
    pub free_ratio: f64,
    /// Fracción ocupada por tuplas muertas que el vacuum todavía no limpió (0 a 1).
    pub dead_ratio: f64,
}

/// `true` si la extensión `pgstattuple` está instalada en esta base.
pub async fn has_pgstattuple(client: &Client) -> Result<bool> {
    let row = client
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_catalog.pg_extension WHERE extname = 'pgstattuple')",
            &[],
        )
        .await?;
    Ok(row.get(0))
}

/// Bloat de las tablas más grandes, medido con `pgstattuple_approx`.
///
/// A diferencia de la proporción de tuplas muertas de [`tables`] —que se deriva de contadores que se
/// reinician con las estadísticas—, esto es una medición real del espacio: `pgstattuple_approx` mira
/// el mapa de visibilidad y el de espacio libre en vez de recorrer la tabla entera, así que da un
/// número honesto sin el costo del `pgstattuple` exacto. Se limita a las tablas más grandes porque
/// son las únicas donde el bloat pesa y para no llamar a la función sobre miles de relaciones.
pub async fn bloat(client: &Client, limit: i64) -> Result<Vec<TableBloat>> {
    let rows = client
        .query(
            "WITH grandes AS (
                 SELECT c.oid, n.nspname, c.relname
                   FROM pg_catalog.pg_class c
                   JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
                  WHERE c.relkind IN ('r', 'm')
                    AND n.nspname NOT IN ('pg_catalog', 'information_schema')
                    AND n.nspname NOT LIKE 'pg_temp%'
                  ORDER BY pg_catalog.pg_total_relation_size(c.oid) DESC
                  LIMIT $1
             )
             SELECT g.nspname::text,
                    g.relname::text,
                    pg_catalog.pg_total_relation_size(g.oid),
                    a.approx_free_space::int8,
                    a.approx_free_percent / 100.0,
                    a.dead_tuple_percent / 100.0
               FROM grandes g
               CROSS JOIN LATERAL pgstattuple_approx(g.oid) a
              ORDER BY a.approx_free_space DESC",
            &[&limit],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| TableBloat {
            schema: row.get(0),
            table: row.get(1),
            total_bytes: row.get(2),
            free_bytes: row.get(3),
            free_ratio: row.get(4),
            dead_ratio: row.get(5),
        })
        .collect())
}

/// Consultas más costosas, si la extensión `pg_stat_statements` está instalada.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementStat {
    /// `pg_stat_statements` identifica cada fila por (usuario, base, queryid): el mismo texto
    /// normalizado aparece repetido para cada combinación, y sin estos campos las filas son
    /// indistinguibles en pantalla.
    pub query_id: Option<i64>,
    pub database: Option<String>,
    pub user: Option<String>,
    /// `None` cuando el archivo de textos de la extensión ya no tiene el de esta fila. La entrada
    /// con sus tiempos sigue valiendo, así que se muestra igual y sin el texto.
    pub query: Option<String>,
    pub calls: i64,
    pub total_ms: f64,
    pub mean_ms: f64,
    pub rows: i64,
}

/// Qué versión de `pg_stat_statements` está instalada en esta base, si está.
///
/// Interesa la versión de la **extensión**, no la del servidor: la vista la define el archivo de la
/// extensión, y una base creada hace años —o migrada con `pg_upgrade`— puede seguir con la 1.7
/// aunque el servidor sea la 14. Ahí las columnas todavía se llaman `total_time` y `mean_time`.
pub async fn statement_stats_version(client: &Client) -> Result<Option<String>> {
    let row = client
        .query_opt(
            "SELECT extversion FROM pg_catalog.pg_extension WHERE extname = 'pg_stat_statements'",
            &[],
        )
        .await?;
    Ok(row.map(|row| row.get(0)))
}

/// `true` si `pg_stat_statements` está instalada en esta base.
pub async fn has_statement_stats(client: &Client) -> Result<bool> {
    Ok(statement_stats_version(client).await?.is_some())
}

/// `true` si esa versión de la extensión ya usa `total_exec_time` en vez de `total_time`.
///
/// El corte es la 1.8, la que vino con PostgreSQL 13: ahí se separó el tiempo de planificación del
/// de ejecución y las dos columnas se renombraron. Una versión que no se puede interpretar se
/// asume nueva, que es lo que trae cualquier servidor del rango soportado sin que nadie la fije.
fn uses_exec_time(extversion: &str) -> bool {
    let mut parts = extversion.split('.');
    let major: u32 = match parts.next().and_then(|part| part.parse().ok()) {
        Some(major) => major,
        None => return true,
    };
    // `unwrap_or(0)`: una versión sin minor (`"2"`) es posterior a cualquier `1.x`.
    let minor: u32 = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);

    (major, minor) >= (1, 8)
}

pub async fn statements(client: &Client, limit: i64) -> Result<Vec<StatementStat>> {
    let extversion = statement_stats_version(client).await?.ok_or_else(|| {
        Error::Config("la extensión pg_stat_statements no está instalada en esta base".to_owned())
    })?;

    // Los nombres salen de una lista cerrada de dos, no de nada que escriba el usuario.
    let (total, mean) = if uses_exec_time(&extversion) {
        ("total_exec_time", "mean_exec_time")
    } else {
        ("total_time", "mean_time")
    };

    let rows = client
        .query(
            &format!(
                "SELECT s.queryid,
                        d.datname::text,
                        r.rolname::text,
                        s.query,
                        s.calls,
                        s.{total},
                        s.{mean},
                        s.rows
                   FROM pg_stat_statements s
                   LEFT JOIN pg_catalog.pg_database d ON d.oid = s.dbid
                   LEFT JOIN pg_catalog.pg_roles r ON r.oid = s.userid
                  ORDER BY s.{total} DESC
                  LIMIT $1"
            ),
            &[&limit],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| StatementStat {
            query_id: row.get(0),
            database: row.get(1),
            user: row.get(2),
            query: row.get(3),
            calls: row.get(4),
            total_ms: row.get(5),
            mean_ms: row.get(6),
            rows: row.get(7),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Una guarda: cómo se marca y con qué motivo, para el mensaje del fallo.
    type Guarda = (&'static str, fn(&mut Guards));

    /// Las cinco, porque las dos listas de índices que sobran tienen que frenar ante las mismas.
    const GUARDAS: [Guarda; 5] = [
        ("restricción", |g| g.constraint = Some("t_per_excl".into())),
        ("clave foránea", |g| g.referenced_by_fk = true),
        ("identidad de réplica", |g| g.replica_identity = true),
        ("cluster", |g| g.clustered = true),
        ("extensión", |g| g.from_extension = true),
    ];

    fn index(scans: i64, unique: bool, primary: bool) -> IndexStat {
        IndexStat {
            schema: "public".into(),
            table: "t".into(),
            index: "i".into(),
            scans,
            bytes: 8192,
            is_unique: unique,
            is_primary: primary,
            is_valid: true,
            guards: Guards::default(),
            unused: false,
        }
    }

    #[test]
    fn solo_marca_sin_uso_los_indices_que_se_pueden_borrar() {
        assert!(index(0, false, false).is_unused());
        assert!(!index(1, false, false).is_unused());
        // Estos sostienen la unicidad: que nadie los consulte no los hace prescindibles.
        assert!(!index(0, true, false).is_unused());
        assert!(!index(0, false, true).is_unused());
    }

    #[test]
    fn sin_uso_frena_ante_las_mismas_guardas_que_los_duplicados() {
        // Ninguno de estos es único ni primario, así que sin mirar `Guards` los cinco salían
        // marcados «nunca se usó» —y el de la restricción y el de la clave foránea ni siquiera se
        // pueden borrar—.
        for (motivo, marcar) in GUARDAS {
            let mut protegido = index(0, false, false);
            marcar(&mut protegido.guards);
            assert!(
                !protegido.is_unused(),
                "sostiene algo ({motivo}): no se marca sin uso"
            );
        }
    }

    #[test]
    fn el_campo_unused_es_lo_que_devuelve_is_unused() {
        // La interfaz lee el campo y no vuelve a escribir la regla, así que los dos tienen que
        // decir lo mismo: `indexes` lo calcula al leer y esto verifica de qué sale.
        let mut sin_uso = index(0, false, false);
        sin_uso.unused = sin_uso.is_unused();
        assert!(sin_uso.unused);
    }

    /// Un índice corriente: sin restricción detrás, con la clase de operadores y la colación por
    /// omisión y en orden ascendente. Los casos que necesitan otra cosa la escriben encima.
    fn shape(name: &str, columns: &[&str]) -> IndexShape {
        IndexShape {
            schema: "public".into(),
            table: "trabajos".into(),
            name: name.into(),
            columns: columns.iter().map(|c| (*c).to_string()).collect(),
            included: Vec::new(),
            method: "btree".into(),
            opclasses: vec![0; columns.len()],
            collations: vec![0; columns.len()],
            options: vec![0; columns.len()],
            unique: false,
            primary: false,
            guards: Guards::default(),
            partitioned: false,
            predicate: None,
            bytes: 1024,
            scans: 0,
        }
    }

    #[test]
    fn dos_indices_con_las_mismas_columnas_dejan_uno_solo() {
        let indexes = vec![shape("idx_a", &["estado"]), shape("idx_b", &["estado"])];
        let sobran = redundancies(&indexes);

        assert_eq!(sobran.len(), 1, "solo uno de los dos sobra: {sobran:?}");
        assert_eq!(sobran[0].kind, RedundancyKind::Duplicate);
        assert!(sobran[0].drop_sql.contains("DROP INDEX"));
    }

    #[test]
    fn el_que_sostiene_una_restriccion_es_el_que_se_queda() {
        let mut unico = shape("trabajos_estado_key", &["estado"]);
        unico.unique = true;
        let indexes = vec![shape("idx_estado", &["estado"]), unico];

        let sobran = redundancies(&indexes);
        assert_eq!(sobran.len(), 1);
        assert_eq!(sobran[0].index, "idx_estado");
        assert_eq!(sobran[0].covered_by, "trabajos_estado_key");
    }

    #[test]
    fn un_indice_es_prescindible_si_otro_lo_empieza() {
        let indexes = vec![
            shape("idx_estado", &["estado"]),
            shape("idx_estado_fecha", &["estado", "fecha"]),
        ];

        let sobran = redundancies(&indexes);
        assert_eq!(sobran.len(), 1);
        assert_eq!(sobran[0].index, "idx_estado");
        assert_eq!(sobran[0].kind, RedundancyKind::Prefix);
    }

    #[test]
    fn el_orden_de_las_columnas_importa() {
        // `(fecha, estado)` no sirve para buscar solo por `estado`: no es un prefijo.
        let indexes = vec![
            shape("idx_estado", &["estado"]),
            shape("idx_fecha_estado", &["fecha", "estado"]),
        ];

        assert!(redundancies(&indexes).is_empty());
    }

    #[test]
    fn un_indice_unico_no_se_borra_porque_otro_lo_empiece() {
        let mut unico = shape("trabajos_codigo_key", &["codigo"]);
        unico.unique = true;
        let indexes = vec![unico, shape("idx_codigo_fecha", &["codigo", "fecha"])];

        assert!(
            redundancies(&indexes).is_empty(),
            "borrarlo sacaría la restricción de unicidad"
        );
    }

    #[test]
    fn no_se_comparan_indices_de_distinto_tipo_ni_con_distinto_predicado() {
        let mut gin = shape("idx_gin", &["datos"]);
        gin.method = "gin".into();
        assert!(redundancies(&[shape("idx_btree", &["datos"]), gin]).is_empty());

        let mut parcial = shape("idx_parcial", &["estado"]);
        parcial.predicate = Some("(activo)".into());
        assert!(redundancies(&[shape("idx_todo", &["estado"]), parcial]).is_empty());
    }

    #[test]
    fn el_que_cubre_con_include_no_se_cambia_por_uno_que_no_cubre() {
        let mut cubre = shape("idx_cubre", &["estado"]);
        cubre.included = vec!["total".into()];
        let sobran = redundancies(&[cubre, shape("idx_pelado", &["estado"])]);

        assert_eq!(sobran.len(), 1);
        assert_eq!(sobran[0].index, "idx_pelado");
    }

    #[test]
    fn el_include_se_compara_por_columna_y_no_por_si_hay_alguna() {
        // Los dos cubren, pero cada uno arrastra otra columna: borrar el corto pierde el recorrido
        // solo por índice sobre `total`.
        let mut corto = shape("idx_corto", &["estado"]);
        corto.included = vec!["total".into()];
        let mut largo = shape("idx_largo", &["estado", "fecha"]);
        largo.included = vec!["cliente".into()];
        assert!(redundancies(&[corto.clone(), largo.clone()]).is_empty());

        // Con la columna del corto adentro del largo, sí sobra.
        largo.included = vec!["total".into(), "cliente".into()];
        let sobran = redundancies(&[corto, largo]);
        assert_eq!(sobran.len(), 1);
        assert_eq!(sobran[0].index, "idx_corto");
    }

    #[test]
    fn una_clase_de_operadores_distinta_responde_otra_pregunta() {
        // Las dos columnas se leen igual del catálogo; lo que las distingue es la clase.
        let mut patron = shape("idx_patron", &["codigo"]);
        patron.opclasses = vec![4217];
        assert!(
            redundancies(&[shape("idx_codigo", &["codigo"]), patron]).is_empty(),
            "text_pattern_ops es lo único que resuelve un LIKE fuera de la colación C"
        );

        let mut colacion = shape("idx_colacion", &["codigo"]);
        colacion.collations = vec![950];
        assert!(
            redundancies(&[shape("idx_codigo", &["codigo"]), colacion]).is_empty(),
            "otra colación es otro orden"
        );
    }

    #[test]
    fn el_orden_invertido_entero_sobra_pero_el_de_los_nulos_no() {
        // `(a DESC)` es indoption 3: DESC y NULLS FIRST, que es el inverso exacto del ascendente.
        // Un btree se recorre para atrás, así que responde lo mismo.
        let mut desc = shape("idx_desc", &["fecha"]);
        desc.options = vec![3];
        let sobran = redundancies(&[shape("idx_asc", &["fecha"]), desc]);
        assert_eq!(sobran.len(), 1, "uno de los dos sobra: {sobran:?}");

        // `(a NULLS FIRST)` es indoption 2: ni igual ni el inverso. Un ORDER BY a NULLS FIRST no lo
        // resuelve el ascendente de siempre.
        let mut nulos = shape("idx_nulos", &["fecha"]);
        nulos.options = vec![2];
        assert!(
            redundancies(&[shape("idx_asc", &["fecha"]), nulos]).is_empty(),
            "invertir solo los nulos no es invertir el orden"
        );
    }

    #[test]
    fn el_orden_se_compara_columna_por_columna() {
        // `(a, b DESC)` no sirve para un ORDER BY a, b: invertir una parte no es invertir nada.
        let mut mixto = shape("idx_mixto", &["estado", "fecha"]);
        mixto.options = vec![0, 3];
        assert!(redundancies(&[shape("idx_par", &["estado", "fecha"]), mixto]).is_empty());
    }

    #[test]
    fn lo_que_sostiene_algo_no_se_propone_aunque_este_duplicado() {
        // Cada uno de estos empata en todo con el otro índice y perdería el desempate por nombre.
        for (motivo, marcar) in GUARDAS {
            let mut protegido = shape("idx_zeta", &["estado"]);
            marcar(&mut protegido.guards);
            let sobran = redundancies(&[shape("idx_alfa", &["estado"]), protegido]);

            assert_eq!(sobran.len(), 1, "por {motivo} tenía que sobrar el otro");
            assert_eq!(
                sobran[0].index, "idx_alfa",
                "el que sostiene algo ({motivo}) no se propone"
            );
        }
    }

    #[test]
    fn un_indice_particionado_no_se_borra_con_concurrently() {
        let mut madre = shape("idx_madre", &["estado"]);
        madre.partitioned = true;
        madre.name = "idx_zeta".into();
        let sobran = redundancies(&[shape("idx_alfa", &["estado"]), madre]);

        assert_eq!(sobran.len(), 1);
        assert_eq!(sobran[0].index, "idx_zeta");
        assert!(
            !sobran[0].drop_sql.contains("CONCURRENTLY"),
            "el servidor rechaza «cannot drop partitioned index … concurrently»: {}",
            sobran[0].drop_sql
        );
    }

    #[test]
    fn los_indices_de_tablas_distintas_no_se_estorban() {
        let mut otra = shape("idx_estado", &["estado"]);
        otra.table = "clientes".into();

        assert!(redundancies(&[shape("idx_estado", &["estado"]), otra]).is_empty());
    }

    #[test]
    fn las_columnas_de_pg_stat_statements_salen_de_la_version_de_la_extension() {
        // El corte está en la 1.8, la de PostgreSQL 13.
        assert!(!uses_exec_time("1.7"), "la 1.7 todavía tiene total_time");
        assert!(!uses_exec_time("1.6"));
        assert!(uses_exec_time("1.8"));
        assert!(uses_exec_time("1.9"));
        // Como texto, "1.10" sería anterior a "1.9": se comparan como números.
        assert!(uses_exec_time("1.10"));
        assert!(uses_exec_time("1.11"));
        assert!(uses_exec_time("2.0"));
        // Sin minor y sin poder interpretarla se asume nueva, que es lo que trae cualquier
        // servidor del rango soportado.
        assert!(uses_exec_time("2"));
        assert!(uses_exec_time("vaya a saber"));
    }
}
