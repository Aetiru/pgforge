//! Funciones y procedimientos contra servidores reales.
//!
//! Lo que se verifica acá no se puede verificar sin servidor: que `CREATE OR REPLACE FUNCTION`
//! reemplace de verdad el cuerpo anterior, y que `pg_get_function_identity_arguments` desambigüe
//! entre dos funciones con el mismo nombre y firmas distintas al borrar una sola.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::ddl::function;

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
    format!("pgforge_function_{}", std::process::id())
}

async fn setup(handle: &ServerHandle, schema: &str) {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema};"
        ))
        .await
        .expect("no se pudo crear el esquema de prueba");
}

async fn teardown(handle: &ServerHandle, schema: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .await;
    }
}

/// A diferencia de una tabla o una vista, una función no vive en `pg_class`: hay que buscarla en
/// `pg_proc`. Sirve para el caso sin sobrecarga, donde el nombre solo ya identifica una sola fila.
async fn fn_oid(handle: &ServerHandle, schema: &str, name: &str) -> u32 {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(
            "SELECT p.oid FROM pg_catalog.pg_proc p
               JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
              WHERE n.nspname = $1 AND p.proname = $2",
            &[&schema, &name],
        )
        .await
        .unwrap_or_else(|e| panic!("no se encontró {schema}.{name}: {e}"))
        .get(0)
}

/// Para el caso con sobrecarga: desambigua por el tipo del primer argumento en vez de adivinar el
/// formato exacto de `pg_get_function_identity_arguments` (que es justo lo que el propio test
/// verifica más abajo, así que acá no conviene depender de él).
async fn fn_oid_by_first_arg(handle: &ServerHandle, schema: &str, name: &str, arg_type: &str) -> u32 {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(
            "SELECT p.oid FROM pg_catalog.pg_proc p
               JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
              WHERE n.nspname = $1 AND p.proname = $2
                AND pg_catalog.format_type(p.proargtypes[0], NULL) = $3",
            &[&schema, &name, &arg_type],
        )
        .await
        .unwrap_or_else(|e| panic!("no se encontró {schema}.{name}({arg_type}, ...): {e}"))
        .get(0)
}

async fn fn_exists(handle: &ServerHandle, schema: &str, name: &str) -> bool {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_opt(
            "SELECT 1 FROM pg_catalog.pg_proc p
               JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
              WHERE n.nspname = $1 AND p.proname = $2",
            &[&schema, &name],
        )
        .await
        .unwrap()
        .is_some()
}

async fn fn_count(handle: &ServerHandle, schema: &str, name: &str) -> i64 {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(
            "SELECT count(*) FROM pg_catalog.pg_proc p
               JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
              WHERE n.nspname = $1 AND p.proname = $2",
            &[&schema, &name],
        )
        .await
        .unwrap()
        .get(0)
}

#[tokio::test]
async fn crea_reemplaza_y_borra_funciones_contra_servidores_reales() {
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
                crea_y_reemplaza(&handle, &schema).await;
                borra_con_identity_args(&handle, &schema).await;
                desambigua_sobrecargas(&handle, &schema).await;
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

async fn crea_y_reemplaza(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    function::apply(
        handle,
        &database,
        &format!(
            "CREATE FUNCTION {schema}.duplicar(n integer) RETURNS integer \
             LANGUAGE sql AS $$ SELECT n * 2 $$"
        ),
    )
    .await
    .expect("tenía que crear la función");

    let client = handle.client(&database).await.unwrap();
    let resultado: i32 = client
        .query_one(&format!("SELECT {schema}.duplicar(21)"), &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(resultado, 42);

    // CREATE OR REPLACE FUNCTION: mismo nombre y firma, otro cuerpo.
    function::apply(
        handle,
        &database,
        &format!(
            "CREATE OR REPLACE FUNCTION {schema}.duplicar(n integer) RETURNS integer \
             LANGUAGE sql AS $$ SELECT n * 3 $$"
        ),
    )
    .await
    .expect("tenía que reemplazar la función");

    let resultado: i32 = client
        .query_one(&format!("SELECT {schema}.duplicar(10)"), &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(resultado, 30, "el reemplazo tenía que cambiar el cuerpo");
}

async fn borra_con_identity_args(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    let oid = fn_oid(handle, schema, "duplicar").await;
    let args = function::identity_args(handle, &database, oid).await.unwrap();
    assert!(
        args.contains("integer"),
        "tiene que traer el tipo del argumento: {args}"
    );

    function::drop(handle, &database, schema, "duplicar", &args, false, false)
        .await
        .expect("tenía que borrar la función");

    assert!(!fn_exists(handle, schema, "duplicar").await);
}

async fn desambigua_sobrecargas(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    function::apply(
        handle,
        &database,
        &format!(
            "CREATE FUNCTION {schema}.combinar(a integer) RETURNS text \
             LANGUAGE sql AS $$ SELECT a::text $$"
        ),
    )
    .await
    .expect("tenía que crear la primera sobrecarga");

    function::apply(
        handle,
        &database,
        &format!(
            "CREATE FUNCTION {schema}.combinar(a text) RETURNS text \
             LANGUAGE sql AS $$ SELECT a $$"
        ),
    )
    .await
    .expect("tenía que crear la segunda sobrecarga");

    assert_eq!(fn_count(handle, schema, "combinar").await, 2);

    let oid_integer = fn_oid_by_first_arg(handle, schema, "combinar", "integer").await;
    let args_integer = function::identity_args(handle, &database, oid_integer)
        .await
        .unwrap();

    function::drop(handle, &database, schema, "combinar", &args_integer, false, false)
        .await
        .expect("tenía que borrar solo la sobrecarga de integer");

    assert_eq!(
        fn_count(handle, schema, "combinar").await,
        1,
        "borrar una sobrecarga no puede tocar la otra"
    );
    fn_oid_by_first_arg(handle, schema, "combinar", "text").await;
}
