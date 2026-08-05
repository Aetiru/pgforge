//! Tablas y columnas que necesitan comillas: `"Procesos"`, `"Id"`, `"nombre del proceso"`.
//!
//! El caso importa porque casi todo el SQL generado interpola identificadores, y un identificador con
//! mayúsculas es el único que falla distinto: sin comillas PostgreSQL lo pasa a minúsculas y el error
//! que devuelve habla de una tabla o columna «que no existe», no de comillas.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::data::{self, edit::Change, edit::Values, page::Cursor};
use pgforge_core::sql::{self, Limits, Outcome, QuerySession};

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
    format!("pgforge_quoted_{}", std::process::id())
}

fn values(pairs: &[(&str, Option<&str>)]) -> Values {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.map(str::to_owned)))
        .collect()
}

#[tokio::test]
async fn edita_tablas_con_identificadores_citados_contra_servidores_reales() {
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

        preparar(&handle, &schema).await;

        let outcome = {
            let handle = Arc::clone(&handle);
            let schema = schema.clone();
            tokio::spawn(async move {
                let oid = oid_de_procesos(&handle, &schema).await;
                lee_la_pagina(&handle, oid).await;
                actualiza_y_borra(&handle, oid).await;
                el_editor_escribe_sobre_la_tabla_citada(&handle, &schema).await;
            })
            .await
        };

        limpiar(&handle, &schema).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({url})");
    }
}

/// El mismo camino que el editor de SQL: partir el script y correr cada sentencia por la sesión de
/// la pestaña. Se prueba entero porque el partidor es el único punto donde una comilla doble podría
/// cortar el texto en el lugar equivocado.
async fn el_editor_escribe_sobre_la_tabla_citada(handle: &ServerHandle, schema: &str) {
    let session = QuerySession::open(handle, handle.default_database())
        .await
        .expect("no se pudo abrir la sesión de consulta");

    let script = format!(
        r#"UPDATE "{schema}"."Procesos" SET "Estado" = 'listo' WHERE "Nombre" = 'UNO';
           DELETE FROM "{schema}"."Procesos" WHERE "Nombre" = 'tres';"#
    );

    let statements = sql::split(&script);
    assert_eq!(
        statements.len(),
        2,
        "las comillas dobles no pueden hacer que el partidor pierda una sentencia: {statements:?}"
    );

    for statement in &statements {
        let outcome = session
            .run(&statement.text, Limits::default())
            .await
            .unwrap_or_else(|e| panic!("falló «{}»: {e}", statement.text));

        match outcome {
            Outcome::Command { tag, affected, .. } => {
                assert_eq!(affected, 1, "{tag} tenía que afectar una fila");
            }
            Outcome::Rows { .. } => panic!("se esperaba un comando"),
        }
    }
}

async fn preparar(handle: &ServerHandle, schema: &str) {
    let client = handle
        .client(handle.default_database())
        .await
        .expect("no se pudo tomar una conexión del pool");
    client
        .batch_execute(&format!(
            r#"CREATE SCHEMA "{schema}";
               CREATE TABLE "{schema}"."Procesos" (
                   "Id"     bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
                   "Nombre" text NOT NULL,
                   "Estado" text
               );
               INSERT INTO "{schema}"."Procesos" ("Nombre", "Estado")
                    VALUES ('uno', 'activo'), ('dos', NULL);"#
        ))
        .await
        .expect("no se pudo preparar la tabla de prueba");
}

async fn limpiar(handle: &ServerHandle, schema: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#))
            .await;
    }
}

async fn oid_de_procesos(handle: &ServerHandle, schema: &str) -> u32 {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(
            "SELECT c.oid FROM pg_class c
               JOIN pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = $1 AND c.relname = 'Procesos'",
            &[&schema],
        )
        .await
        .expect("no se encontró la tabla")
        .get(0)
}

async fn lee_la_pagina(handle: &ServerHandle, oid: u32) {
    let database = handle.default_database();
    let shape = data::shape::shape(handle, database, oid)
        .await
        .expect("no se pudo leer la forma de la tabla");

    assert_eq!(shape.name, "Procesos");
    assert_eq!(
        shape.read_only, None,
        "la tabla tiene clave primaria: tenía que abrirse editable"
    );

    let page = data::page::page(handle, database, &shape, None, 200)
        .await
        .expect("no se pudo leer la página");
    assert_eq!(page.rows.len(), 2);
    assert_eq!(page.columns[0], "Id");

    // La segunda página se pide por clave, con el valor tal como lo devolvió la primera.
    let cursor = Cursor::After {
        key: vec![page.rows[0][0].clone().unwrap()],
    };
    let siguiente = data::page::page(handle, database, &shape, Some(&cursor), 200)
        .await
        .expect("no se pudo paginar por clave");
    assert_eq!(siguiente.rows.len(), 1);
}

async fn actualiza_y_borra(handle: &ServerHandle, oid: u32) {
    let database = handle.default_database();
    let shape = data::shape::shape(handle, database, oid).await.unwrap();

    let page = data::page::page(handle, database, &shape, None, 200)
        .await
        .unwrap();
    let primera = page.rows[0][0].clone().unwrap();
    let segunda = page.rows[1][0].clone().unwrap();

    let applied = data::edit::apply(
        handle,
        database,
        &shape,
        &[Change::Update {
            key: vec![primera.clone()],
            original: values(&[("Nombre", Some("uno"))]),
            changes: values(&[("Nombre", Some("UNO"))]),
        }],
    )
    .await
    .expect("el UPDATE sobre una tabla citada tenía que funcionar");
    assert_eq!(applied.updated, 1);

    let applied = data::edit::apply(
        handle,
        database,
        &shape,
        &[Change::Delete { key: vec![segunda] }],
    )
    .await
    .expect("el DELETE sobre una tabla citada tenía que funcionar");
    assert_eq!(applied.deleted, 1);

    let applied = data::edit::apply(
        handle,
        database,
        &shape,
        &[Change::Insert {
            values: values(&[("Nombre", Some("tres")), ("Estado", None)]),
        }],
    )
    .await
    .expect("el INSERT sobre una tabla citada tenía que funcionar");
    assert_eq!(applied.inserted, 1);

    let page = data::page::page(handle, database, &shape, None, 200)
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 2);
    assert_eq!(page.rows[0][1].as_deref(), Some("UNO"));
}
