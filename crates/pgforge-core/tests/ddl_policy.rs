//! Row-Level Security contra servidores reales.
//!
//! Lo que se verifica acá no se puede verificar sin servidor: que el filtro filtre de verdad. Todo
//! lo que importa de RLS es qué filas ve un rol que no es el dueño, y eso no se deduce leyendo el
//! SQL generado.
//!
//! Las lecturas se hacen con `SET LOCAL ROLE` dentro de una transacción que después se descarta.
//! Tiene que ser `LOCAL`: las conexiones salen de un pool que no reinicia la sesión al devolverlas,
//! así que un `SET ROLE` suelto se quedaría pegado a la conexión y contaminaría la consulta
//! siguiente. Y tiene que ser otro rol: un superusuario se saltea RLS siempre, así que un test que
//! mirara como `postgres` no probaría nada.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::ddl::policy::{self, Command, PolicyChange, PolicyDef, PolicyKind};
use pgforge_core::ddl::table::{self, ColumnDef, Identity, TableChange};

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
    format!("pgforge_policy_{}", std::process::id())
}

/// El rol con el que se mira la tabla. No puede ser superusuario ni el dueño (al principio), que
/// son justamente los dos que se saltean el filtro.
fn role_name() -> String {
    format!("pgforge_rls_{}", std::process::id())
}

async fn setup(handle: &ServerHandle, schema: &str, role: &str) {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE;
             DROP ROLE IF EXISTS {role};
             CREATE SCHEMA {schema};
             CREATE ROLE {role} LOGIN;"
        ))
        .await
        .expect("no se pudo preparar el esquema y el rol de prueba");
}

/// El orden importa: el rol termina siendo dueño de la tabla, y un rol con objetos propios no se
/// puede borrar. Primero se va el esquema con todo adentro, después el rol.
async fn teardown(handle: &ServerHandle, schema: &str, role: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .await;
        let _ = client
            .batch_execute(&format!("DROP ROLE IF EXISTS {role}"))
            .await;
    }
}

/// Cuántas filas ve `role`. La transacción se descarta siempre, así que el `SET LOCAL ROLE` no
/// sobrevive a la conexión devuelta al pool.
async fn visibles(handle: &ServerHandle, schema: &str, role: &str) -> i64 {
    let database = handle.default_database().to_owned();
    let mut client = handle.client(&database).await.unwrap();
    let transaction = client.transaction().await.unwrap();

    transaction
        .batch_execute(&format!("SET LOCAL ROLE {role}"))
        .await
        .expect("no se pudo cambiar de rol");

    let count: i64 = transaction
        .query_one(&format!("SELECT count(*) FROM {schema}.documentos"), &[])
        .await
        .expect("la consulta filtrada tenía que funcionar")
        .get(0);

    transaction.rollback().await.unwrap();
    count
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
async fn el_filtro_por_fila_filtra_contra_servidores_reales() {
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
        setup(&handle, &schema, &role).await;

        let outcome = {
            let handle = Arc::clone(&handle);
            let schema = schema.clone();
            let role = role.clone();
            tokio::spawn(async move {
                prepara_la_tabla(&handle, &schema, &role).await;
                el_filtro_sin_politicas_no_deja_pasar_nada(&handle, &schema, &role).await;
                una_politica_deja_ver_lo_propio(&handle, &schema, &role).await;
                una_restrictiva_recorta_lo_que_la_permisiva_dejo(&handle, &schema, &role).await;
                el_dueno_se_saltea_el_filtro_salvo_que_se_fuerce(&handle, &schema, &role).await;
                lo_leido_coincide_con_lo_creado(&handle, &schema, &role).await;
                el_ddl_reconstruido_vuelve_a_crear_la_misma_politica(&handle, &schema, &role).await;
                apagar_el_filtro_devuelve_todo(&handle, &schema, &role).await;
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

async fn prepara_la_tabla(handle: &ServerHandle, schema: &str, role: &str) {
    let database = handle.default_database().to_owned();

    table::apply(
        handle,
        &database,
        &[TableChange::CreateTable {
            schema: schema.to_owned(),
            name: "documentos".into(),
            columns: vec![
                ColumnDef {
                    identity: Some(Identity::Always),
                    ..plain("id", "bigint")
                },
                plain("dueno", "text"),
                plain("texto", "text"),
            ],
        }],
    )
    .await
    .expect("tenía que crear la tabla");

    let client = handle.client(&database).await.unwrap();
    client
        .batch_execute(&format!(
            "INSERT INTO {schema}.documentos (dueno, texto) VALUES
                 ('{role}', 'visible'),
                 ('{role}', 'oculto'),
                 ('otro',   'ajeno');
             GRANT USAGE, CREATE ON SCHEMA {schema} TO {role};
             GRANT SELECT ON {schema}.documentos TO {role};"
        ))
        .await
        .expect("tenía que cargar las filas y los privilegios");

    // Sin RLS todavía: el rol ve las tres. Es la línea de base contra la que se comparan las demás.
    assert_eq!(visibles(handle, schema, role).await, 3);
}

async fn el_filtro_sin_politicas_no_deja_pasar_nada(
    handle: &ServerHandle,
    schema: &str,
    role: &str,
) {
    let database = handle.default_database().to_owned();

    policy::apply(
        handle,
        &database,
        &[PolicyChange::SetRowSecurity {
            schema: schema.to_owned(),
            table: "documentos".into(),
            enabled: true,
        }],
    )
    .await
    .expect("tenía que prender el filtro");

    assert_eq!(
        visibles(handle, schema, role).await,
        0,
        "con el filtro prendido y sin políticas no tiene que pasar ninguna fila"
    );
}

async fn una_politica_deja_ver_lo_propio(handle: &ServerHandle, schema: &str, role: &str) {
    let database = handle.default_database().to_owned();

    policy::apply(
        handle,
        &database,
        &[PolicyChange::CreatePolicy {
            schema: schema.to_owned(),
            table: "documentos".into(),
            name: "solo_los_propios".into(),
            definition: PolicyDef {
                command: Command::All,
                kind: PolicyKind::Permissive,
                roles: vec![role.to_owned()],
                using: Some("dueno = current_user".into()),
                check: None,
            },
        }],
    )
    .await
    .expect("tenía que crear la política");

    assert_eq!(
        visibles(handle, schema, role).await,
        2,
        "tenía que ver solo las filas cuyo dueño es él"
    );
}

async fn una_restrictiva_recorta_lo_que_la_permisiva_dejo(
    handle: &ServerHandle,
    schema: &str,
    role: &str,
) {
    let database = handle.default_database().to_owned();

    policy::apply(
        handle,
        &database,
        &[PolicyChange::CreatePolicy {
            schema: schema.to_owned(),
            table: "documentos".into(),
            name: "nada_oculto".into(),
            definition: PolicyDef {
                command: Command::Select,
                kind: PolicyKind::Restrictive,
                roles: vec![role.to_owned()],
                using: Some("texto <> 'oculto'".into()),
                check: None,
            },
        }],
    )
    .await
    .expect("tenía que crear la política restrictiva");

    assert_eq!(
        visibles(handle, schema, role).await,
        1,
        "la restrictiva se combina con AND: tenía que recortar una de las dos"
    );

    policy::apply(
        handle,
        &database,
        &[PolicyChange::DropPolicy {
            schema: schema.to_owned(),
            table: "documentos".into(),
            name: "nada_oculto".into(),
        }],
    )
    .await
    .expect("tenía que borrar la política restrictiva");

    assert_eq!(visibles(handle, schema, role).await, 2);
}

async fn el_dueno_se_saltea_el_filtro_salvo_que_se_fuerce(
    handle: &ServerHandle,
    schema: &str,
    role: &str,
) {
    let database = handle.default_database().to_owned();

    let client = handle.client(&database).await.unwrap();
    client
        .batch_execute(&format!("ALTER TABLE {schema}.documentos OWNER TO {role}"))
        .await
        .expect("tenía que poder cambiar el dueño de la tabla");

    assert_eq!(
        visibles(handle, schema, role).await,
        3,
        "el dueño se saltea sus propias políticas mientras no se fuerce el filtro"
    );

    policy::apply(
        handle,
        &database,
        &[PolicyChange::SetForceRowSecurity {
            schema: schema.to_owned(),
            table: "documentos".into(),
            forced: true,
        }],
    )
    .await
    .expect("tenía que forzar el filtro");

    assert_eq!(
        visibles(handle, schema, role).await,
        2,
        "forzado, el filtro alcanza también al dueño"
    );
}

async fn lo_leido_coincide_con_lo_creado(handle: &ServerHandle, schema: &str, role: &str) {
    let database = handle.default_database().to_owned();
    let oid = oid_of(handle, schema, "documentos").await;

    let security = policy::table_security(handle, &database, oid)
        .await
        .expect("tenía que leer el estado de seguridad");

    assert!(security.enabled);
    assert!(security.forced);
    assert_eq!(security.policies.len(), 1);

    let policy = &security.policies[0];
    assert_eq!(policy.name, "solo_los_propios");
    assert_eq!(policy.command, Command::All);
    assert_eq!(policy.kind, PolicyKind::Permissive);
    assert_eq!(policy.roles, vec![role.to_owned()]);
    assert!(
        policy
            .using
            .as_deref()
            .unwrap_or_default()
            .contains("dueno"),
        "la expresión tenía que volver del catálogo: {:?}",
        policy.using
    );
    assert!(policy.check.is_none());
}

/// No existe `pg_get_policydef`, así que el DDL de una política lo reconstruye pgforge. Que el
/// texto se vea bien no alcanza: tiene que volver a crear exactamente la misma política.
async fn el_ddl_reconstruido_vuelve_a_crear_la_misma_politica(
    handle: &ServerHandle,
    schema: &str,
    role: &str,
) {
    let database = handle.default_database().to_owned();
    let oid = oid_of(handle, schema, "documentos").await;

    let antes = policy::table_security(handle, &database, oid)
        .await
        .unwrap();
    let original = &antes.policies[0];
    let sql = policy::describe(handle, &database, original.oid)
        .await
        .expect("tenía que reconstruir el DDL de la política");

    policy::apply(
        handle,
        &database,
        &[PolicyChange::DropPolicy {
            schema: schema.to_owned(),
            table: "documentos".into(),
            name: original.name.clone(),
        }],
    )
    .await
    .unwrap();

    let client = handle.client(&database).await.unwrap();
    client
        .batch_execute(&sql)
        .await
        .unwrap_or_else(|e| panic!("el DDL reconstruido no era válido: {e}\n{sql}"));

    let despues = policy::table_security(handle, &database, oid)
        .await
        .unwrap();
    assert_eq!(despues.policies.len(), 1);

    let recreada = &despues.policies[0];
    assert_eq!(recreada.name, original.name);
    assert_eq!(recreada.command, original.command);
    assert_eq!(recreada.kind, original.kind);
    assert_eq!(recreada.roles, original.roles);
    assert_eq!(recreada.using, original.using);
    assert_eq!(recreada.check, original.check);

    // Y sigue filtrando igual que antes de borrarla y recrearla.
    assert_eq!(visibles(handle, schema, role).await, 2);
}

async fn apagar_el_filtro_devuelve_todo(handle: &ServerHandle, schema: &str, role: &str) {
    let database = handle.default_database().to_owned();

    policy::apply(
        handle,
        &database,
        &[PolicyChange::SetRowSecurity {
            schema: schema.to_owned(),
            table: "documentos".into(),
            enabled: false,
        }],
    )
    .await
    .expect("tenía que apagar el filtro");

    assert_eq!(
        visibles(handle, schema, role).await,
        3,
        "apagado, la política sigue existiendo pero no se aplica"
    );

    let oid = oid_of(handle, schema, "documentos").await;
    let security = policy::table_security(handle, &database, oid)
        .await
        .unwrap();
    assert!(!security.enabled);
    assert_eq!(
        security.policies.len(),
        1,
        "apagar el filtro no borra las políticas"
    );
}
