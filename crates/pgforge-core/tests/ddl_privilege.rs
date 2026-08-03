//! Privilegios contra servidores reales.
//!
//! Lo que se verifica acá no se puede verificar sin servidor: que `aclexplode` combinado con
//! `acldefault` sea la sintaxis correcta —incluidas las letras de cada tipo de objeto, que son
//! distintas en `acldefault` y en `pg_default_acl`— y que otorgar y revocar realmente cambien lo
//! que devuelve el catálogo.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::ddl::function;
use pgforge_core::ddl::privilege::{
    self, ColumnPrivilege, DatabasePrivilege, DefaultPrivileges, DefaultScope, FunctionPrivilege,
    Grantable, PrivilegeChange, SchemaPrivilege, SequencePrivilege, TablePrivilege,
};
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

/// El rol del test de reasignación. Va aparte del otro porque ese test lo deja sin nada y después
/// lo borra: mezclarlo con el rol que sostiene los privilegios ordenaría los tests entre sí.
fn owner_role_name() -> String {
    format!("pgforge_privilege_owner_{}", std::process::id())
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

/// `DROP OWNED` antes de `DROP ROLE` no es un lujo: un rol que quedó como destinatario de un
/// privilegio por omisión no se puede borrar, y el error no dice cuál es.
async fn teardown(handle: &ServerHandle, schema: &str, roles: &[&str]) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .await;
        for role in roles {
            let quoted = quote_ident(role);
            let _ = client
                .batch_execute(&format!("DROP OWNED BY {quoted} CASCADE"))
                .await;
            let _ = client
                .batch_execute(&format!("DROP ROLE IF EXISTS {quoted}"))
                .await;
        }
    }
}

async fn relation_oid(handle: &ServerHandle, schema: &str, name: &str) -> u32 {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(
            "SELECT c.oid FROM pg_class c
               JOIN pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = $1 AND c.relname = $2",
            &[&schema, &name],
        )
        .await
        .unwrap_or_else(|e| panic!("no se encontró {schema}.{name}: {e}"))
        .get(0)
}

async fn schema_oid(handle: &ServerHandle, schema: &str) -> u32 {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(
            "SELECT oid FROM pg_namespace WHERE nspname = $1",
            &[&schema],
        )
        .await
        .unwrap_or_else(|e| panic!("no se encontró el esquema {schema}: {e}"))
        .get(0)
}

async fn function_oid(handle: &ServerHandle, schema: &str, name: &str) -> u32 {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(
            "SELECT p.oid FROM pg_proc p
               JOIN pg_namespace n ON n.oid = p.pronamespace
              WHERE n.nspname = $1 AND p.proname = $2",
            &[&schema, &name],
        )
        .await
        .unwrap_or_else(|e| panic!("no se encontró la función {schema}.{name}: {e}"))
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
        let owner = owner_role_name();

        teardown(&handle, &schema, &[&role, &owner]).await; // por si quedó algo de una corrida anterior
        setup(&handle, &schema).await;

        let outcome = {
            let handle = Arc::clone(&handle);
            let schema = schema.clone();
            let role = role.clone();
            let owner = owner.clone();
            tokio::spawn(async move {
                prepara(&handle, &schema, &role).await;
                privilegios_de_tabla(&handle, &schema, &role).await;
                privilegios_de_esquema(&handle, &schema, &role).await;
                privilegios_de_secuencia(&handle, &schema, &role).await;
                privilegios_de_funcion(&handle, &schema, &role).await;
                privilegios_por_columna(&handle, &schema, &role).await;
                privilegios_de_base(&handle, &role).await;
                privilegios_por_omision(&handle, &schema, &role).await;
                privilegio_a_public(&handle, &schema).await;
                reasigna_lo_que_el_rol_posee(&handle, &schema, &owner).await;
            })
            .await
        };

        teardown(&handle, &schema, &[&role, &owner]).await;

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
            columns: vec![
                ColumnDef {
                    name: "id".into(),
                    type_name: "bigint".into(),
                    not_null: false,
                    default: None,
                    identity: None,
                },
                ColumnDef {
                    name: "nombre".into(),
                    type_name: "text".into(),
                    not_null: false,
                    default: None,
                    identity: None,
                },
            ],
        }],
    )
    .await
    .expect("tenía que crear la tabla");

    let client = handle.client(&database).await.unwrap();
    client
        .batch_execute(&format!(
            "CREATE SEQUENCE {schema}.numeros;
             CREATE FUNCTION {schema}.doble(n integer) RETURNS integer
                 LANGUAGE sql IMMUTABLE AS $$ SELECT n * 2 $$;"
        ))
        .await
        .expect("tenía que crear la secuencia y la función de prueba");

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

/// Otorga y revoca sobre `target`, y devuelve lo que quedó en el catálogo después de cada paso.
async fn grant(handle: &ServerHandle, target: Grantable, grantee: &str, option: bool) {
    let database = handle.default_database().to_owned();
    privilege::apply(
        handle,
        &database,
        &[PrivilegeChange::Grant {
            target,
            grantee: grantee.to_owned(),
            grant_option: option,
        }],
    )
    .await
    .expect("tenía que otorgar el privilegio");
}

async fn privilegios_de_tabla(handle: &ServerHandle, schema: &str, role_name: &str) {
    let database = handle.default_database().to_owned();
    let oid = relation_oid(handle, schema, "clientes").await;

    grant(
        handle,
        Grantable::Table {
            schema: schema.to_owned(),
            table: "clientes".into(),
            privileges: vec![TablePrivilege::Select, TablePrivilege::Insert],
        },
        role_name,
        true,
    )
    .await;

    let grants = privilege::relation_privileges(handle, &database, oid)
        .await
        .unwrap();
    let select = grants
        .iter()
        .find(|g| g.grantee == role_name && g.privilege == "SELECT")
        .expect("tenía que aparecer el SELECT otorgado");
    assert!(select.grantable, "se otorgó WITH GRANT OPTION");
    assert!(
        grants
            .iter()
            .any(|g| g.grantee == role_name && g.privilege == "INSERT"),
        "tenía que aparecer el INSERT otorgado"
    );

    privilege::apply(
        handle,
        &database,
        &[PrivilegeChange::Revoke {
            target: Grantable::Table {
                schema: schema.to_owned(),
                table: "clientes".into(),
                privileges: vec![TablePrivilege::Insert],
            },
            grantee: role_name.to_owned(),
            grant_option_only: false,
            cascade: false,
        }],
    )
    .await
    .expect("tenía que revocar solo el INSERT");

    let grants = privilege::relation_privileges(handle, &database, oid)
        .await
        .unwrap();
    assert!(
        grants
            .iter()
            .any(|g| g.grantee == role_name && g.privilege == "SELECT"),
        "el SELECT no se puede haber tocado"
    );
    assert!(
        !grants
            .iter()
            .any(|g| g.grantee == role_name && g.privilege == "INSERT"),
        "el INSERT revocado no puede seguir apareciendo"
    );
}

async fn privilegios_de_esquema(handle: &ServerHandle, schema: &str, role_name: &str) {
    let database = handle.default_database().to_owned();
    let oid = schema_oid(handle, schema).await;

    grant(
        handle,
        Grantable::Schema {
            schema: schema.to_owned(),
            privileges: vec![SchemaPrivilege::Usage],
        },
        role_name,
        false,
    )
    .await;

    let grants = privilege::schema_privileges(handle, &database, oid)
        .await
        .unwrap();
    assert!(grants
        .iter()
        .any(|g| g.grantee == role_name && g.privilege == "USAGE" && !g.grantable));
}

/// Una secuencia vive en `pg_class` igual que una tabla, pero su `acldefault` no: si se le pasara
/// la letra de una relación, el privilegio otorgado aparecería igual y el default implícito no.
async fn privilegios_de_secuencia(handle: &ServerHandle, schema: &str, role_name: &str) {
    let database = handle.default_database().to_owned();
    let oid = relation_oid(handle, schema, "numeros").await;

    grant(
        handle,
        Grantable::Sequence {
            schema: schema.to_owned(),
            sequence: "numeros".into(),
            privileges: vec![SequencePrivilege::Usage, SequencePrivilege::Select],
        },
        role_name,
        false,
    )
    .await;

    let grants = privilege::relation_privileges(handle, &database, oid)
        .await
        .unwrap();
    assert!(
        grants
            .iter()
            .any(|g| g.grantee == role_name && g.privilege == "USAGE"),
        "tenía que aparecer el USAGE de la secuencia: {grants:?}"
    );
    assert!(
        grants
            .iter()
            .any(|g| g.grantee == role_name && g.privilege == "SELECT"),
        "tenía que aparecer el SELECT de la secuencia: {grants:?}"
    );
}

async fn privilegios_de_funcion(handle: &ServerHandle, schema: &str, role_name: &str) {
    let database = handle.default_database().to_owned();
    let oid = function_oid(handle, schema, "doble").await;
    let args = function::identity_args(handle, &database, oid)
        .await
        .expect("tenía que devolver los argumentos de la función");

    grant(
        handle,
        Grantable::Function {
            schema: schema.to_owned(),
            name: "doble".into(),
            args,
            procedure: false,
            privileges: vec![FunctionPrivilege::Execute],
        },
        role_name,
        false,
    )
    .await;

    let grants = privilege::function_privileges(handle, &database, oid)
        .await
        .unwrap();
    assert!(
        grants
            .iter()
            .any(|g| g.grantee == role_name && g.privilege == "EXECUTE"),
        "tenía que aparecer el EXECUTE otorgado: {grants:?}"
    );
}

async fn privilegios_por_columna(handle: &ServerHandle, schema: &str, role_name: &str) {
    let database = handle.default_database().to_owned();
    let oid = relation_oid(handle, schema, "clientes").await;

    grant(
        handle,
        Grantable::Columns {
            schema: schema.to_owned(),
            table: "clientes".into(),
            columns: vec!["nombre".into()],
            privileges: vec![ColumnPrivilege::Update],
        },
        role_name,
        false,
    )
    .await;

    let grants = privilege::column_privileges(handle, &database, oid)
        .await
        .unwrap();
    assert!(
        grants
            .iter()
            .any(|g| g.column == "nombre" && g.grantee == role_name && g.privilege == "UPDATE"),
        "tenía que aparecer el UPDATE de la columna: {grants:?}"
    );
    assert!(
        !grants.iter().any(|g| g.column == "id"),
        "una columna sin ACL propio no tiene que aparecer: {grants:?}"
    );
}

async fn privilegios_de_base(handle: &ServerHandle, role_name: &str) {
    let database = handle.default_database().to_owned();

    grant(
        handle,
        Grantable::Database {
            database: database.clone(),
            privileges: vec![DatabasePrivilege::Connect],
        },
        role_name,
        false,
    )
    .await;

    let grants = privilege::database_privileges(handle, &database, &database)
        .await
        .unwrap();
    assert!(
        grants
            .iter()
            .any(|g| g.grantee == role_name && g.privilege == "CONNECT"),
        "tenía que aparecer el CONNECT otorgado: {grants:?}"
    );
}

/// Lo único que prueba que un `ALTER DEFAULT PRIVILEGES` sirvió es una tabla creada **después**:
/// que la sentencia no falle no dice nada sobre si va a aplicarse.
async fn privilegios_por_omision(handle: &ServerHandle, schema: &str, role_name: &str) {
    let database = handle.default_database().to_owned();

    privilege::apply(
        handle,
        &database,
        &[PrivilegeChange::GrantDefault {
            scope: DefaultScope {
                role: None,
                schema: Some(schema.to_owned()),
            },
            target: DefaultPrivileges::Tables {
                privileges: vec![TablePrivilege::Select],
            },
            grantee: role_name.to_owned(),
            grant_option: false,
        }],
    )
    .await
    .expect("tenía que definir los privilegios por omisión");

    let defaults = privilege::default_privileges(handle, &database)
        .await
        .unwrap();
    assert!(
        defaults.iter().any(|d| d.grantee == role_name
            && d.privilege == "SELECT"
            && d.objects == "tables"
            && d.schema.as_deref() == Some(schema)),
        "el privilegio por omisión tenía que quedar en pg_default_acl: {defaults:?}"
    );

    table::apply(
        handle,
        &database,
        &[TableChange::CreateTable {
            schema: schema.to_owned(),
            name: "posterior".into(),
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
    .expect("tenía que crear la tabla posterior");

    let oid = relation_oid(handle, schema, "posterior").await;
    let grants = privilege::relation_privileges(handle, &database, oid)
        .await
        .unwrap();
    assert!(
        grants
            .iter()
            .any(|g| g.grantee == role_name && g.privilege == "SELECT"),
        "la tabla creada después tenía que nacer con el SELECT puesto: {grants:?}"
    );
}

async fn privilegio_a_public(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();
    let oid = relation_oid(handle, schema, "clientes").await;

    grant(
        handle,
        Grantable::Table {
            schema: schema.to_owned(),
            table: "clientes".into(),
            privileges: vec![TablePrivilege::Select],
        },
        "PUBLIC",
        false,
    )
    .await;

    let grants = privilege::relation_privileges(handle, &database, oid)
        .await
        .unwrap();
    assert!(
        grants
            .iter()
            .any(|g| g.grantee == "PUBLIC" && g.privilege == "SELECT"),
        "PUBLIC tiene que aparecer tal cual, no como un rol citado: {grants:?}"
    );
}

/// Un rol dueño de algo no se puede borrar, y el error del servidor no dice qué hacer. Esto
/// verifica la salida que ofrece la interfaz: reasignar lo que posee y soltar lo que le otorgaron.
async fn reasigna_lo_que_el_rol_posee(handle: &ServerHandle, schema: &str, owner: &str) {
    let database = handle.default_database().to_owned();

    role::apply(
        handle,
        &database,
        &[RoleChange::CreateRole {
            name: owner.to_owned(),
            attributes: RoleAttributes::default(),
            member_of: vec![],
        }],
    )
    .await
    .expect("tenía que crear el rol dueño");

    let client = handle.client(&database).await.unwrap();
    client
        .batch_execute(&format!(
            "CREATE TABLE {schema}.suya (id bigint);
             ALTER TABLE {schema}.suya OWNER TO {};",
            quote_ident(owner)
        ))
        .await
        .expect("tenía que dejar la tabla a nombre del rol");

    let drop_role = [RoleChange::DropRole {
        name: owner.to_owned(),
    }];
    assert!(
        role::apply(handle, &database, &drop_role).await.is_err(),
        "un rol dueño de una tabla no se tiene que poder borrar"
    );

    role::apply(
        handle,
        &database,
        &[
            RoleChange::ReassignOwned {
                from: owner.to_owned(),
                to: "CURRENT_USER".into(),
            },
            RoleChange::DropOwned {
                role: owner.to_owned(),
                cascade: false,
            },
        ],
    )
    .await
    .expect("tenía que reasignar y soltar lo que el rol tenía");

    role::apply(handle, &database, &drop_role)
        .await
        .expect("después de reasignar, el rol tiene que poder borrarse");
}
