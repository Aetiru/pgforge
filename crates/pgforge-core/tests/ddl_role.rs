//! Roles contra servidores reales.
//!
//! Lo que se verifica acá no se puede verificar sin servidor: que `ALTER ROLE` solo cambie el
//! atributo que se le pidió y deje el resto intacto, y que la membresía se pueda otorgar y revocar
//! de verdad.
//!
//! A diferencia de todos los demás tests de `ddl_*.rs`, acá no hay un esquema descartable: los
//! roles son del clúster entero, así que la limpieza es por nombre.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::ddl::quote_ident;
use pgforge_core::ddl::role::{self, AccessLevel, RoleAttributes, RoleChange};

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

fn role_name(suffix: &str) -> String {
    format!("pgforge_role_{}_{suffix}", std::process::id())
}

async fn drop_if_exists(handle: &ServerHandle, name: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!("DROP ROLE IF EXISTS {}", quote_ident(name)))
            .await;
    }
}

async fn oid_of(handle: &ServerHandle, name: &str) -> Option<u32> {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_opt(
            "SELECT oid FROM pg_catalog.pg_roles WHERE rolname = $1",
            &[&name],
        )
        .await
        .unwrap()
        .map(|row| row.get(0))
}

#[tokio::test]
async fn crea_altera_agrupa_y_borra_roles_contra_servidores_reales() {
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

        let uno = role_name("uno");
        let renombrado = role_name("uno_renombrado");
        let dos = role_name("dos");

        // Por si quedó algo de una corrida anterior que se cortó a la mitad.
        drop_if_exists(&handle, &uno).await;
        drop_if_exists(&handle, &renombrado).await;
        drop_if_exists(&handle, &dos).await;

        let outcome = {
            let handle = Arc::clone(&handle);
            let uno = uno.clone();
            let renombrado = renombrado.clone();
            let dos = dos.clone();
            tokio::spawn(async move {
                crea_y_verifica(&handle, &uno).await;
                altera_solo_lo_pedido(&handle, &uno).await;
                renombra(&handle, &uno, &renombrado).await;
                agrupa_y_desagrupa(&handle, &renombrado, &dos).await;
                borra(&handle, &renombrado, &dos).await;
            })
            .await
        };

        drop_if_exists(&handle, &uno).await;
        drop_if_exists(&handle, &renombrado).await;
        drop_if_exists(&handle, &dos).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({url})");
    }
}

async fn crea_y_verifica(handle: &ServerHandle, name: &str) {
    let database = handle.default_database().to_owned();

    role::apply(
        handle,
        &database,
        &[RoleChange::CreateRole {
            name: name.to_owned(),
            attributes: RoleAttributes {
                login: Some(true),
                connection_limit: Some(5),
                valid_until: Some("infinity".into()),
                ..Default::default()
            },
            member_of: vec![],
        }],
    )
    .await
    .expect("tenía que crear el rol");

    let oid = oid_of(handle, name)
        .await
        .expect("el rol tiene que existir");
    let info = role::role(handle, &database, oid).await.unwrap();
    assert!(info.login);
    assert!(!info.superuser);
    assert_eq!(info.connection_limit, 5);
    assert_eq!(info.valid_until.as_deref(), Some("infinity"));
}

async fn altera_solo_lo_pedido(handle: &ServerHandle, name: &str) {
    let database = handle.default_database().to_owned();

    role::apply(
        handle,
        &database,
        &[RoleChange::AlterRole {
            name: name.to_owned(),
            attributes: RoleAttributes {
                superuser: Some(true),
                ..Default::default()
            },
        }],
    )
    .await
    .expect("tenía que alterar el rol");

    let oid = oid_of(handle, name).await.unwrap();
    let info = role::role(handle, &database, oid).await.unwrap();
    assert!(info.superuser, "tenía que aplicar el cambio pedido");
    assert!(info.login, "un atributo no pedido no se puede haber tocado");
    assert_eq!(
        info.connection_limit, 5,
        "un atributo no pedido no se puede haber tocado"
    );
}

async fn renombra(handle: &ServerHandle, name: &str, new_name: &str) {
    let database = handle.default_database().to_owned();

    role::apply(
        handle,
        &database,
        &[RoleChange::RenameRole {
            name: name.to_owned(),
            new_name: new_name.to_owned(),
        }],
    )
    .await
    .expect("tenía que renombrar el rol");

    assert!(oid_of(handle, name).await.is_none());
    assert!(oid_of(handle, new_name).await.is_some());
}

async fn agrupa_y_desagrupa(handle: &ServerHandle, role_name: &str, member_name: &str) {
    let database = handle.default_database().to_owned();

    role::apply(
        handle,
        &database,
        &[RoleChange::CreateRole {
            name: member_name.to_owned(),
            attributes: RoleAttributes::default(),
            member_of: vec![],
        }],
    )
    .await
    .expect("tenía que crear el segundo rol");

    role::apply(
        handle,
        &database,
        &[RoleChange::GrantMembership {
            role: role_name.to_owned(),
            member: member_name.to_owned(),
            admin_option: false,
        }],
    )
    .await
    .expect("tenía que otorgar la membresía");

    let memberships = role::role_memberships(handle, &database, member_name)
        .await
        .unwrap();
    assert!(
        memberships.iter().any(|m| m == role_name),
        "{member_name} tenía que quedar como miembro de {role_name}: {memberships:?}"
    );

    role::apply(
        handle,
        &database,
        &[RoleChange::RevokeMembership {
            role: role_name.to_owned(),
            member: member_name.to_owned(),
        }],
    )
    .await
    .expect("tenía que revocar la membresía");

    let memberships = role::role_memberships(handle, &database, member_name)
        .await
        .unwrap();
    assert!(!memberships.iter().any(|m| m == role_name));
}

async fn borra(handle: &ServerHandle, uno: &str, dos: &str) {
    let database = handle.default_database().to_owned();

    role::apply(
        handle,
        &database,
        &[
            RoleChange::DropRole {
                name: uno.to_owned(),
            },
            RoleChange::DropRole {
                name: dos.to_owned(),
            },
        ],
    )
    .await
    .expect("tenía que borrar los dos roles");

    assert!(oid_of(handle, uno).await.is_none());
    assert!(oid_of(handle, dos).await.is_none());
}

/// Una plantilla de acceso da exactamente los permisos pedidos, y solo esos. Se verifica con las
/// funciones `has_*_privilege` del servidor y no leyendo las ACL: es la respuesta que daría
/// PostgreSQL a quien después use ese rol.
#[tokio::test]
async fn la_plantilla_de_acceso_da_los_permisos_pedidos_contra_servidores_reales() {
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
        let lectora = role_name("lectora");
        let escritora = role_name("escritora");
        let schema = format!("pgforge_acc_{}", std::process::id());
        let database = handle.default_database().to_owned();

        limpia(&handle, &database, &schema, &[&lectora, &escritora]).await;

        let outcome = {
            let handle = Arc::clone(&handle);
            let (lectora, escritora, schema, database) = (
                lectora.clone(),
                escritora.clone(),
                schema.clone(),
                database.clone(),
            );
            tokio::spawn(async move {
                let client = handle.client(&database).await.unwrap();
                client
                    .batch_execute(&format!(
                        "CREATE SCHEMA {schema};
                         CREATE TABLE {schema}.pedidos (id serial PRIMARY KEY, total numeric);"
                    ))
                    .await
                    .unwrap();

                for (name, level) in [(&lectora, AccessLevel::Read), (&escritora, AccessLevel::Write)] {
                    role::apply(
                        &handle,
                        &database,
                        &[
                            RoleChange::CreateRole {
                                name: name.clone(),
                                attributes: RoleAttributes {
                                    login: Some(true),
                                    ..Default::default()
                                },
                                member_of: vec![],
                            },
                            RoleChange::GrantSchemaAccess {
                                role: name.clone(),
                                schema: schema.clone(),
                                level,
                                future_tables: true,
                            },
                        ],
                    )
                    .await
                    .expect("tenía que crear el rol con su plantilla");
                }

                // Las opciones del selector traen la bandera de login y los grupos van antes.
                let options = role::role_options(&handle, &database).await.unwrap();
                assert!(options.iter().any(|o| o.name == lectora && o.login));
                let first_login = options.iter().position(|o| o.login).unwrap_or(options.len());
                assert!(
                    options[first_login..].iter().all(|o| o.login),
                    "los roles sin login tienen que ir todos antes que los que sí inician sesión"
                );
                assert!(options.iter().all(|o| !o.name.starts_with("pg_")));

                let can = |sql: String| {
                    let client = &client;
                    async move { client.query_one(&sql, &[]).await.unwrap().get::<_, bool>(0) }
                };
                let table = |role: &str, privilege: &str, table: &str| {
                    format!(
                        "SELECT has_table_privilege('{role}', '{schema}.{table}', '{privilege}')"
                    )
                };

                // Solo lectura: ve el esquema y lee, pero no escribe ni usa la secuencia.
                assert!(can(format!("SELECT has_schema_privilege('{lectora}', '{schema}', 'USAGE')")).await);
                assert!(can(table(&lectora, "SELECT", "pedidos")).await);
                assert!(!can(table(&lectora, "INSERT", "pedidos")).await);
                assert!(!can(format!(
                    "SELECT has_sequence_privilege('{lectora}', '{schema}.pedidos_id_seq', 'USAGE')"
                ))
                .await);
                assert!(!can(format!("SELECT has_schema_privilege('{lectora}', '{schema}', 'CREATE')")).await);

                // Lectura y escritura: modifica y usa la secuencia (sin eso un INSERT con `serial` falla).
                assert!(can(table(&escritora, "INSERT", "pedidos")).await);
                assert!(can(table(&escritora, "DELETE", "pedidos")).await);
                assert!(can(format!(
                    "SELECT has_sequence_privilege('{escritora}', '{schema}.pedidos_id_seq', 'USAGE')"
                ))
                .await);
                assert!(!can(format!("SELECT has_schema_privilege('{escritora}', '{schema}', 'CREATE')")).await);

                // Las tablas que se crean después también quedan cubiertas (lo creó este mismo rol).
                client
                    .batch_execute(&format!("CREATE TABLE {schema}.nueva (id int)"))
                    .await
                    .unwrap();
                assert!(can(table(&lectora, "SELECT", "nueva")).await);
                assert!(!can(table(&lectora, "INSERT", "nueva")).await);
                assert!(can(table(&escritora, "UPDATE", "nueva")).await);
            })
            .await
        };

        limpia(&handle, &database, &schema, &[&lectora, &escritora]).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({url})");
    }
}

/// `DROP OWNED` antes de borrar los roles: los `ALTER DEFAULT PRIVILEGES` los dejan enganchados.
async fn limpia(handle: &ServerHandle, database: &str, schema: &str, roles: &[&String]) {
    let Ok(client) = handle.client(database).await else {
        return;
    };
    for role in roles {
        let quoted = quote_ident(role);
        let exists: bool = client
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = $1)",
                &[role],
            )
            .await
            .map(|row| row.get(0))
            .unwrap_or(false);
        if exists {
            let _ = client
                .batch_execute(&format!("DROP OWNED BY {quoted}; DROP ROLE {quoted}"))
                .await;
        }
    }
    let _ = client
        .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
        .await;
}
