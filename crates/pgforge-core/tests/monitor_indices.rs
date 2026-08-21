//! Índices que sobran, contra servidores reales.
//!
//! La regla que decide cuál sobra es pura y se prueba en `monitor::stats`; lo que no se puede
//! verificar sin servidor es lo de antes: que la forma de cada índice se lea bien del catálogo
//! —las columnas de una expresión, las de un `INCLUDE`, la clase de operadores, la colación, el
//! orden, la restricción que sostiene, la identidad de réplica— en todo el rango de versiones
//! soportado. Es justo donde una diferencia de catálogo haría proponer que se borre un índice que
//! hace falta.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::monitor::stats;

fn test_urls() -> Vec<String> {
    std::env::var("PGFORGE_TEST_URLS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
        .collect()
}

async fn connect(url: &str) -> Arc<ServerHandle> {
    let (profile, password) = ConnectionProfile::from_url("test", url)
        .unwrap_or_else(|e| panic!("URL de prueba inválida ({url}): {e}"));
    let manager = ConnectionManager::new();
    manager
        .connect(profile, password)
        .await
        .unwrap_or_else(|e| panic!("no se pudo conectar a {url}: {e}"))
}

fn schema_name() -> String {
    format!("pgforge_indices_{}", std::process::id())
}

const FIXTURE: &str = r#"
CREATE TABLE {s}.trabajos (
    id serial PRIMARY KEY,
    codigo text UNIQUE,
    estado text,
    fecha date,
    datos jsonb
);

-- El par obvio: dos índices con las mismas columnas.
CREATE INDEX trabajos_estado_a ON {s}.trabajos (estado);
CREATE INDEX trabajos_estado_b ON {s}.trabajos (estado);

-- Y el que sobra porque otro lo empieza.
CREATE INDEX trabajos_fecha ON {s}.trabajos (fecha);
CREATE INDEX trabajos_fecha_estado ON {s}.trabajos (fecha, estado);

-- Los que no tienen que aparecer: el parcial responde otra pregunta, el gin es de otro tipo, y el
-- único sostiene una restricción aunque otro más largo lo empiece.
CREATE INDEX trabajos_estado_parcial ON {s}.trabajos (estado) WHERE fecha IS NOT NULL;
CREATE INDEX trabajos_datos ON {s}.trabajos USING gin (datos);
CREATE INDEX trabajos_codigo_fecha ON {s}.trabajos (codigo, fecha);

-- Lo que el catálogo escribe igual y no lo es: la clase de operadores, la colación y el orden. Las
-- cuatro columnas de abajo se leen todas como "cliente" o "creado".
CREATE TABLE {s}.pedidos (
    id serial PRIMARY KEY,
    cliente text,
    total numeric,
    creado timestamptz
);
CREATE INDEX pedidos_creado ON {s}.pedidos (creado);
CREATE INDEX pedidos_creado_desc ON {s}.pedidos (creado DESC);
CREATE INDEX pedidos_creado_nf ON {s}.pedidos (creado NULLS FIRST);
CREATE INDEX pedidos_cliente ON {s}.pedidos (cliente);
CREATE INDEX pedidos_cliente_pat ON {s}.pedidos (cliente text_pattern_ops);
CREATE INDEX pedidos_cliente_c ON {s}.pedidos (cliente COLLATE "C");
CREATE INDEX pedidos_cliente_inc ON {s}.pedidos (cliente) INCLUDE (total);

-- Un índice de restricción EXCLUDE: no es único ni primario, así que sin leer pg_constraint pasaba
-- las guardas y se proponía borrarlo. El servidor lo rechaza con "constraint ... requires it".
CREATE TABLE {s}.reservas (
    per daterange,
    EXCLUDE USING gist (per WITH &&)
);
CREATE INDEX reservas_per ON {s}.reservas USING gist (per);

-- La identidad de réplica es la única guarda que el servidor no hace cumplir: el DROP INDEX
-- funciona y la tabla queda sin identidad. Y el CLUSTER elige un índice que tampoco se puede
-- cambiar por otro. En los dos casos el desempate por nombre, sin la guarda, marcaría al que hay
-- que conservar.
CREATE TABLE {s}.eventos (id int NOT NULL, dato text);
CREATE UNIQUE INDEX eventos_u1 ON {s}.eventos (id);
CREATE UNIQUE INDEX eventos_u2 ON {s}.eventos (id);
ALTER TABLE {s}.eventos REPLICA IDENTITY USING INDEX eventos_u2;
CREATE INDEX eventos_dato_a ON {s}.eventos (dato);
CREATE INDEX eventos_dato_b ON {s}.eventos (dato);
CLUSTER {s}.eventos USING eventos_dato_b;

-- Las particiones: el par duplicado está una sola vez, en la madre.
CREATE TABLE {s}.partes (id int, f date) PARTITION BY RANGE (f);
CREATE TABLE {s}.partes_2025 PARTITION OF {s}.partes
    FOR VALUES FROM ('2025-01-01') TO ('2026-01-01');
CREATE INDEX partes_f_a ON {s}.partes (f);
CREATE INDEX partes_f_b ON {s}.partes (f);

-- Dos claves foráneas contra el mismo índice único: si el catálogo se leyera con un JOIN a
-- pg_constraint en vez de un EXISTS, este índice aparecería dos veces.
CREATE TABLE {s}.cabecera (codigo text);
CREATE UNIQUE INDEX cabecera_codigo_u ON {s}.cabecera (codigo);
CREATE TABLE {s}.detalle_uno (codigo text REFERENCES {s}.cabecera (codigo));
CREATE TABLE {s}.detalle_dos (codigo text REFERENCES {s}.cabecera (codigo));
"#;

async fn setup(handle: &ServerHandle, schema: &str) {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema};"
        ))
        .await
        .expect("no se pudo crear el esquema de prueba");
    client
        .batch_execute(&FIXTURE.replace("{s}", schema))
        .await
        .expect("no se pudo crear el fixture de índices");
}

async fn teardown(handle: &ServerHandle, schema: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .await;
    }
}

#[tokio::test]
async fn encuentra_los_indices_de_mas_contra_servidores_reales() {
    let urls = test_urls();
    if urls.is_empty() {
        eprintln!(
            "AVISO: PGFORGE_TEST_URLS no está definida, no se verificó nada contra un servidor real."
        );
        return;
    }

    for url in urls {
        let handle = connect(&url).await;
        let version = handle.caps.version;
        let schema = schema_name();
        setup(&handle, &schema).await;

        let outcome = {
            let handle = Arc::clone(&handle);
            let schema = schema.clone();
            tokio::spawn(async move {
                let client = handle.client(handle.default_database()).await.unwrap();
                let shapes: Vec<_> = stats::index_shapes(&client)
                    .await
                    .expect("no se pudieron leer los índices")
                    .into_iter()
                    .filter(|shape| shape.schema == schema)
                    .collect();

                let del = |name: &str| {
                    shapes
                        .iter()
                        .find(|shape| shape.name == name)
                        .unwrap_or_else(|| panic!("falta {name} entre los índices leídos"))
                        .clone()
                };

                // Las columnas salen del catálogo con el nombre escrito, no con el número de atributo.
                assert_eq!(del("trabajos_fecha_estado").columns, ["fecha", "estado"]);
                assert_eq!(del("trabajos_datos").method, "gin");
                assert!(del("trabajos_estado_parcial").predicate.is_some());
                assert!(del("trabajos_codigo_key").unique);
                assert!(del("trabajos_pkey").primary);

                // Lo que `pg_get_indexdef` por columna **no** escribe y hay que leer aparte: las
                // seis columnas de abajo se leen todas como "cliente" o "creado".
                assert_eq!(del("pedidos_cliente_pat").columns, ["cliente"]);
                assert_ne!(
                    del("pedidos_cliente_pat").opclasses,
                    del("pedidos_cliente").opclasses,
                    "text_pattern_ops es otra clase de operadores"
                );
                assert_ne!(
                    del("pedidos_cliente_c").collations,
                    del("pedidos_cliente").collations,
                    "COLLATE \"C\" es otra colación"
                );
                assert_eq!(del("pedidos_creado").options, [0]);
                assert_eq!(
                    del("pedidos_creado_desc").options,
                    [3],
                    "DESC trae también NULLS FIRST"
                );
                assert_eq!(del("pedidos_creado_nf").options, [2]);
                assert_eq!(del("pedidos_cliente_inc").included, ["total"]);
                assert!(del("pedidos_cliente").included.is_empty());

                // Las guardas, tal como las escribe el catálogo.
                assert_eq!(
                    del("reservas_per_excl").guards.constraint.as_deref(),
                    Some("reservas_per_excl"),
                    "un EXCLUDE no es único ni primario: sin pg_constraint no hay con qué frenarlo"
                );
                assert!(del("eventos_u2").guards.replica_identity);
                assert!(!del("eventos_u1").guards.replica_identity);
                assert!(del("eventos_dato_b").guards.clustered);
                assert!(del("cabecera_codigo_u").guards.referenced_by_fk);
                assert!(!del("pedidos_cliente").guards.referenced_by_fk);

                // Las particiones no entran, y la madre trae el tamaño de su árbol.
                assert!(
                    shapes.iter().all(|shape| shape.table != "partes_2025"),
                    "el índice de una partición cuelga del de su madre: no se propone aparte"
                );
                assert!(del("partes_f_a").partitioned);
                assert!(
                    del("partes_f_a").bytes > 0,
                    "pg_relation_size de un índice particionado da 0: el tamaño es el de sus partes"
                );

                // Un solo renglón por índice aunque dos claves foráneas lo referencien.
                assert_eq!(
                    shapes
                        .iter()
                        .filter(|shape| shape.name == "cabecera_codigo_u")
                        .count(),
                    1
                );

                let sobran = stats::redundancies(&shapes);
                let nombres: Vec<&str> = sobran.iter().map(|r| r.index.as_str()).collect();

                assert!(
                    nombres.contains(&"trabajos_estado_a") || nombres.contains(&"trabajos_estado_b"),
                    "uno de los dos índices iguales tenía que sobrar: {nombres:?}"
                );
                assert!(
                    !(nombres.contains(&"trabajos_estado_a")
                        && nombres.contains(&"trabajos_estado_b")),
                    "solo uno de los dos, o borrarlos a los dos deja la tabla sin índice: {nombres:?}"
                );
                assert!(
                    nombres.contains(&"trabajos_fecha"),
                    "(fecha) sobra porque (fecha, estado) lo empieza: {nombres:?}"
                );

                // El inverso exacto sobra —el btree se recorre para atrás—; el que solo mueve los
                // nulos, no.
                assert!(
                    nombres.contains(&"pedidos_creado_desc"),
                    "(creado DESC) es el inverso exacto de (creado): {nombres:?}"
                );
                // El de (cliente) sobra por el que además arrastra total en su INCLUDE.
                let cliente = sobran
                    .iter()
                    .find(|r| r.index == "pedidos_cliente")
                    .expect("(cliente) sobra por el que cubre");
                assert_eq!(cliente.covered_by, "pedidos_cliente_inc");

                // De cada par protegido sobra el otro, y el protegido no aparece nunca.
                for (sobra, se_queda) in [
                    ("reservas_per", "reservas_per_excl"),
                    ("eventos_u1", "eventos_u2"),
                    ("eventos_dato_a", "eventos_dato_b"),
                ] {
                    let encontrado = sobran
                        .iter()
                        .find(|r| r.index == sobra)
                        .unwrap_or_else(|| panic!("{sobra} tenía que sobrar: {nombres:?}"));
                    assert_eq!(encontrado.covered_by, se_queda);
                    assert!(
                        !nombres.contains(&se_queda),
                        "{se_queda} sostiene algo y no se propone: {nombres:?}"
                    );
                }

                for intocable in [
                    "trabajos_pkey",
                    "trabajos_codigo_key",
                    "trabajos_estado_parcial",
                    "trabajos_datos",
                    "trabajos_fecha_estado",
                    "trabajos_codigo_fecha",
                    "pedidos_creado_nf",
                    "pedidos_cliente_pat",
                    "pedidos_cliente_c",
                    "pedidos_cliente_inc",
                    "cabecera_codigo_u",
                ] {
                    assert!(
                        !nombres.contains(&intocable),
                        "{intocable} no tenía que aparecer como prescindible: {nombres:?}"
                    );
                }

                let uno = sobran
                    .iter()
                    .find(|r| r.index == "trabajos_fecha")
                    .expect("ya se verificó que está");
                assert!(
                    uno.drop_sql.contains("DROP INDEX CONCURRENTLY"),
                    "la sentencia que se muestra tiene que ser la que se ejecutaría: {}",
                    uno.drop_sql
                );

                // El par de la partitionada aparece una sola vez y sin CONCURRENTLY, que el
                // servidor rechaza sobre un índice particionado.
                let partes: Vec<_> = sobran
                    .iter()
                    .filter(|r| r.table == "partes" || r.table == "partes_2025")
                    .collect();
                assert_eq!(partes.len(), 1, "un par duplicado, un renglón: {partes:?}");
                assert!(
                    !partes[0].drop_sql.contains("CONCURRENTLY"),
                    "cannot drop partitioned index ... concurrently: {}",
                    partes[0].drop_sql
                );

                // La otra lista de índices que sobran, la de los que nunca se usaron, tiene que
                // frenar ante lo mismo: es la misma base con las mismas guardas leída por otra
                // consulta, así que acá se ve si las dos siguen de acuerdo.
                let stats: Vec<_> = stats::indexes(&client, 2000)
                    .await
                    .expect("no se pudieron leer las estadísticas de índices")
                    .into_iter()
                    .filter(|stat| stat.schema == schema)
                    .collect();

                let sin_uso = |name: &str| {
                    stats
                        .iter()
                        .find(|stat| stat.index == name)
                        .unwrap_or_else(|| panic!("falta {name} entre las estadísticas leídas"))
                        .unused
                };

                // Ninguno de estos dos es único ni primario, así que antes de leer las guardas los
                // dos salían marcados «nunca se usó».
                assert!(
                    !sin_uso("reservas_per_excl"),
                    "sostiene una restricción EXCLUDE: el servidor ni siquiera deja borrarlo"
                );
                assert!(
                    !sin_uso("eventos_dato_b"),
                    "es el índice del último CLUSTER"
                );
                // Y lo que no sostiene nada sí se marca, o la guarda habría tapado la lista entera.
                assert!(
                    sin_uso("trabajos_estado_a"),
                    "nadie lo consultó y no sostiene nada"
                );
            })
            .await
        };

        teardown(&handle, &schema).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({url})");
    }
}
