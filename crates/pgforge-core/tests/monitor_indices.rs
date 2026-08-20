//! Índices que sobran, contra servidores reales.
//!
//! La regla que decide cuál sobra es pura y se prueba en `monitor::stats`; lo que no se puede
//! verificar sin servidor es lo de antes: que las columnas de cada índice se lean bien del catálogo
//! —incluidas las de una expresión, las de un `INCLUDE` y las de un índice parcial— en todo el
//! rango de versiones soportado. Es justo donde una diferencia de catálogo haría proponer que se
//! borre un índice que hace falta.

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

                for intocable in [
                    "trabajos_pkey",
                    "trabajos_codigo_key",
                    "trabajos_estado_parcial",
                    "trabajos_datos",
                    "trabajos_fecha_estado",
                    "trabajos_codigo_fecha",
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
