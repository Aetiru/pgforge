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
use pgforge_core::ddl::role::{self, RoleAttributes, RoleChange};

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
