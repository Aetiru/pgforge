//! Extensiones contra servidores reales.
//!
//! Lo que se verifica acá no se puede sin servidor: que instalar deje la extensión en el esquema
//! pedido, que moverla de esquema funcione cuando es relocatable, y que quitarla la borre de verdad.
//!
//! Necesita una extensión que el servidor **ofrezca pero que no esté instalada**, para poder
//! instalarla y quitarla sin tocar nada que ya estuviera. Si no hay ninguna candidata, avisa por
//! stderr y no verifica nada —igual que `backup.rs` cuando falta `pg_dump`—.
//!
//! Fuera de alcance: la **actualización de versión** (`ALTER EXTENSION … UPDATE`). Depende de que el
//! paquete tenga un camino de actualización concreto entre dos versiones, que varía por extensión y
//! por servidor y volvería el test frágil. El armado del SQL de `Update` sí lo cubren los tests
//! unitarios de `ddl::extension`.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::ddl::extension::{self, ExtensionChange};
use pgforge_core::ddl::quote_ident;

/// Extensiones de contrib que suelen venir con el paquete y son relocatable y descartables. Se
/// prueba con la primera de la lista que el servidor ofrezca y que no esté ya instalada.
const CANDIDATES: &[&str] = &["pgcrypto", "citext", "hstore", "pg_trgm", "uuid-ossp"];

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

fn schema_name(suffix: &str) -> String {
    format!("pgforge_ext_{}_{suffix}", std::process::id())
}

/// La primera extensión candidata que el servidor ofrece y que no está instalada, con su bandera
/// de relocatable. `None` si ninguna está disponible.
async fn pick_candidate(handle: &ServerHandle) -> Option<(String, bool)> {
    let client = handle.client(handle.default_database()).await.unwrap();
    let rows = client
        .query(
            "SELECT ae.name::text, coalesce(av.relocatable, false)
               FROM pg_catalog.pg_available_extensions ae
               LEFT JOIN pg_catalog.pg_available_extension_versions av
                      ON av.name = ae.name AND av.version = ae.default_version
              WHERE ae.installed_version IS NULL AND ae.name::text = ANY($1)",
            &[&CANDIDATES],
        )
        .await
        .unwrap();

    let found: std::collections::HashMap<String, bool> = rows
        .into_iter()
        .map(|row| (row.get::<_, String>(0), row.get::<_, bool>(1)))
        .collect();

    // Se respeta el orden de preferencia de CANDIDATES, no el que devuelva el servidor.
    CANDIDATES
        .iter()
        .find_map(|name| found.get(*name).map(|reloc| (name.to_string(), *reloc)))
}

async fn exec(handle: &ServerHandle, sql: &str) {
    let client = handle.client(handle.default_database()).await.unwrap();
    let _ = client.batch_execute(sql).await;
}

async fn teardown(handle: &ServerHandle, extension: &str, schemas: &[&str]) {
    exec(
        handle,
        &format!(
            "DROP EXTENSION IF EXISTS {} CASCADE",
            quote_ident(extension)
        ),
    )
    .await;
    for schema in schemas {
        exec(
            handle,
            &format!("DROP SCHEMA IF EXISTS {} CASCADE", quote_ident(schema)),
        )
        .await;
    }
}

#[tokio::test]
async fn instala_mueve_y_quita_extensiones_contra_servidores_reales() {
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

        let Some((name, relocatable)) = pick_candidate(&handle).await else {
            eprintln!(
                "AVISO ({url}): ninguna de las extensiones candidatas está disponible sin instalar; \
                 no se verificó nada contra PostgreSQL {version}."
            );
            continue;
        };

        let schema = schema_name("a");
        let other = schema_name("b");

        // Por si quedó algo de una corrida anterior que se cortó a la mitad.
        teardown(&handle, &name, &[&schema, &other]).await;
        exec(&handle, &format!("CREATE SCHEMA {}", quote_ident(&schema))).await;
        exec(&handle, &format!("CREATE SCHEMA {}", quote_ident(&other))).await;

        let outcome = {
            let handle = Arc::clone(&handle);
            let name = name.clone();
            let schema = schema.clone();
            let other = other.clone();
            tokio::spawn(async move {
                instala_en_el_esquema_pedido(&handle, &name, &schema).await;
                if relocatable {
                    mueve_de_esquema(&handle, &name, &other).await;
                }
                quita_la_extension(&handle, &name).await;
            })
            .await
        };

        teardown(&handle, &name, &[&schema, &other]).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({url}) con «{name}»");
    }
}

async fn instala_en_el_esquema_pedido(handle: &ServerHandle, name: &str, schema: &str) {
    let database = handle.default_database().to_owned();

    extension::apply(
        handle,
        &database,
        &[ExtensionChange::Create {
            name: name.to_owned(),
            schema: Some(schema.to_owned()),
            version: None,
            cascade: true,
        }],
    )
    .await
    .expect("tenía que instalar la extensión");

    let info = extension::extension(handle, &database, name)
        .await
        .expect("la extensión tiene que existir");
    assert_eq!(info.name, name);
    assert_eq!(info.schema, schema, "tenía que quedar en el esquema pedido");
    assert!(
        !info.version.is_empty(),
        "tiene que traer la versión instalada"
    );
}

async fn mueve_de_esquema(handle: &ServerHandle, name: &str, other: &str) {
    let database = handle.default_database().to_owned();

    extension::apply(
        handle,
        &database,
        &[ExtensionChange::SetSchema {
            name: name.to_owned(),
            schema: other.to_owned(),
        }],
    )
    .await
    .expect("tenía que mover la extensión de esquema");

    let info = extension::extension(handle, &database, name).await.unwrap();
    assert_eq!(info.schema, other, "tenía que quedar en el esquema nuevo");
}

async fn quita_la_extension(handle: &ServerHandle, name: &str) {
    let database = handle.default_database().to_owned();

    extension::apply(
        handle,
        &database,
        &[ExtensionChange::Drop {
            name: name.to_owned(),
            cascade: true,
        }],
    )
    .await
    .expect("tenía que quitar la extensión");

    let gone = extension::extension(handle, &database, name).await;
    assert!(
        gone.is_err(),
        "la extensión ya no tendría que estar instalada"
    );
}
