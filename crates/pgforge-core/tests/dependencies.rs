//! Dependencias de una relación contra servidores reales.
//!
//! Se ejecuta contra todas las instancias de `PGFORGE_TEST_URLS`. Lo que se verifica es que la
//! lista diga la verdad en las dos direcciones y que no repita: una vista que lee varias columnas
//! de la misma tabla deja varias filas en `pg_depend`.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::introspect::{self, Dependency, Direction};

fn test_urls() -> Vec<String> {
    std::env::var("PGFORGE_TEST_URLS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
        .collect()
}

const FIXTURE: &str = r#"
CREATE TABLE {s}.clientes (id bigint PRIMARY KEY, nombre text NOT NULL, ciudad text);
CREATE TABLE {s}.pedidos (
    id bigint PRIMARY KEY,
    cliente_id bigint NOT NULL REFERENCES {s}.clientes(id),
    jefe_id bigint REFERENCES {s}.pedidos(id)
);
CREATE VIEW {s}.clientes_por_ciudad AS SELECT id, nombre, ciudad FROM {s}.clientes;
CREATE VIEW {s}.resumen AS
    SELECT p.id, c.nombre FROM {s}.pedidos p JOIN {s}.clientes c ON c.id = p.cliente_id;
"#;

async fn connect(url: &str) -> Arc<ServerHandle> {
    let (profile, password) = ConnectionProfile::from_url("test", url)
        .unwrap_or_else(|e| panic!("URL de prueba inválida ({url}): {e}"));
    ConnectionManager::new()
        .connect(profile, password)
        .await
        .unwrap_or_else(|e| panic!("no se pudo conectar a {url}: {e}"))
}

fn schema_name() -> String {
    format!("pgforge_deps_{}", std::process::id())
}

async fn oid_of(handle: &ServerHandle, schema: &str, name: &str) -> u32 {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(
            "SELECT $1::text::regclass::oid",
            &[&format!("{schema}.{name}")],
        )
        .await
        .unwrap()
        .get(0)
}

fn find<'a>(list: &'a [Dependency], direction: Direction, name: &str) -> Vec<&'a Dependency> {
    list.iter()
        .filter(|d| d.direction == direction && d.name == name)
        .collect()
}

#[tokio::test]
async fn dice_que_depende_de_cada_relacion_contra_servidores_reales() {
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
        let client = handle.client(handle.default_database()).await.unwrap();
        client
            .batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema};"
            ))
            .await
            .unwrap();
        client
            .batch_execute(&FIXTURE.replace("{s}", &schema))
            .await
            .expect("no se pudo crear el fixture");

        // Un fallo no puede impedir borrar el esquema de prueba.
        let outcome = {
            let handle = Arc::clone(&handle);
            let schema = schema.clone();
            tokio::spawn(async move { assertions(&handle, &schema).await }).await
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

async fn assertions(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    // clientes: la leen dos vistas y la apunta una clave foránea de pedidos.
    let oid = oid_of(handle, schema, "clientes").await;
    let deps = introspect::dependencies(handle, &database, oid)
        .await
        .unwrap();
    assert_eq!(
        find(&deps, Direction::UsedBy, "clientes_por_ciudad").len(),
        1,
        "una sola fila por vista, aunque lea tres columnas"
    );
    assert_eq!(find(&deps, Direction::UsedBy, "resumen").len(), 1);
    let fk = find(&deps, Direction::UsedBy, "pedidos");
    assert_eq!(
        fk.len(),
        1,
        "la autorreferencia de pedidos no cuenta, la de clientes sí"
    );
    assert_eq!(fk[0].kind, "clave foránea");
    assert!(fk[0].detail.as_deref().unwrap().contains("REFERENCES"));
    assert!(deps.iter().all(|d| d.direction == Direction::UsedBy));

    // pedidos: apunta a clientes por clave foránea; la autorreferencia se omite.
    let oid = oid_of(handle, schema, "pedidos").await;
    let deps = introspect::dependencies(handle, &database, oid)
        .await
        .unwrap();
    assert_eq!(find(&deps, Direction::Uses, "clientes").len(), 1);
    assert_eq!(
        find(&deps, Direction::Uses, "pedidos").len(),
        0,
        "sin autorreferencias"
    );
    assert_eq!(find(&deps, Direction::UsedBy, "resumen").len(), 1);

    // La vista resumen lee las dos tablas.
    let oid = oid_of(handle, schema, "resumen").await;
    let deps = introspect::dependencies(handle, &database, oid)
        .await
        .unwrap();
    assert_eq!(find(&deps, Direction::Uses, "clientes")[0].kind, "tabla");
    assert_eq!(find(&deps, Direction::Uses, "pedidos").len(), 1);
}
