//! Los diagnósticos del monitoreo contra servidores reales.
//!
//! Las reglas que deciden qué es «para revisar» son puras y se prueban sin servidor; lo que solo se
//! puede comprobar acá es el SQL: que cada consulta exista y tenga las columnas que se leen en todo
//! el rango PG 13 a 17 (`pg_stat_checkpointer` y `pg_stat_wal` cambian según la versión), y que un
//! objeto recién creado aparezca donde corresponde.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::monitor::diagnose::{self, Level};
use pgforge_core::monitor::trend::{self, Sample, TrendStore};

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
    format!("pgforge_diag_{}", std::process::id())
}

async fn setup(handle: &ServerHandle, schema: &str) {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE;
             CREATE SCHEMA {schema};
             CREATE SEQUENCE {schema}.cuenta AS smallint MAXVALUE 100;
             SELECT setval('{schema}.cuenta', 90);
             CREATE TABLE {schema}.filas (id int PRIMARY KEY, texto text);
             INSERT INTO {schema}.filas SELECT g, repeat('x', 100) FROM generate_series(1, 2000) g;
             SELECT count(*) FROM {schema}.filas;"
        ))
        .await
        .expect("no se pudo armar el esquema de prueba");
}

async fn teardown(handle: &ServerHandle, schema: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .await;
    }
}

#[tokio::test]
async fn los_diagnosticos_leen_el_catalogo_en_todas_las_versiones() {
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
                let session = handle
                    .open_session(handle.default_database(), None)
                    .await
                    .expect("no se pudo abrir la sesión");
                let client = session.client();

                // Una secuencia de smallint puesta en 90 de 100 está para revisar.
                let sequences = diagnose::sequences(client, 1000).await.unwrap();
                let cuenta = sequences
                    .iter()
                    .find(|s| s.schema == schema && s.name == "cuenta")
                    .expect("la secuencia de prueba tenía que aparecer");
                assert!(cuenta.used_ratio > 0.85, "usó {}", cuenta.used_ratio);
                assert_eq!(cuenta.level, Level::Review);
                assert_eq!(cuenta.remaining, 10);

                let wrap = diagnose::wraparound(client, 10).await.unwrap();
                assert!(!wrap.databases.is_empty());
                assert!(wrap.freeze_max_age > 0);

                let vacuum = diagnose::autovacuum(client, 1000).await.unwrap();
                assert!(
                    vacuum
                        .iter()
                        .any(|r| r.schema == schema && r.table == "filas"),
                    "la tabla de prueba tenía que estar en las estadísticas de autovacuum"
                );

                let cache = diagnose::cache(client, 1000).await.unwrap();
                assert!(cache.tables.iter().any(|t| t.schema == schema));

                let report = diagnose::connections(client, &handle.caps).await.unwrap();
                assert!(report.max_connections > 0);
                assert!(report.total >= 0);

                let map = diagnose::space_map(client, 1000).await.unwrap();
                assert!(map.database_bytes > 0);
                let filas = map
                    .entries
                    .iter()
                    .find(|e| e.schema == schema && e.table == "filas")
                    .expect("la tabla de prueba tenía que estar en el mapa");
                assert!(filas.heap_bytes > 0 && filas.index_bytes > 0);

                let checkpoints = diagnose::checkpoints(client, &handle.caps).await.unwrap();
                assert!(checkpoints.timed + checkpoints.requested >= 0);
                // PG 13 no tiene `pg_stat_wal`; de la 14 en adelante tiene que venir.
                assert_eq!(checkpoints.wal.is_some(), handle.caps.has_pg_stat_wal());

                let review =
                    pgforge_core::monitor::config_review::config_review(client, Some(16 << 30))
                        .await
                        .unwrap();
                // Con 16 GB declarados y el valor de fábrica, shared_buffers siempre sale marcado
                // salvo que el servidor de prueba ya lo tenga subido.
                assert!(review.findings.iter().all(|f| !f.name.is_empty()));
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

/// No necesita servidor: cubre que el almacén y la recta trabajen juntos de punta a punta.
#[test]
fn las_muestras_guardadas_alcanzan_para_proyectar() {
    let mut store = TrendStore::open_in_memory().unwrap();
    for day in 0..4_i64 {
        let sample = Sample {
            kind: "database",
            name: "app".into(),
            value: 1_000.0 + 100.0 * day as f64,
        };
        assert!(store.record("s", "app", day * 86_400, &[sample]).unwrap());
    }
    let series = store.series("s", "app", "database", "app").unwrap();
    let forecast = trend::forecast(&series, Some(2_000.0)).unwrap();
    assert!((forecast.per_day - 100.0).abs() < 1e-6);
    assert!((forecast.days_to_limit.unwrap() - 7.0).abs() < 1e-6);
}
