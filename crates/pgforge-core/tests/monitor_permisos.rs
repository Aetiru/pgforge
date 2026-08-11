//! Monitoreo con un rol que no puede leer las estadísticas de los demás.
//!
//! `pg_stat_activity` devuelve NULL en casi toda columna de las sesiones ajenas cuando el rol
//! conectado no es superusuario ni miembro de `pg_read_all_stats`. Leerlas como `String` hacía
//! panic dentro de la tarea del monitoreo, y con `panic = "abort"` eso cerraba la aplicación
//! entera: es exactamente el caso que no se puede verificar sin un servidor real.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::monitor::activity::{self, ActivityFilter};

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

fn role_name() -> String {
    format!("pgforge_sin_stats_{}", std::process::id())
}

/// El rol va sin `LOGIN` a propósito: no hace falta conectarse con él, alcanza con `SET ROLE` sobre
/// una sesión dedicada. `pg_stat_activity` decide qué esconder mirando el usuario actual.
async fn setup(handle: &ServerHandle, role: &str) {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .batch_execute(&format!(
            "DROP ROLE IF EXISTS {role}; CREATE ROLE {role} NOSUPERUSER;"
        ))
        .await
        .expect("no se pudo crear el rol de prueba (¿la URL conecta con un superusuario?)");
}

async fn teardown(handle: &ServerHandle, role: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!("DROP ROLE IF EXISTS {role}"))
            .await;
    }
}

#[tokio::test]
async fn lee_las_sesiones_con_un_rol_sin_permiso_de_ver_estadisticas() {
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
        let role = role_name();
        setup(&handle, &role).await;

        let outcome = {
            let handle = Arc::clone(&handle);
            let role = role.clone();
            tokio::spawn(async move {
                // Sesión dedicada y no una del pool: el `SET ROLE` quedaría pegado a la conexión
                // reciclada y ensuciaría lo que haga el resto de la prueba.
                let session = handle
                    .open_session(handle.default_database(), None)
                    .await
                    .expect("no se pudo abrir la sesión");
                session
                    .client()
                    .batch_execute(&format!("SET ROLE {role}"))
                    .await
                    .expect("no se pudo cambiar de rol");

                let sesiones = activity::backends(session.client(), &handle.caps)
                    .await
                    .expect("leer pg_stat_activity sin permisos no tiene que fallar");

                assert!(
                    !sesiones.is_empty(),
                    "la propia sesión de la prueba tenía que aparecer"
                );
                // `backend_type` es la columna que el servidor anula para todas las sesiones ajenas
                // —incluida la propia, porque compara contra el usuario de sesión y no contra el de
                // `SET ROLE`—. Si viniera, la prueba no estaría ejercitando el caso.
                assert!(
                    sesiones.iter().all(|b| b.backend_type.is_none()),
                    "con el rol sin privilegios el servidor tenía que esconder el backend_type"
                );

                // Sin poder ver el tipo, la sesión cuenta como sesión de usuario: si se la tomara
                // por proceso interno, el filtro por omisión dejaría la lista y las métricas en cero.
                let visibles = ActivityFilter::default().apply(sesiones);
                assert!(
                    !visibles.is_empty(),
                    "el filtro por omisión no tiene que vaciar la lista por falta de permisos"
                );
            })
            .await
        };

        teardown(&handle, &role).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({url})");
    }
}
