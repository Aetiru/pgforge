//! La vista de una base entera contra servidores reales.
//!
//! Se ejecuta contra todas las instancias de `PGFORGE_TEST_URLS`. Lo que se verifica es lo que se
//! rompe callado entre versiones: que las particiones no inflen el conteo de tablas, que las
//! relaciones del sistema no entren y que una base inexistente falle con un error y no con vacío.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::introspect;

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
    ConnectionManager::new()
        .connect(profile, password)
        .await
        .unwrap_or_else(|e| panic!("no se pudo conectar a {url}: {e}"))
}

fn schema_name() -> String {
    format!("pgforge_db_{}", std::process::id())
}

async fn setup(handle: &ServerHandle, schema: &str) {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE;
             CREATE SCHEMA {schema};
             CREATE TABLE {schema}.clientes (id bigint PRIMARY KEY, nombre text);
             CREATE TABLE {schema}.eventos (id bigint, dia date) PARTITION BY RANGE (dia);
             CREATE TABLE {schema}.eventos_2024 PARTITION OF {schema}.eventos
                 FOR VALUES FROM ('2024-01-01') TO ('2025-01-01');
             CREATE TABLE {schema}.eventos_2025 PARTITION OF {schema}.eventos
                 FOR VALUES FROM ('2025-01-01') TO ('2026-01-01');
             CREATE VIEW {schema}.resumen AS SELECT id FROM {schema}.clientes;"
        ))
        .await
        .unwrap();
}

async fn teardown(handle: &ServerHandle, schema: &str) {
    let client = handle.client(handle.default_database()).await.unwrap();
    let _ = client
        .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
        .await;
}

#[tokio::test]
async fn resume_una_base_sin_contar_particiones_ni_catalogos_del_sistema() {
    let urls = test_urls();
    if urls.is_empty() {
        eprintln!("PGFORGE_TEST_URLS sin definir: este test no verifica nada");
        return;
    }

    for url in urls {
        let handle = connect(&url).await;
        let schema = schema_name();
        setup(&handle, &schema).await;

        let database = handle.default_database().to_owned();
        let outcome = {
            let handle = handle.clone();
            let schema = schema.clone();
            let database = database.clone();
            tokio::spawn(async move {
                let overview = introspect::database_overview(&handle, &database)
                    .await
                    .unwrap();

                assert_eq!(overview.details.name, database);
                assert!(overview.details.size_bytes > 0);
                assert!(!overview.details.encoding.is_empty());

                let mine = overview.schemas.iter().find(|s| s.name == schema).unwrap();
                // `clientes` y `eventos` (la madre): las dos particiones no cuentan.
                assert_eq!(mine.tables, 2, "las particiones no son tablas aparte");
                assert_eq!(mine.views, 1);

                let names: Vec<_> = overview
                    .tables
                    .iter()
                    .filter(|t| t.schema == schema)
                    .map(|t| (t.name.as_str(), t.kind.as_str()))
                    .collect();
                assert!(names.contains(&("clientes", "r")));
                assert!(names.contains(&("eventos", "p")));
                assert!(names.contains(&("resumen", "v")));
                assert!(!names.iter().any(|(n, _)| n.starts_with("eventos_20")));

                assert!(
                    overview
                        .schemas
                        .iter()
                        .all(|s| !s.name.starts_with("pg_") && s.name != "information_schema"),
                    "los esquemas del sistema no entran"
                );
                assert!(overview.tables.iter().all(|t| t.estimated_rows >= 0.0));

                let missing = introspect::database_overview(&handle, "pgforge_no_existe_xyz").await;
                assert!(missing.is_err());
            })
            .await
        };

        teardown(&handle, &schema).await;
        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra {url}");
    }
}
