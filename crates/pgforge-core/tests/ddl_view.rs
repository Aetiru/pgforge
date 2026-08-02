//! Vistas y vistas materializadas contra servidores reales.
//!
//! Lo que se verifica acá no se puede verificar sin servidor: que `CREATE OR REPLACE VIEW`
//! realmente reemplace la consulta anterior, y que `REFRESH MATERIALIZED VIEW CONCURRENTLY` —que
//! necesita un índice único y un camino interno distinto al de un refresh simple— deje la vista al
//! día sin bloquear lectores.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::ddl::index::{self, IndexDef};
use pgforge_core::ddl::table::{self, ColumnDef, Identity, TableChange};
use pgforge_core::ddl::view::{self, ViewChange};

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
    format!("pgforge_view_{}", std::process::id())
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

async fn oid_of(handle: &ServerHandle, schema: &str, relname: &str) -> Option<u32> {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_opt(
            "SELECT c.oid FROM pg_class c
               JOIN pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = $1 AND c.relname = $2",
            &[&schema, &relname],
        )
        .await
        .unwrap()
        .map(|row| row.get(0))
}

async fn count_rows(handle: &ServerHandle, schema: &str, relname: &str) -> i64 {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(&format!("SELECT count(*) FROM {schema}.{relname}"), &[])
        .await
        .unwrap()
        .get(0)
}

fn plain(name: &str, type_name: &str) -> ColumnDef {
    ColumnDef {
        name: name.into(),
        type_name: type_name.into(),
        not_null: false,
        default: None,
        identity: None,
    }
}

#[tokio::test]
async fn crea_cambia_y_borra_vistas_contra_servidores_reales() {
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
                crea_tabla_base(&handle, &schema).await;
                vistas_normales(&handle, &schema).await;
                vistas_materializadas(&handle, &schema).await;
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

async fn crea_tabla_base(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    table::apply(
        handle,
        &database,
        &[TableChange::CreateTable {
            schema: schema.to_owned(),
            name: "ventas".into(),
            columns: vec![
                ColumnDef {
                    identity: Some(Identity::Always),
                    ..plain("id", "bigint")
                },
                plain("monto", "numeric(12,2)"),
            ],
        }],
    )
    .await
    .expect("tenía que crear la tabla base");

    let client = handle.client(&database).await.unwrap();
    client
        .batch_execute(&format!(
            "INSERT INTO {schema}.ventas (monto) VALUES (10), (20), (30)"
        ))
        .await
        .expect("tenía que cargar las filas de prueba");
}

async fn vistas_normales(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    view::apply(
        handle,
        &database,
        &[ViewChange::CreateView {
            schema: schema.to_owned(),
            name: "ventas_altas".into(),
            columns: vec![],
            query: format!("SELECT * FROM {schema}.ventas WHERE monto > 15"),
            replace: false,
        }],
    )
    .await
    .expect("tenía que crear la vista");

    let oid = oid_of(handle, schema, "ventas_altas")
        .await
        .expect("la vista tiene que existir después de crearla");
    assert_eq!(count_rows(handle, schema, "ventas_altas").await, 2);

    // CREATE OR REPLACE VIEW: mismo objeto, otra consulta.
    view::apply(
        handle,
        &database,
        &[ViewChange::CreateView {
            schema: schema.to_owned(),
            name: "ventas_altas".into(),
            columns: vec![],
            query: format!("SELECT * FROM {schema}.ventas WHERE monto > 25"),
            replace: true,
        }],
    )
    .await
    .expect("tenía que reemplazar la vista");

    assert_eq!(
        oid_of(handle, schema, "ventas_altas").await,
        Some(oid),
        "reemplazar la vista no tiene que cambiar su oid"
    );
    assert_eq!(count_rows(handle, schema, "ventas_altas").await, 1);

    let query = view::query_of(handle, &database, oid).await.unwrap();
    assert!(
        query.to_lowercase().contains("monto"),
        "tiene que traer la consulta que quedó después del reemplazo: {query}"
    );

    view::apply(
        handle,
        &database,
        &[ViewChange::DropView {
            schema: schema.to_owned(),
            name: "ventas_altas".into(),
            cascade: false,
        }],
    )
    .await
    .expect("tenía que borrar la vista");

    assert!(oid_of(handle, schema, "ventas_altas").await.is_none());
}

async fn vistas_materializadas(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    view::apply(
        handle,
        &database,
        &[ViewChange::CreateMaterializedView {
            schema: schema.to_owned(),
            name: "resumen".into(),
            columns: vec![],
            query: format!("SELECT id, monto FROM {schema}.ventas"),
            with_data: true,
        }],
    )
    .await
    .expect("tenía que crear la vista materializada");

    assert!(oid_of(handle, schema, "resumen").await.is_some());
    assert_eq!(count_rows(handle, schema, "resumen").await, 3);

    // REFRESH CONCURRENTLY necesita un índice único sobre la vista.
    index::create(
        handle,
        &database,
        &IndexDef {
            schema: schema.to_owned(),
            table: "resumen".into(),
            name: Some("resumen_id_idx".into()),
            unique: true,
            method: None,
            columns: vec!["id".into()],
            where_clause: None,
            concurrently: false,
        },
    )
    .await
    .expect("tenía que crear el índice único para poder refrescar CONCURRENTLY");

    let client = handle.client(&database).await.unwrap();
    client
        .batch_execute(&format!("INSERT INTO {schema}.ventas (monto) VALUES (40)"))
        .await
        .expect("tenía que agregar una fila nueva a la tabla base");

    view::apply(
        handle,
        &database,
        &[ViewChange::RefreshMaterializedView {
            schema: schema.to_owned(),
            name: "resumen".into(),
            concurrently: true,
        }],
    )
    .await
    .expect("tenía que refrescar CONCURRENTLY");

    assert_eq!(
        count_rows(handle, schema, "resumen").await,
        4,
        "el refresh tiene que traer la fila nueva"
    );

    view::apply(
        handle,
        &database,
        &[ViewChange::DropMaterializedView {
            schema: schema.to_owned(),
            name: "resumen".into(),
            cascade: false,
        }],
    )
    .await
    .expect("tenía que borrar la vista materializada");

    assert!(oid_of(handle, schema, "resumen").await.is_none());
}
