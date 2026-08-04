//! Restore contra servidores reales.
//!
//! Lo único que importa acá es lo que no se puede verificar sin servidor: que un backup vuelva a
//! ser una base. Que la línea de comando esté bien armada ya lo cubren los tests unitarios de
//! `backup::restore`; que `pg_restore` la acepte y recupere de verdad los datos, no.
//!
//! Por eso el test hace el viaje redondo: hace un backup con `pg_dump`, borra el esquema, lo
//! restaura con `pg_restore` y comprueba contra el catálogo que el esquema y sus filas volvieron.
//!
//! Queda afuera cancelar a mitad: en un restore es difícil de volver determinista (termina rápido,
//! y trabarlo a propósito es frágil), y además el camino de cancelación es el mismo
//! `backup::spawn_streaming` que ejercita `tests/backup.rs`.

use std::path::PathBuf;
use std::sync::Arc;

use pgforge_core::backup::restore::{self, RestoreOptions};
use pgforge_core::backup::tools::{self, Tool};
use pgforge_core::backup::{self, BackupOptions, Format};
use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use tokio::sync::{mpsc, oneshot};

fn test_urls() -> Vec<String> {
    std::env::var("PGFORGE_TEST_URLS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
        .collect()
}

/// La URL sin la contraseña, para poder nombrarla en la salida del test. Los mensajes de los tests
/// terminan en el registro de CI, y ahí una credencial en claro queda para siempre.
fn redacted(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned();
    };
    let Some((credentials, target)) = rest.split_once('@') else {
        return url.to_owned();
    };
    let user = credentials.split_once(':').map_or(credentials, |(u, _)| u);
    format!("{scheme}://{user}:***@{target}")
}

async fn connect(url: &str) -> Arc<ServerHandle> {
    let (profile, password) = ConnectionProfile::from_url("test", url)
        .unwrap_or_else(|e| panic!("URL de prueba inválida ({}): {e}", redacted(url)));
    let manager = ConnectionManager::new();
    manager
        .connect(profile, password)
        .await
        .unwrap_or_else(|e| panic!("no se pudo conectar a {}: {e}", redacted(url)))
}

fn schema_name() -> String {
    format!("pgforge_restore_{}", std::process::id())
}

fn work_dir() -> PathBuf {
    std::env::temp_dir().join(format!("pgforge_restore_{}", std::process::id()))
}

async fn setup(handle: &ServerHandle, schema: &str) {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE;
             CREATE SCHEMA {schema};
             CREATE TABLE {schema}.clientes (id bigint PRIMARY KEY, nombre text);
             INSERT INTO {schema}.clientes VALUES (1, 'ana'), (2, 'beto');"
        ))
        .await
        .expect("no se pudo preparar el esquema de prueba");
}

async fn teardown(handle: &ServerHandle, schema: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .await;
    }
    let _ = std::fs::remove_dir_all(work_dir());
}

fn backup_options(handle: &ServerHandle, schema: &str, path: PathBuf) -> BackupOptions {
    BackupOptions {
        database: handle.default_database().to_owned(),
        format: Format::Custom,
        path,
        schemas: vec![schema.to_owned()],
        exclude_schemas: vec![],
        tables: vec![],
        schema_only: false,
        data_only: false,
        no_owner: false,
        no_privileges: false,
        compression: None,
        jobs: None,
    }
}

fn restore_options(handle: &ServerHandle, source: PathBuf) -> RestoreOptions {
    RestoreOptions {
        source,
        format: Format::Custom,
        database: handle.default_database().to_owned(),
        schemas: vec![],
        tables: vec![],
        schema_only: false,
        data_only: false,
        clean: false,
        if_exists: false,
        create: false,
        no_owner: false,
        no_privileges: false,
        single_transaction: false,
        jobs: None,
    }
}

/// Hace un backup descartando el progreso. Nadie cancela: el extremo que envía se suelta acá mismo
/// y la espera nunca se resuelve.
async fn run_backup(handle: &ServerHandle, options: &BackupOptions) {
    let (progress, mut lines) = mpsc::channel(64);
    let drain = tokio::spawn(async move { while lines.recv().await.is_some() {} });
    let (_cancel, never) = oneshot::channel();

    let outcome = backup::run(handle, options, progress, never).await;
    let _ = drain.await;
    outcome.expect("tenía que hacer el backup");
}

/// Igual que [`run_backup`] pero para el restore.
async fn run_restore(handle: &ServerHandle, options: &RestoreOptions) {
    let (progress, mut lines) = mpsc::channel(64);
    let drain = tokio::spawn(async move { while lines.recv().await.is_some() {} });
    let (_cancel, never) = oneshot::channel();

    let outcome = restore::run(handle, options, progress, never).await;
    let _ = drain.await;
    outcome.expect("tenía que hacer el restore");
}

/// Cuántas filas tiene la tabla del esquema de prueba.
async fn cuenta_clientes(handle: &ServerHandle, schema: &str) -> i64 {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(&format!("SELECT count(*) FROM {schema}.clientes"), &[])
        .await
        .expect("tenía que poder contar las filas")
        .get(0)
}

/// Si el esquema existe en el catálogo.
async fn existe_el_esquema(handle: &ServerHandle, schema: &str) -> bool {
    let client = handle.client(handle.default_database()).await.unwrap();
    let count: i64 = client
        .query_one(
            "SELECT count(*) FROM pg_namespace WHERE nspname = $1",
            &[&schema],
        )
        .await
        .expect("tenía que poder consultar pg_namespace")
        .get(0);
    count > 0
}

#[tokio::test]
async fn restaura_contra_servidores_reales() {
    let urls = test_urls();
    if urls.is_empty() {
        eprintln!(
            "AVISO: PGFORGE_TEST_URLS no está definida, no se verificó nada contra un servidor real."
        );
        return;
    }

    // El viaje redondo necesita las dos herramientas: una escribe el archivo, la otra lo lee.
    let (Some(pg_dump), Some(pg_restore)) =
        (tools::find(Tool::PgDump), tools::find(Tool::PgRestore))
    else {
        eprintln!("AVISO: falta pg_dump o pg_restore en esta máquina, no se verificó el restore.");
        return;
    };
    let dump_version = tools::version(&pg_dump)
        .await
        .expect("pg_dump tenía que decir su versión");
    let restore_version = tools::version(&pg_restore)
        .await
        .expect("pg_restore tenía que decir su versión");

    for url in urls {
        let handle = connect(&url).await;
        let version = handle.caps.version;

        // Ninguna de las dos herramientas puede con un servidor más nuevo que ella. No es una falla
        // del código: es una máquina sin las client tools de esa versión, y decirlo vale más que
        // fallar.
        if dump_version.major() < version.major() || restore_version.major() < version.major() {
            eprintln!(
                "AVISO: pg_dump {dump_version} / pg_restore {restore_version} contra un servidor \
                 {version}: salteado ({})",
                redacted(&url)
            );
            continue;
        }

        let schema = schema_name();
        teardown(&handle, &schema).await; // por si quedó algo de una corrida anterior
        setup(&handle, &schema).await;
        std::fs::create_dir_all(work_dir()).expect("no se pudo crear el directorio de trabajo");

        let outcome = {
            let handle = Arc::clone(&handle);
            let schema = schema.clone();
            tokio::spawn(async move {
                el_ciclo_completo_recupera_el_esquema_y_los_datos(&handle, &schema).await;
                limpiar_devuelve_la_tabla_al_estado_del_backup(&handle, &schema).await;
            })
            .await
        };

        teardown(&handle, &schema).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({})", redacted(&url));
    }
}

/// El viaje redondo: backup, borrar todo, restaurar, y que el esquema con sus filas haya vuelto.
async fn el_ciclo_completo_recupera_el_esquema_y_los_datos(handle: &ServerHandle, schema: &str) {
    let path = work_dir().join("ciclo.dump");
    run_backup(handle, &backup_options(handle, schema, path.clone())).await;

    // Se borra el esquema entero: si el restore no lo recrea, la comprobación de después falla sola.
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .expect("tenía que poder borrar el esquema");
    assert!(
        !existe_el_esquema(handle, schema).await,
        "el esquema tenía que quedar borrado antes del restore"
    );

    run_restore(handle, &restore_options(handle, path)).await;

    assert!(
        existe_el_esquema(handle, schema).await,
        "el restore tenía que recrear el esquema"
    );
    assert_eq!(
        cuenta_clientes(handle, schema).await,
        2,
        "el restore tenía que devolver las dos filas originales"
    );
}

/// `--clean` reconstruye desde el backup: una fila agregada después del backup no sobrevive al
/// restore.
async fn limpiar_devuelve_la_tabla_al_estado_del_backup(handle: &ServerHandle, schema: &str) {
    let path = work_dir().join("limpiar.dump");
    run_backup(handle, &backup_options(handle, schema, path.clone())).await;

    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .batch_execute(&format!(
            "INSERT INTO {schema}.clientes VALUES (3, 'ensucia')"
        ))
        .await
        .expect("tenía que poder ensuciar la tabla");
    assert_eq!(cuenta_clientes(handle, schema).await, 3, "quedó ensuciada");

    let mut options = restore_options(handle, path);
    options.clean = true;
    run_restore(handle, &options).await;

    assert_eq!(
        cuenta_clientes(handle, schema).await,
        2,
        "«limpiar» tenía que devolver la tabla al estado del backup"
    );
}
