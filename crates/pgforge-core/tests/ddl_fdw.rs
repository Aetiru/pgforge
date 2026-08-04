//! Datos externos contra servidores reales: wrappers, servidores foráneos y mapeos de usuario.
//!
//! Lo que se verifica acá no se puede sin servidor: que crear un servidor sobre un wrapper deje sus
//! opciones en el catálogo, que alterarlas con ADD/SET/DROP funcione, y que un mapeo de usuario se
//! cree y se lea. Crear estos objetos no conecta a ningún lado —solo guardan metadatos—, así que los
//! valores de host/port no necesitan ser alcanzables.
//!
//! Necesita el wrapper `postgres_fdw`, que viene con el paquete contrib. Si no está disponible en
//! `pg_available_extensions`, avisa por stderr y no verifica nada.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::ddl::fdw::{self, FdwChange, OptionsDelta, ServerChange, UserMappingChange};
use pgforge_core::ddl::quote_ident;

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

fn name(suffix: &str) -> String {
    format!("pgforge_fdw_{}_{suffix}", std::process::id())
}

async fn exec(handle: &ServerHandle, sql: &str) {
    let client = handle.client(handle.default_database()).await.unwrap();
    let _ = client.batch_execute(sql).await;
}

async fn has_postgres_fdw(handle: &ServerHandle) -> bool {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(
            "SELECT count(*) > 0 FROM pg_catalog.pg_available_extensions WHERE name = 'postgres_fdw'",
            &[],
        )
        .await
        .map(|row| row.get::<_, bool>(0))
        .unwrap_or(false)
}

async fn teardown(handle: &ServerHandle, server: &str, wrapper: &str, installed_fdw: bool) {
    exec(
        handle,
        &format!("DROP SERVER IF EXISTS {} CASCADE", quote_ident(server)),
    )
    .await;
    exec(
        handle,
        &format!(
            "DROP FOREIGN DATA WRAPPER IF EXISTS {} CASCADE",
            quote_ident(wrapper)
        ),
    )
    .await;
    // Solo se quita `postgres_fdw` si lo instaló este test: si ya estaba, es del usuario.
    if installed_fdw {
        exec(handle, "DROP EXTENSION IF EXISTS postgres_fdw CASCADE").await;
    }
}

#[tokio::test]
async fn crea_altera_y_borra_datos_externos_contra_servidores_reales() {
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

        if !has_postgres_fdw(&handle).await {
            eprintln!(
                "AVISO ({url}): postgres_fdw no está disponible; no se verificó nada contra PostgreSQL {version}."
            );
            continue;
        }

        // ¿Ya estaba instalado postgres_fdw? Si sí, no hay que quitarlo al limpiar.
        let already = {
            let client = handle.client(handle.default_database()).await.unwrap();
            client
                .query_one(
                    "SELECT count(*) > 0 FROM pg_catalog.pg_extension WHERE extname = 'postgres_fdw'",
                    &[],
                )
                .await
                .unwrap()
                .get::<_, bool>(0)
        };
        exec(&handle, "CREATE EXTENSION IF NOT EXISTS postgres_fdw").await;

        let server = name("srv");
        let wrapper = name("wrap");
        teardown(&handle, &server, &wrapper, false).await;

        let outcome = {
            let handle = Arc::clone(&handle);
            let server = server.clone();
            let wrapper = wrapper.clone();
            tokio::spawn(async move {
                un_wrapper_propio(&handle, &wrapper).await;
                un_servidor_y_su_mapeo(&handle, &server).await;
            })
            .await
        };

        teardown(&handle, &server, &wrapper, !already).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({url})");
    }
}

async fn un_wrapper_propio(handle: &ServerHandle, wrapper: &str) {
    let database = handle.default_database().to_owned();

    let statements = fdw::fdw_statements(&[FdwChange::Create {
        name: wrapper.to_owned(),
        handler: None,
        validator: None,
        options: vec![("delimiter".into(), ",".into())],
    }])
    .unwrap();
    fdw::apply(handle, &database, &statements)
        .await
        .expect("tenía que crear el wrapper");

    let info = fdw::fdw_info(handle, &database, wrapper).await.unwrap();
    assert!(info.handler.is_none());
    assert_eq!(info.options, vec![("delimiter".into(), ",".into())]);

    // Alterar: cambia el valor de una opción y agrega otra.
    let statements = fdw::fdw_statements(&[FdwChange::Alter {
        name: wrapper.to_owned(),
        handler: None,
        no_handler: false,
        validator: None,
        no_validator: false,
        options: OptionsDelta {
            add: vec![("quote".into(), "\"".into())],
            set: vec![("delimiter".into(), ";".into())],
            drop: vec![],
        },
    }])
    .unwrap();
    fdw::apply(handle, &database, &statements)
        .await
        .expect("tenía que alterar el wrapper");

    let info = fdw::fdw_info(handle, &database, wrapper).await.unwrap();
    let options: std::collections::HashMap<_, _> = info.options.into_iter().collect();
    assert_eq!(options.get("delimiter").map(String::as_str), Some(";"));
    assert_eq!(options.get("quote").map(String::as_str), Some("\""));
}

async fn un_servidor_y_su_mapeo(handle: &ServerHandle, server: &str) {
    let database = handle.default_database().to_owned();

    let statements = fdw::server_statements(&[ServerChange::Create {
        name: server.to_owned(),
        fdw: "postgres_fdw".into(),
        server_type: None,
        version: None,
        options: vec![
            ("host".into(), "localhost".into()),
            ("port".into(), "5432".into()),
            ("dbname".into(), "postgres".into()),
        ],
    }])
    .unwrap();
    fdw::apply(handle, &database, &statements)
        .await
        .expect("tenía que crear el servidor foráneo");

    let info = fdw::server_info(handle, &database, server).await.unwrap();
    assert_eq!(info.fdw, "postgres_fdw");
    let options: std::collections::HashMap<_, _> = info.options.iter().cloned().collect();
    assert_eq!(options.get("host").map(String::as_str), Some("localhost"));

    // Alterar opciones del servidor: SET una, DROP otra.
    let statements = fdw::server_statements(&[ServerChange::Alter {
        name: server.to_owned(),
        version: None,
        options: OptionsDelta {
            add: vec![],
            set: vec![("dbname".into(), "otra".into())],
            drop: vec!["port".into()],
        },
    }])
    .unwrap();
    fdw::apply(handle, &database, &statements)
        .await
        .expect("tenía que alterar el servidor");

    let info = fdw::server_info(handle, &database, server).await.unwrap();
    let options: std::collections::HashMap<_, _> = info.options.into_iter().collect();
    assert_eq!(options.get("dbname").map(String::as_str), Some("otra"));
    assert!(
        !options.contains_key("port"),
        "la opción port tenía que quedar quitada"
    );

    // Mapeo de usuario para el rol conectado.
    let statements = fdw::user_mapping_statements(&[UserMappingChange::Create {
        server: server.to_owned(),
        user: "current_user".into(),
        options: vec![("user".into(), "remoto".into())],
    }])
    .unwrap();
    fdw::apply(handle, &database, &statements)
        .await
        .expect("tenía que crear el mapeo");

    let mappings = fdw::user_mappings(handle, &database, server).await.unwrap();
    let current = &handle.caps.current_user;
    assert!(
        mappings.iter().any(|mapping| &mapping.user == current),
        "el mapeo del rol conectado tenía que aparecer: {mappings:?}"
    );

    // Quitar el mapeo.
    let statements = fdw::user_mapping_statements(&[UserMappingChange::Drop {
        server: server.to_owned(),
        user: "current_user".into(),
    }])
    .unwrap();
    fdw::apply(handle, &database, &statements)
        .await
        .expect("tenía que quitar el mapeo");

    let mappings = fdw::user_mappings(handle, &database, server).await.unwrap();
    assert!(mappings.iter().all(|mapping| &mapping.user != current));
}
