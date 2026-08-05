//! Estimación de bloat contra servidores reales.
//!
//! Lo que no se puede verificar sin servidor: que `pgstattuple_approx` corra sobre las tablas del
//! catálogo, que la detección de la extensión funcione, y que una tabla a la que se le borró la
//! mitad de las filas sin vacuum aparezca con espacio muerto o libre estimado.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::monitor::Monitor;

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
    format!("pgforge_bloat_{}", std::process::id())
}

const FIXTURE: &str = r#"
CREATE EXTENSION IF NOT EXISTS pgstattuple;

CREATE TABLE {s}.inflada (id int PRIMARY KEY, relleno text);
INSERT INTO {s}.inflada SELECT g, repeat('x', 200) FROM generate_series(1, 5000) g;

-- Se borra la mitad de las filas y no se hace VACUUM: quedan tuplas muertas ocupando espacio, que es
-- justo lo que la estimación de bloat tiene que ver.
DELETE FROM {s}.inflada WHERE id % 2 = 0;
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
        .expect("no se pudo crear el fixture (¿la imagen trae pgstattuple?)");
}

async fn teardown(handle: &ServerHandle, schema: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .await;
    }
}

#[tokio::test]
async fn estima_el_bloat_contra_servidores_reales() {
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
                let monitor = Monitor::open(&handle, handle.default_database())
                    .await
                    .expect("no se pudo abrir el monitor");

                assert!(
                    monitor.has_bloat_stats().await.unwrap(),
                    "el fixture instaló pgstattuple, tendría que detectarse"
                );

                let rows = monitor
                    .bloat(200)
                    .await
                    .expect("no se pudo estimar el bloat");
                let nuestra = rows
                    .iter()
                    .find(|b| b.schema == schema && b.table == "inflada")
                    .expect("la tabla inflada tenía que aparecer en la estimación de bloat");

                assert!(nuestra.total_bytes > 0);
                assert!(
                    nuestra.dead_ratio > 0.0 || nuestra.free_ratio > 0.0,
                    "tras borrar la mitad de las filas tenía que haber espacio muerto o libre \
                     estimado, y vino dead={} free={}",
                    nuestra.dead_ratio,
                    nuestra.free_ratio
                );
                // Las fracciones vienen normalizadas a 0..1, no como porcentaje 0..100.
                assert!(nuestra.dead_ratio <= 1.0 && nuestra.free_ratio <= 1.0);
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
