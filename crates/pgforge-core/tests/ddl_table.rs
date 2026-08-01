//! Estructura de tablas contra servidores reales.
//!
//! Lo que se verifica acá no se puede verificar sin servidor: que el DDL generado sea sintaxis
//! válida de verdad (identidad, cambios de tipo, renombres), y que un lote con un paso inválido no
//! deje nada aplicado.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::ddl::table::{self, ColumnDef, Identity, TableChange};
use pgforge_core::data;

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
    format!("pgforge_ddl_{}", std::process::id())
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

async fn oid_of(handle: &ServerHandle, schema: &str, table: &str) -> Option<u32> {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_opt(
            "SELECT c.oid FROM pg_class c
               JOIN pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = $1 AND c.relname = $2",
            &[&schema, &table],
        )
        .await
        .unwrap()
        .map(|row| row.get(0))
}

fn plain(name: &str, type_name: &str) -> ColumnDef {
    ColumnDef {
        name: name.to_owned(),
        type_name: type_name.to_owned(),
        not_null: false,
        default: None,
        identity: None,
    }
}

#[tokio::test]
async fn crea_cambia_y_borra_una_tabla_contra_servidores_reales() {
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
                crea_la_tabla(&handle, &schema).await;
                cambia_columnas(&handle, &schema).await;
                renombra_tabla_y_columna(&handle, &schema).await;
                un_lote_fallido_no_deja_nada(&handle, &schema).await;
                borra_columna_y_tabla(&handle, &schema).await;
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

async fn crea_la_tabla(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();
    let changes = [TableChange::CreateTable {
        schema: schema.to_owned(),
        name: "clientes".into(),
        columns: vec![
            ColumnDef {
                identity: Some(Identity::Always),
                ..plain("id", "bigint")
            },
            {
                let mut nombre = plain("nombre", "text");
                nombre.not_null = true;
                nombre
            },
            {
                let mut activo = plain("activo", "boolean");
                activo.default = Some("true".into());
                activo
            },
        ],
    }];

    table::apply(handle, &database, &changes)
        .await
        .expect("tenía que crear la tabla");

    let oid = oid_of(handle, schema, "clientes")
        .await
        .expect("la tabla tiene que existir después de crearla");
    let shape = data::shape(handle, &database, oid).await.unwrap();

    assert!(shape.column("id").unwrap().generated);
    assert!(shape.column("nombre").unwrap().not_null);
    assert_eq!(
        shape.column("activo").unwrap().default.as_deref(),
        Some("true")
    );
}

async fn cambia_columnas(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    table::apply(
        handle,
        &database,
        &[TableChange::AddColumn {
            schema: schema.to_owned(),
            table: "clientes".into(),
            column: plain("apodo", "text"),
        }],
    )
    .await
    .expect("tenía que agregar la columna");

    table::apply(
        handle,
        &database,
        &[
            TableChange::AlterColumnType {
                schema: schema.to_owned(),
                table: "clientes".into(),
                column: "apodo".into(),
                type_name: "varchar(100)".into(),
                using: None,
            },
            TableChange::SetColumnNotNull {
                schema: schema.to_owned(),
                table: "clientes".into(),
                column: "apodo".into(),
                not_null: true,
            },
            TableChange::SetColumnDefault {
                schema: schema.to_owned(),
                table: "clientes".into(),
                column: "apodo".into(),
                default: Some("'sin apodo'".into()),
            },
        ],
    )
    .await
    .expect("tenía que cambiar tipo, nulabilidad y default en un solo lote");

    let oid = oid_of(handle, schema, "clientes").await.unwrap();
    let shape = data::shape(handle, &database, oid).await.unwrap();
    let apodo = shape.column("apodo").unwrap();

    assert_eq!(apodo.type_name, "character varying(100)");
    assert!(apodo.not_null);
    assert!(
        apodo.default.as_deref().unwrap_or_default().contains("sin apodo"),
        "{:?}",
        apodo.default
    );
}

async fn renombra_tabla_y_columna(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    table::apply(
        handle,
        &database,
        &[
            TableChange::RenameColumn {
                schema: schema.to_owned(),
                table: "clientes".into(),
                column: "apodo".into(),
                new_name: "alias".into(),
            },
            TableChange::RenameTable {
                schema: schema.to_owned(),
                name: "clientes".into(),
                new_name: "personas".into(),
            },
        ],
    )
    .await
    .expect("tenía que renombrar la columna y la tabla en el mismo lote");

    assert!(oid_of(handle, schema, "clientes").await.is_none());
    let oid = oid_of(handle, schema, "personas")
        .await
        .expect("la tabla tiene que existir con el nombre nuevo");

    let shape = data::shape(handle, &database, oid).await.unwrap();
    assert!(shape.column("alias").is_some());
    assert!(shape.column("apodo").is_none());
}

/// Un tipo inventado en el segundo paso tiene que revertir también el primero, que por sí solo es
/// válido.
async fn un_lote_fallido_no_deja_nada(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    let result = table::apply(
        handle,
        &database,
        &[
            TableChange::RenameTable {
                schema: schema.to_owned(),
                name: "personas".into(),
                new_name: "gente".into(),
            },
            TableChange::AddColumn {
                schema: schema.to_owned(),
                table: "gente".into(),
                column: plain("nada", "tipo_que_no_existe"),
            },
        ],
    )
    .await;

    assert!(result.is_err(), "un tipo inexistente tiene que fallar");
    assert!(
        oid_of(handle, schema, "personas").await.is_some(),
        "el renombre del primer paso no se puede haber quedado aplicado"
    );
    assert!(oid_of(handle, schema, "gente").await.is_none());
}

async fn borra_columna_y_tabla(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    table::apply(
        handle,
        &database,
        &[TableChange::DropColumn {
            schema: schema.to_owned(),
            table: "personas".into(),
            column: "alias".into(),
            cascade: false,
        }],
    )
    .await
    .expect("tenía que borrar la columna");

    let oid = oid_of(handle, schema, "personas").await.unwrap();
    let shape = data::shape(handle, &database, oid).await.unwrap();
    assert!(shape.column("alias").is_none());

    table::apply(
        handle,
        &database,
        &[TableChange::DropTable {
            schema: schema.to_owned(),
            name: "personas".into(),
            cascade: false,
        }],
    )
    .await
    .expect("tenía que borrar la tabla");

    assert!(oid_of(handle, schema, "personas").await.is_none());
}
