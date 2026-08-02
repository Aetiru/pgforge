//! Privilegios sobre tablas y esquemas contra servidores reales.
//!
//! Lo que se verifica acá no se puede verificar sin servidor: que `aclexplode` combinado con
//! `acldefault` sea la sintaxis correcta (es la única parte de este recorte que no se puede probar
//! sin conectarse a Postgres), y que otorgar/revocar realmente cambie lo que devuelve.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::ddl::privilege::{self, PrivilegeChange, SchemaPrivilege, TablePrivilege};
use pgforge_core::ddl::quote_ident;
use pgforge_core::ddl::role::{self, RoleAttributes, RoleChange};
use pgforge_core::ddl::table::{self, ColumnDef, TableChange};

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
    format!("pgforge_privilege_{}", std::process::id())
}

fn role_name() -> String {
    format!("pgforge_privilege_role_{}", std::process::id())
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

async fn teardown(handle: &ServerHandle, schema: &str, role: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .await;
        let _ = client
            .batch_execute(&format!("DROP ROLE IF EXISTS {}", quote_ident(role)))
            .await;
    }
}

async fn table_oid(handle: &ServerHandle, schema: &str, table: &str) -> u32 {
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

async fn schema_oid(handle: &ServerHandle, schema: &str) -> u32 {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one("SELECT oid FROM pg_namespace WHERE nspname = $1", &[&schema])
        .await
        .unwrap_or_else(|e| panic!("no se encontró el esquema {schema}: {e}"))
        .get(0)
}

#[tokio::test]
async fn otorga_y_revoca_privilegios_contra_servidores_reales() {
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
        let role = role_name();

        setup(&handle, &schema).await;
        teardown(&handle, &schema, &role).await; // por si quedó algo de una corrida anterior
        setup(&handle, &schema).await;

        let outcome = {
            let handle = Arc::clone(&handle);
            let schema = schema.clone();
            let role = role.clone();
            tokio::spawn(async move {
                prepara(&handle, &schema, &role).await;
                privilegios_de_tabla(&handle, &schema, &role).await;
                privilegios_de_esquema(&handle, &schema, &role).await;
                privilegio_a_public(&handle, &schema).await;
            })
            .await
        };

        teardown(&handle, &schema, &role).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({url})");
    }
}

async fn prepara(handle: &ServerHandle, schema: &str, role_name: &str) {
    let database = handle.default_database().to_owned();

    table::apply(
        handle,
        &database,
        &[TableChange::CreateTable {
            schema: schema.to_owned(),
            name: "clientes".into(),
            columns: vec![ColumnDef {
                name: "id".into(),
                type_name: "bigint".into(),
                not_null: false,
                default: None,
                identity: None,
            }],
        }],
    )
    .await
    .expect("tenía que crear la tabla");

    role::apply(
        handle,
        &database,
        &[RoleChange::CreateRole {
            name: role_name.to_owned(),
            attributes: RoleAttributes::default(),
            member_of: vec![],
        }],
    )
    .await
    .expect("tenía que crear el rol de prueba");
}

async fn privilegios_de_tabla(handle: &ServerHandle, schema: &str, role_name: &str) {
    let database = handle.default_database().to_owned();
    let oid = table_oid(handle, schema, "clientes").await;

    privilege::apply(
        handle,
        &database,
        &[PrivilegeChange::GrantTable {
            schema: schema.to_owned(),
            table: "clientes".into(),
            privileges: vec![TablePrivilege::Select, TablePrivilege::Insert],
            grantee: role_name.to_owned(),
            grant_option: true,
        }],
    )
    .await
    .expect("tenía que otorgar SELECT e INSERT");

    let grants = privilege::table_privileges(handle, &database, oid).await.unwrap();
    let select = grants
        .iter()
        .find(|g| g.grantee == role_name && g.privilege == "SELECT")
        .expect("tenía que aparecer el SELECT otorgado");
    assert!(select.grantable, "se otorgó WITH GRANT OPTION");
    assert!(
        grants.iter().any(|g| g.grantee == role_name && g.privilege == "INSERT"),
        "tenía que aparecer el INSERT otorgado"
    );

    privilege::apply(
        handle,
        &database,
        &[PrivilegeChange::RevokeTable {
            schema: schema.to_owned(),
            table: "clientes".into(),
            privileges: vec![TablePrivilege::Insert],
            grantee: role_name.to_owned(),
            grant_option_only: false,
            cascade: false,
        }],
    )
    .await
    .expect("tenía que revocar solo el INSERT");

    let grants = privilege::table_privileges(handle, &database, oid).await.unwrap();
    assert!(
        grants.iter().any(|g| g.grantee == role_name && g.privilege == "SELECT"),
        "el SELECT no se puede haber tocado"
    );
    assert!(
        !grants.iter().any(|g| g.grantee == role_name && g.privilege == "INSERT"),
        "el INSERT revocado no puede seguir apareciendo"
    );
}

async fn privilegios_de_esquema(handle: &ServerHandle, schema: &str, role_name: &str) {
    let database = handle.default_database().to_owned();
    let oid = schema_oid(handle, schema).await;

    privilege::apply(
        handle,
        &database,
        &[PrivilegeChange::GrantSchema {
            schema: schema.to_owned(),
            privileges: vec![SchemaPrivilege::Usage],
            grantee: role_name.to_owned(),
            grant_option: false,
        }],
    )
    .await
    .expect("tenía que otorgar USAGE sobre el esquema");

    let grants = privilege::schema_privileges(handle, &database, oid).await.unwrap();
    assert!(grants
        .iter()
        .any(|g| g.grantee == role_name && g.privilege == "USAGE" && !g.grantable));
}

async fn privilegio_a_public(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();
    let oid = table_oid(handle, schema, "clientes").await;

    privilege::apply(
        handle,
        &database,
        &[PrivilegeChange::GrantTable {
            schema: schema.to_owned(),
            table: "clientes".into(),
            privileges: vec![TablePrivilege::Select],
            grantee: "PUBLIC".into(),
            grant_option: false,
        }],
    )
    .await
    .expect("tenía que otorgar SELECT a PUBLIC");

    let grants = privilege::table_privileges(handle, &database, oid).await.unwrap();
    assert!(
        grants.iter().any(|g| g.grantee == "PUBLIC" && g.privilege == "SELECT"),
        "PUBLIC tiene que aparecer tal cual, no como un rol citado: {grants:?}"
    );
}
