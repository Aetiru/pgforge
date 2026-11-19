//! Lo que el editor de SQL le pide al servidor: revisar sin ejecutar, valores probables de una
//! columna y firmas de funciones. Las reglas que deciden qué mostrar son puras y se prueban en el
//! propio módulo; lo que solo se comprueba acá es el SQL contra PG 13 a 17.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::introspect::{self, ValuesKind};
use pgforge_core::sql::{Check, Limits, Outcome, QuerySession};

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
    format!("pgforge_editor_{}", std::process::id())
}

async fn setup(handle: &ServerHandle, schema: &str) {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE;
             CREATE SCHEMA {schema};
             CREATE TYPE {schema}.estado AS ENUM ('nuevo', 'activo', 'baja');
             CREATE TABLE {schema}.cliente (id int PRIMARY KEY, estado {schema}.estado, plan text);
             INSERT INTO {schema}.cliente
                  SELECT g, 'activo', CASE WHEN g % 10 = 0 THEN 'pro' ELSE 'basico' END
                    FROM generate_series(1, 500) g;
             ANALYZE {schema}.cliente;"
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
async fn el_editor_revisa_sugiere_valores_y_conoce_las_firmas() {
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
                let db = handle.default_database().to_owned();
                let session = QuerySession::open(&handle, &db).await.unwrap();

                // --- revisar sin ejecutar ------------------------------------------------------
                let bien = format!("SELECT id FROM {schema}.cliente WHERE id = 1");
                assert_eq!(session.check(&bien).await.unwrap(), Check::Ok);
                // El `;` final no cuenta como una segunda sentencia.
                assert_eq!(session.check(&format!("{bien};")).await.unwrap(), Check::Ok);

                // Un error de nombre apunta a la columna mal escrita, en base 0.
                let mal = format!("SELECT idd FROM {schema}.cliente");
                match session.check(&mal).await.unwrap() {
                    Check::Error { position, .. } => {
                        assert_eq!(position, Some("SELECT ".len()), "posición del error");
                    }
                    otro => panic!("tenía que ser un error, fue {otro:?}"),
                }

                // Revisar no ejecuta: un DELETE revisado no borra nada.
                session
                    .check(&format!("DELETE FROM {schema}.cliente"))
                    .await
                    .unwrap();
                let client = handle.client(&db).await.unwrap();
                let filas: i64 = client
                    .query_one(&format!("SELECT count(*) FROM {schema}.cliente"), &[])
                    .await
                    .unwrap()
                    .get(0);
                assert_eq!(filas, 500);

                // Varias sentencias y vacío se saltean.
                assert!(matches!(
                    session.check("SELECT 1; SELECT 2").await.unwrap(),
                    Check::Skipped { .. }
                ));
                assert!(matches!(
                    session.check("   ").await.unwrap(),
                    Check::Skipped { .. }
                ));

                // El lock_timeout de la pestaña vuelve a lo que era.
                match session
                    .run("SHOW lock_timeout", Limits::default())
                    .await
                    .unwrap()
                {
                    Outcome::Rows { rows, .. } => assert_eq!(
                        rows[0][0].as_deref(),
                        Some("0"),
                        "la revisión dejó el lock_timeout cambiado"
                    ),
                    otro => panic!("SHOW tenía que devolver filas: {otro:?}"),
                }

                // Con una transacción abierta no se revisa.
                session.begin_if_needed().await.unwrap();
                assert!(matches!(
                    session.check(&bien).await.unwrap(),
                    Check::Skipped { .. }
                ));
                session.rollback().await.unwrap();

                // --- valores probables ---------------------------------------------------------
                let valores = introspect::column_values(&client, &schema, "cliente", "estado", 10)
                    .await
                    .unwrap();
                assert_eq!(valores.kind, ValuesKind::Enum);
                let etiquetas: Vec<_> = valores.values.iter().map(|v| v.value.as_str()).collect();
                assert_eq!(etiquetas, ["nuevo", "activo", "baja"]);

                let planes = introspect::column_values(&client, &schema, "cliente", "plan", 10)
                    .await
                    .unwrap();
                assert_eq!(planes.kind, ValuesKind::Frequent);
                assert_eq!(planes.values[0].value, "basico");
                assert!(planes.values[0].frequency.unwrap() > 0.8);

                // Una columna que no existe no es un error.
                let nada = introspect::column_values(&client, &schema, "cliente", "no_existe", 10)
                    .await
                    .unwrap();
                assert_eq!(nada.kind, ValuesKind::None);

                // --- firmas --------------------------------------------------------------------
                let firmas = introspect::routine_signatures(&client, None, "date_trunc")
                    .await
                    .unwrap();
                assert!(!firmas.is_empty(), "date_trunc tenía que existir");
                assert!(firmas.iter().all(|f| f.schema == "pg_catalog"));
                assert!(firmas.iter().any(|f| f.arguments.contains("field text")));
                let solo = introspect::routine_signatures(&client, Some("public"), "date_trunc")
                    .await
                    .unwrap();
                assert!(solo.is_empty());
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
