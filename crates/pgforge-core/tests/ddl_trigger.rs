//! Triggers contra servidores reales.
//!
//! Lo que se verifica acá no se puede verificar sin servidor: que el trigger se dispare de verdad
//! con el evento que se le pidió, y que reemplazarlo (borrar + crear, ya que no hay
//! `CREATE OR REPLACE TRIGGER` en el piso soportado) deje solo el comportamiento nuevo.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::ddl::function;
use pgforge_core::ddl::table::{self, ColumnDef, Identity, TableChange};
use pgforge_core::ddl::trigger::{self, Event, Level, Timing, TriggerChange, TriggerDef};

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
    format!("pgforge_trigger_{}", std::process::id())
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

async fn oid_of(handle: &ServerHandle, schema: &str, table: &str) -> u32 {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(
            "SELECT c.oid FROM pg_class c
               JOIN pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = $1 AND c.relname = $2",
            &[&schema, &table],
        )
        .await
        .unwrap_or_else(|e| panic!("no se encontró {schema}.{table}: {e}"))
        .get(0)
}

async fn bitacora_count(handle: &ServerHandle, schema: &str) -> i64 {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(&format!("SELECT count(*) FROM {schema}.bitacora"), &[])
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
async fn crea_reemplaza_y_borra_triggers_contra_servidores_reales() {
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
                prepara_tablas_y_funcion(&handle, &schema).await;
                crea_y_dispara(&handle, &schema).await;
                reemplaza_y_dispara_distinto(&handle, &schema).await;
                borra_y_deja_de_dispararse(&handle, &schema).await;
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

async fn prepara_tablas_y_funcion(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    table::apply(
        handle,
        &database,
        &[
            TableChange::CreateTable {
                schema: schema.to_owned(),
                name: "clientes".into(),
                columns: vec![
                    ColumnDef {
                        identity: Some(Identity::Always),
                        ..plain("id", "bigint")
                    },
                    plain("nombre", "text"),
                ],
            },
            TableChange::CreateTable {
                schema: schema.to_owned(),
                name: "bitacora".into(),
                columns: vec![plain("mensaje", "text")],
            },
        ],
    )
    .await
    .expect("tenía que crear las tablas");

    function::apply(
        handle,
        &database,
        &format!(
            "CREATE FUNCTION {schema}.registrar() RETURNS trigger LANGUAGE plpgsql AS $$ \
             BEGIN INSERT INTO {schema}.bitacora (mensaje) VALUES (TG_OP); RETURN NEW; END; $$"
        ),
    )
    .await
    .expect("tenía que crear la función del trigger");
}

async fn crea_y_dispara(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    trigger::apply(
        handle,
        &database,
        &[TriggerChange::CreateTrigger {
            schema: schema.to_owned(),
            table: "clientes".into(),
            name: "clientes_bi".into(),
            definition: TriggerDef {
                timing: Timing::Before,
                events: vec![Event::Insert],
                level: Level::Row,
                when: None,
                function_schema: schema.to_owned(),
                function_name: "registrar".into(),
            },
        }],
    )
    .await
    .expect("tenía que crear el trigger");

    let client = handle.client(&database).await.unwrap();
    client
        .batch_execute(&format!("INSERT INTO {schema}.clientes (nombre) VALUES ('ana')"))
        .await
        .expect("el insert tenía que disparar el trigger sin fallar");

    assert_eq!(bitacora_count(handle, schema).await, 1);
}

async fn reemplaza_y_dispara_distinto(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    // Sin CREATE OR REPLACE TRIGGER: se borra el de INSERT y se crea uno de UPDATE, en el mismo
    // lote, con otro nombre.
    trigger::apply(
        handle,
        &database,
        &[
            TriggerChange::DropTrigger {
                schema: schema.to_owned(),
                table: "clientes".into(),
                name: "clientes_bi".into(),
                cascade: false,
            },
            TriggerChange::CreateTrigger {
                schema: schema.to_owned(),
                table: "clientes".into(),
                name: "clientes_au".into(),
                definition: TriggerDef {
                    timing: Timing::After,
                    events: vec![Event::Update],
                    level: Level::Row,
                    when: None,
                    function_schema: schema.to_owned(),
                    function_name: "registrar".into(),
                },
            },
        ],
    )
    .await
    .expect("tenía que reemplazar el trigger");

    let oid = oid_of(handle, schema, "clientes").await;
    let listados = trigger::triggers(handle, &database, oid).await.unwrap();
    assert_eq!(listados.len(), 1);
    assert_eq!(listados[0].name, "clientes_au");
    assert_eq!(listados[0].timing, Timing::After);
    assert_eq!(listados[0].events, vec![Event::Update]);

    let client = handle.client(&database).await.unwrap();

    // El de INSERT ya no existe: insertar no tiene que sumar nada a la bitácora.
    let antes = bitacora_count(handle, schema).await;
    client
        .batch_execute(&format!("INSERT INTO {schema}.clientes (nombre) VALUES ('beto')"))
        .await
        .unwrap();
    assert_eq!(bitacora_count(handle, schema).await, antes, "ya no tiene que disparar por INSERT");

    // El de UPDATE es nuevo: actualizar sí tiene que disparar.
    client
        .batch_execute(&format!("UPDATE {schema}.clientes SET nombre = 'beto2' WHERE nombre = 'beto'"))
        .await
        .unwrap();
    assert_eq!(bitacora_count(handle, schema).await, antes + 1, "tenía que disparar por UPDATE");
}

async fn borra_y_deja_de_dispararse(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    trigger::apply(
        handle,
        &database,
        &[TriggerChange::DropTrigger {
            schema: schema.to_owned(),
            table: "clientes".into(),
            name: "clientes_au".into(),
            cascade: false,
        }],
    )
    .await
    .expect("tenía que borrar el trigger");

    let oid = oid_of(handle, schema, "clientes").await;
    assert!(trigger::triggers(handle, &database, oid).await.unwrap().is_empty());

    let antes = bitacora_count(handle, schema).await;
    let client = handle.client(&database).await.unwrap();
    client
        .batch_execute(&format!("UPDATE {schema}.clientes SET nombre = 'beto3'"))
        .await
        .unwrap();
    assert_eq!(bitacora_count(handle, schema).await, antes, "sin trigger no tiene que disparar nada");
}
