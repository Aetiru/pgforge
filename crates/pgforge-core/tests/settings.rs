//! Configuración del servidor contra servidores reales.
//!
//! Lo que se verifica acá no se puede sin servidor: que `ALTER SYSTEM SET` sobre un parámetro de
//! recarga cambie el valor efectivo, y que `RESET` lo devuelva al de fábrica. Se usa
//! `log_min_duration_statement` (contexto `sighup`, default `-1`) porque cambiarlo no afecta a nada
//! y siempre está. El test **siempre** hace el `RESET` al final: `ALTER SYSTEM` escribe
//! `postgresql.auto.conf` en disco.
//!
//! El valor se relee con reintentos: `pg_reload_conf()` avisa por SIGHUP y el backend puede tardar
//! un instante en ver el cambio.

use std::sync::Arc;
use std::time::Duration;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::settings::{self, SettingChange};

const PARAM: &str = "log_min_duration_statement";

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

async fn value_of(handle: &ServerHandle, name: &str) -> String {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(
            "SELECT setting FROM pg_catalog.pg_settings WHERE name = $1",
            &[&name],
        )
        .await
        .unwrap()
        .get(0)
}

/// Relee el valor hasta que sea `expected`, o falla tras varios intentos: el reload por SIGHUP no es
/// instantáneo.
async fn wait_for(handle: &ServerHandle, name: &str, expected: &str) {
    for _ in 0..20 {
        if value_of(handle, name).await == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!(
        "{name} tenía que quedar en «{expected}», quedó en «{}»",
        value_of(handle, name).await
    );
}

#[tokio::test]
async fn lee_y_cambia_la_configuracion_contra_servidores_reales() {
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

        // Listar trae los parámetros conocidos.
        let list = settings::list(&handle).await.unwrap();
        assert!(
            list.iter().any(|setting| setting.name == "work_mem"),
            "la lista tenía que incluir work_mem"
        );

        let outcome = {
            let handle = Arc::clone(&handle);
            tokio::spawn(async move {
                // Cambiar y verificar que el valor efectivo cambió.
                let pending = settings::apply(
                    &handle,
                    &[SettingChange::Set {
                        name: PARAM.into(),
                        value: "250".into(),
                    }],
                )
                .await
                .expect("tenía que aplicar el cambio");
                assert!(
                    !pending,
                    "un parámetro sighup no queda pendiente de reinicio"
                );
                wait_for(&handle, PARAM, "250").await;

                // Restablecer y verificar que volvió al default (-1).
                settings::apply(&handle, &[SettingChange::Reset { name: PARAM.into() }])
                    .await
                    .expect("tenía que restablecer");
                wait_for(&handle, PARAM, "-1").await;
            })
            .await
        };

        // Pase lo que pase, dejar el parámetro como estaba.
        let _ = settings::apply(&handle, &[SettingChange::Reset { name: PARAM.into() }]).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({url})");
    }
}
