//! Búsqueda de objetos por nombre, contra servidores reales.
//!
//! Es una sola consulta con tres uniones y un orden por relevancia: nada de eso se puede verificar
//! sin un catálogo de verdad. Lo que más se rompe al tocarla es lo de los bordes —el guión bajo que
//! no tiene que ser comodín, los esquemas del sistema que no tienen que aparecer— y por eso cada
//! uno tiene su comprobación.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::introspect::{self, NodeKind, SearchHit, TreeOptions};

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
    format!("pgforge_search_{}", std::process::id())
}

/// El fixture mezcla a propósito nombres que se pisan: `cliente` es prefijo de `clientes` y de
/// `cliente_pago`, y ese último tiene el guión bajo que un `LIKE` mal armado convertiría en comodín.
const FIXTURE: &str = r#"
CREATE TABLE {s}.cliente (id bigint PRIMARY KEY);
CREATE TABLE {s}.clientes_viejos (id bigint PRIMARY KEY);
CREATE TABLE {s}.cliente_pago (id bigint PRIMARY KEY);
CREATE VIEW {s}.cliente_activo AS SELECT id FROM {s}.cliente;
CREATE SEQUENCE {s}.cliente_seq;
CREATE TYPE {s}.cliente_estado AS ENUM ('alta', 'baja');

CREATE FUNCTION {s}.cliente_saludo(quien text) RETURNS text
    LANGUAGE sql IMMUTABLE AS $fn$ SELECT 'hola ' || quien $fn$;

CREATE TABLE {s}.factura (id bigint PRIMARY KEY);
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
        .expect("no se pudo crear el fixture");
}

async fn teardown(handle: &ServerHandle, schema: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .await;
    }
}

async fn find(handle: &ServerHandle, pattern: &str) -> Vec<SearchHit> {
    let database = handle.default_database().to_owned();
    introspect::search(handle, &database, pattern, TreeOptions::default(), 200)
        .await
        .unwrap_or_else(|e| panic!("falló la búsqueda de «{pattern}»: {e}"))
}

#[tokio::test]
async fn encuentra_objetos_por_nombre_contra_servidores_reales() {
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
            tokio::spawn(async move { assertions(&handle, &schema).await }).await
        };
        teardown(&handle, &schema).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({url})");
    }
}

async fn assertions(handle: &ServerHandle, schema: &str) {
    let mine = |hits: &[SearchHit]| -> Vec<(String, NodeKind)> {
        hits.iter()
            .filter(|hit| hit.schema == schema)
            .map(|hit| (hit.label.clone(), hit.kind))
            .collect()
    };

    // Un patrón encuentra objetos de las tres familias: relaciones, rutinas y tipos.
    let hits = find(handle, "cliente").await;
    let found = mine(&hits);
    let names: Vec<&str> = found.iter().map(|(label, _)| label.as_str()).collect();

    assert!(names.contains(&"cliente"), "falta la tabla: {names:?}");
    assert!(
        names.contains(&"clientes_viejos"),
        "falta la otra tabla: {names:?}"
    );
    assert!(
        names.contains(&"cliente_activo"),
        "falta la vista: {names:?}"
    );
    assert!(
        names.contains(&"cliente_seq"),
        "falta la secuencia: {names:?}"
    );
    assert!(
        names.contains(&"cliente_estado"),
        "falta el tipo enumerado: {names:?}"
    );
    assert!(
        names.contains(&"cliente_saludo(quien text)"),
        "la función tiene que venir con su firma, como en el árbol: {names:?}"
    );
    assert!(
        !names.contains(&"factura"),
        "apareció algo que no coincide: {names:?}"
    );

    // Cada uno con el tipo que le toca: de eso depende en qué carpeta lo revela la interfaz.
    let kind_of = |label: &str| {
        found
            .iter()
            .find(|(name, _)| name == label)
            .map(|(_, kind)| *kind)
            .unwrap_or_else(|| panic!("no se encontró «{label}»"))
    };
    assert_eq!(kind_of("cliente"), NodeKind::Table);
    assert_eq!(kind_of("cliente_activo"), NodeKind::View);
    assert_eq!(kind_of("cliente_seq"), NodeKind::Sequence);
    assert_eq!(kind_of("cliente_estado"), NodeKind::Type);
    assert_eq!(kind_of("cliente_saludo(quien text)"), NodeKind::Function);

    // El OID y el esquema son lo que la interfaz usa para abrir el camino hasta el objeto.
    let tabla = hits
        .iter()
        .find(|hit| hit.schema == schema && hit.label == "cliente")
        .unwrap();
    assert!(tabla.oid > 0, "sin OID no se puede revelar en el árbol");
    assert_eq!(tabla.database, handle.default_database());

    // El guión bajo es un carácter común en los nombres, no un comodín: si la consulta usara LIKE
    // sin escapar, «cliente_p» traería también «clientep» y cualquier otra letra en el medio.
    let underscore = find(handle, "cliente_pago").await;
    let underscore = mine(&underscore);
    assert_eq!(
        underscore.len(),
        1,
        "el guión bajo tiene que buscarse literal: {underscore:?}"
    );

    // La coincidencia exacta va primero: es lo que uno buscaba cuando escribió el nombre entero.
    let exact = find(handle, "cliente").await;
    let first = exact
        .iter()
        .find(|hit| hit.schema == schema)
        .expect("ninguna coincidencia en el esquema de prueba");
    assert_eq!(
        first.label, "cliente",
        "la coincidencia exacta tiene que encabezar el resultado"
    );

    // Sin mayúsculas ni minúsculas de por medio.
    let upper = find(handle, "CLIENTE_SEQ").await;
    assert!(
        mine(&upper).iter().any(|(label, _)| label == "cliente_seq"),
        "la búsqueda no distingue mayúsculas"
    );

    // Los esquemas del sistema quedan afuera mientras no se los pida.
    let system = find(handle, "pg_class").await;
    assert!(
        system.is_empty(),
        "sin pedir los esquemas del sistema no tiene que aparecer nada de pg_catalog: {system:?}"
    );

    let database = handle.default_database().to_owned();
    let asked = introspect::search(
        handle,
        &database,
        "pg_class",
        TreeOptions {
            show_system_schemas: true,
        },
        200,
    )
    .await
    .expect("falló la búsqueda con esquemas del sistema");
    assert!(
        asked.iter().any(|hit| hit.schema == "pg_catalog"),
        "pidiendo los del sistema, pg_class tiene que estar"
    );

    // Un patrón vacío no devuelve el catálogo entero.
    let empty = introspect::search(handle, &database, "   ", TreeOptions::default(), 200)
        .await
        .expect("falló la búsqueda vacía");
    assert!(empty.is_empty(), "un patrón vacío no busca nada");

    // El techo se respeta: es lo que evita traer cinco mil filas por escribir una letra.
    let capped = introspect::search(handle, &database, "e", TreeOptions::default(), 3)
        .await
        .expect("falló la búsqueda con techo");
    assert!(capped.len() <= 3, "el límite tiene que acotar el resultado");
}
