//! Comparación de datos contra servidores reales.
//!
//! Lo que no se prueba sin servidor: que el `SELECT ... ORDER BY ...::text COLLATE "C"` lo acepte
//! PostgreSQL en todo el rango, que NULL y cadena vacía lleguen distintos, y que el script armado
//! con literales escapados, ejecutado contra el destino, lo deje igual al origen (la prueba fuerte:
//! la segunda comparación tiene que dar cero diferencias). El merge y la generación del script ya
//! los cubren los tests unitarios de `compare::data`.

use std::sync::Arc;

use pgforge_core::compare::data::{self, RowDiff, Side};
use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};

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

fn schema(prefix: &str) -> String {
    format!("pgforge_dcmp_{prefix}_{}", std::process::id())
}

const SOURCE: &str = r#"
CREATE TABLE {s}.clientes (
    tenant int NOT NULL,
    id     int NOT NULL,
    nombre text,
    nota   text,
    activo boolean NOT NULL DEFAULT true,
    PRIMARY KEY (tenant, id)
);
INSERT INTO {s}.clientes VALUES
    (1, 1, 'Ana', 'igual', true),
    (1, 2, 'Beto', NULL, true),
    (1, 3, 'O''Brien \ barra', '', false),
    (1, 10, 'diez', 'solo en origen', true),
    (2, 1, 'Zoe', 'cambia', true);
CREATE TABLE {s}.sin_clave (a int);
"#;

const TARGET: &str = r#"
CREATE TABLE {s}.clientes (
    tenant int NOT NULL,
    id     int NOT NULL,
    nombre text,
    nota   text,
    activo boolean NOT NULL DEFAULT true,
    PRIMARY KEY (tenant, id)
);
INSERT INTO {s}.clientes VALUES
    (1, 1, 'Ana', 'igual', true),
    (1, 2, 'Beto', '', true),
    (1, 9, 'sobra', NULL, true),
    (2, 1, 'Zoe', 'otro', true);
"#;

async fn setup(handle: &ServerHandle, name: &str, fixture: &str) {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {name} CASCADE; CREATE SCHEMA {name};"
        ))
        .await
        .expect("no se pudo crear el esquema de prueba");
    client
        .batch_execute(&fixture.replace("{s}", name))
        .await
        .expect("no se pudo crear el fixture");
}

async fn teardown(handle: &ServerHandle, name: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {name} CASCADE"))
            .await;
    }
}

#[tokio::test]
async fn compara_datos_y_el_script_deja_el_destino_igual_contra_servidores_reales() {
    let urls = test_urls();
    if urls.is_empty() {
        eprintln!(
            "AVISO: PGFORGE_TEST_URLS no está definida, no se verificó nada contra un servidor real."
        );
        return;
    }

    for (position, url) in urls.iter().enumerate() {
        let target_url = &urls[(position + 1) % urls.len()];
        let source = connect(url).await;
        let target = connect(target_url).await;
        let (source_schema, target_schema) = (schema("src"), schema("dst"));

        setup(&source, &source_schema, SOURCE).await;
        setup(&target, &target_schema, TARGET).await;

        let outcome = {
            let (source, target) = (Arc::clone(&source), Arc::clone(&target));
            let (source_schema, target_schema) = (source_schema.clone(), target_schema.clone());
            tokio::spawn(async move {
                assertions(&source, &source_schema, &target, &target_schema).await
            })
            .await
        };

        teardown(&source, &source_schema).await;
        teardown(&target, &target_schema).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!(
            "ok: PostgreSQL {} ({url}) contra PostgreSQL {} ({target_url})",
            source.caps.version, target.caps.version
        );
    }
}

async fn run_compare(
    source: &ServerHandle,
    source_schema: &str,
    target: &ServerHandle,
    target_schema: &str,
    table: &str,
    max: usize,
) -> pgforge_core::Result<data::DataComparison> {
    data::compare(
        Side {
            handle: source,
            database: source.default_database(),
            schema: source_schema,
            table,
            sink: None,
        },
        Side {
            handle: target,
            database: target.default_database(),
            schema: target_schema,
            table,
            sink: None,
        },
        max,
        &|_| {},
    )
    .await
}

async fn assertions(
    source: &ServerHandle,
    source_schema: &str,
    target: &ServerHandle,
    target_schema: &str,
) {
    let comparison = run_compare(
        source,
        source_schema,
        target,
        target_schema,
        "clientes",
        1000,
    )
    .await
    .expect("no se pudo comparar");

    assert_eq!(comparison.layout.key_columns, ["tenant", "id"]);
    assert_eq!(comparison.counts.equal, 1, "{:?}", comparison.counts);
    assert_eq!(comparison.counts.only_source, 2);
    assert_eq!(comparison.counts.only_target, 1);
    assert_eq!(comparison.counts.different, 2);

    // NULL contra cadena vacía tiene que verse como diferencia, y solo en la columna que cambió.
    let nota = comparison
        .layout
        .columns
        .iter()
        .position(|c| c == "nota")
        .unwrap();
    assert!(comparison.rows.iter().any(|row| matches!(
        row,
        RowDiff::Different { key, changed, .. } if key == &["1", "2"] && changed == &[nota]
    )));

    // El tope corta el informe pero no el conteo.
    let capped = run_compare(source, source_schema, target, target_schema, "clientes", 2)
        .await
        .unwrap();
    assert_eq!(capped.rows.len(), 2);
    assert!(capped.truncated);
    assert_eq!(capped.counts.different, 2);

    // Sin clave no se adivina.
    let error = run_compare(
        source,
        source_schema,
        target,
        target_schema,
        "sin_clave",
        10,
    )
    .await
    .expect_err("una tabla sin clave no se compara");
    assert!(error.to_string().contains("clave"), "{error}");

    // La prueba fuerte: el script, ejecutado contra el destino, lo deja igual al origen.
    let script = data::script(&comparison.statements);
    let client = target.client(target.default_database()).await.unwrap();
    client
        .batch_execute(&script)
        .await
        .unwrap_or_else(|e| panic!("el script no corre: {e}\n{script}"));

    let again = run_compare(
        source,
        source_schema,
        target,
        target_schema,
        "clientes",
        1000,
    )
    .await
    .unwrap();
    assert_eq!(again.counts.differences(), 0, "{script}\n{:?}", again.rows);
    assert_eq!(again.counts.equal, 5);
}
