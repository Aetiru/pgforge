//! Estadísticas de una sola tabla contra servidores reales.
//!
//! No se afirman conteos de filas: los contadores de `pg_stat_user_tables` se vuelcan de forma
//! asíncrona y cambian de versión en versión, así que un número exacto haría un test que falla según
//! la hora. Lo que sí es exacto: que encuentre la tabla por OID, que los tamaños sean de verdad y que
//! una tabla particionada conteste `None` en todo el rango: PG 13 no la lista en
//! `pg_stat_user_tables` y PG 14 en adelante sí, con ceros, y esa diferencia es justo lo que este test
//! atrapa.

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
    ConnectionManager::new()
        .connect(profile, password)
        .await
        .unwrap_or_else(|e| panic!("no se pudo conectar a {url}: {e}"))
}

async fn oid_of(client: &tokio_postgres::Client, qualified: &str) -> u32 {
    client
        .query_one("SELECT $1::text::regclass::oid", &[&qualified])
        .await
        .unwrap()
        .get(0)
}

#[tokio::test]
async fn trae_las_estadisticas_de_una_tabla_contra_servidores_reales() {
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
        let schema = format!("pgforge_tstat_{}", std::process::id());
        let client = handle.client(handle.default_database()).await.unwrap();
        client
            .batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {schema} CASCADE;
                 CREATE SCHEMA {schema};
                 CREATE TABLE {schema}.clientes (id bigint PRIMARY KEY, nombre text);
                 INSERT INTO {schema}.clientes SELECT g, 'cliente ' || g FROM generate_series(1, 2000) g;
                 CREATE TABLE {schema}.ventas (id bigint, fecha date) PARTITION BY RANGE (fecha);"
            ))
            .await
            .expect("no se pudo armar el fixture");

        let outcome = {
            let client_oid = oid_of(&client, &format!("{schema}.clientes")).await;
            let parent_oid = oid_of(&client, &format!("{schema}.ventas")).await;
            let handle = Arc::clone(&handle);
            let schema = schema.clone();
            tokio::spawn(async move {
                let client = handle.client(handle.default_database()).await.unwrap();

                let stat = stats::table_by_oid(&client, client_oid)
                    .await
                    .unwrap()
                    .expect("la tabla tenía que tener estadísticas");
                assert_eq!(stat.schema, schema);
                assert_eq!(stat.table, "clientes");
                assert!(stat.table_bytes > 0, "la tabla ocupa algo");
                assert!(stat.index_bytes > 0, "la clave primaria ocupa algo");
                assert_eq!(stat.total_bytes, stat.table_bytes + stat.index_bytes);

                assert!(
                    stats::table_by_oid(&client, parent_oid)
                        .await
                        .unwrap()
                        .is_none(),
                    "una tabla particionada no tiene estadísticas propias, en ninguna versión"
                );
                assert!(
                    stats::table_by_oid(&client, 0).await.unwrap().is_none(),
                    "un OID que no existe no es un error"
                );
            })
            .await
        };
        let _ = client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE;"))
            .await;
        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({url})");
    }
}
