//! Backups contra servidores reales.
//!
//! Lo que no se puede verificar sin servidor es lo único que importa acá: que el archivo que queda
//! sea un backup de verdad. Que la línea de comando esté bien armada ya lo cubren los tests
//! unitarios; que `pg_dump` la acepte, no.
//!
//! El archivo en formato custom se comprueba con `pg_restore --list`, que es la herramienta que va
//! a tener que leerlo el día que haga falta restaurar. Mirar solo el tamaño del archivo no
//! distingue un backup válido de un montón de bytes.

use std::path::PathBuf;
use std::sync::Arc;

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
    format!("pgforge_backup_{}", std::process::id())
}

fn work_dir() -> PathBuf {
    std::env::temp_dir().join(format!("pgforge_backup_{}", std::process::id()))
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

fn options(handle: &ServerHandle, schema: &str, format: Format, path: PathBuf) -> BackupOptions {
    BackupOptions {
        database: handle.default_database().to_owned(),
        format,
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

/// Corre el backup descartando el progreso. Nadie cancela: el extremo que envía se suelta acá
/// mismo y la espera nunca se resuelve.
async fn run(handle: &ServerHandle, options: &BackupOptions) -> pgforge_core::Result<u64> {
    let (progress, mut lines) = mpsc::channel(64);
    let drain = tokio::spawn(async move { while lines.recv().await.is_some() {} });
    let (_cancel, never) = oneshot::channel();

    let outcome = backup::run(handle, options, progress, never).await;
    let _ = drain.await;
    outcome.map(|outcome| outcome.bytes)
}

#[tokio::test]
async fn hace_backups_contra_servidores_reales() {
    let urls = test_urls();
    if urls.is_empty() {
        eprintln!(
            "AVISO: PGFORGE_TEST_URLS no está definida, no se verificó nada contra un servidor real."
        );
        return;
    }

    let Some(binary) = tools::find(Tool::PgDump) else {
        eprintln!("AVISO: no hay pg_dump en esta máquina, no se verificó ningún backup.");
        return;
    };
    let dump_version = tools::version(&binary)
        .await
        .expect("pg_dump tenía que decir su versión");

    for url in urls {
        let handle = connect(&url).await;
        let version = handle.caps.version;

        // `pg_dump` no puede leer un servidor más nuevo que él. No es una falla del código: es una
        // máquina sin las herramientas cliente de esa versión, y decirlo vale más que fallar.
        if dump_version.major() < version.major() {
            eprintln!(
                "AVISO: el pg_dump de esta máquina es {dump_version} y el servidor {version}: \
                 salteado ({})",
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
                el_formato_custom_es_un_archivo_que_pg_restore_puede_leer(&handle, &schema).await;
                el_formato_plano_es_el_sql_de_las_tablas(&handle, &schema).await;
                un_backup_fallido_no_deja_el_archivo_a_medias(&handle, &schema).await;
                cancelar_no_deja_el_archivo_a_medias(&handle, &schema).await;
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

async fn el_formato_custom_es_un_archivo_que_pg_restore_puede_leer(
    handle: &ServerHandle,
    schema: &str,
) {
    let path = work_dir().join("custom.dump");
    let options = options(handle, schema, Format::Custom, path.clone());

    let bytes = run(handle, &options)
        .await
        .expect("tenía que hacer el backup");
    assert!(path.is_file(), "no quedó el archivo en {}", path.display());
    assert!(bytes > 0, "el archivo quedó vacío");
    assert_eq!(
        bytes,
        std::fs::metadata(&path).unwrap().len(),
        "el tamaño informado no es el del archivo"
    );

    let Some(pg_restore) = tools::find(Tool::PgRestore) else {
        eprintln!("AVISO: no hay pg_restore, no se verificó el contenido del archivo custom.");
        return;
    };
    let listing = std::process::Command::new(pg_restore)
        .arg("--list")
        .arg(&path)
        .output()
        .expect("no se pudo ejecutar pg_restore --list");
    assert!(
        listing.status.success(),
        "pg_restore no pudo leer el archivo: {}",
        String::from_utf8_lossy(&listing.stderr)
    );

    let listing = String::from_utf8_lossy(&listing.stdout);
    assert!(
        listing.contains("clientes"),
        "la tabla tenía que estar en el índice del archivo:\n{listing}"
    );
}

async fn el_formato_plano_es_el_sql_de_las_tablas(handle: &ServerHandle, schema: &str) {
    let path = work_dir().join("plano.sql");
    let options = options(handle, schema, Format::Plain, path.clone());

    run(handle, &options)
        .await
        .expect("tenía que hacer el backup");

    let sql = std::fs::read_to_string(&path).expect("tenía que poder leerse");
    assert!(
        sql.contains(&format!("CREATE TABLE {schema}.clientes")),
        "el script tenía que traer la tabla:\n{sql}"
    );
    // Sin `--schema-only`, los datos van adentro.
    assert!(sql.contains("ana"), "el script tenía que traer las filas");
}

/// Lo mismo que el de abajo pero por el otro camino: cancelar a mitad tampoco puede dejar un
/// archivo que parezca un backup.
///
/// Para que sea determinista se le pone a `pg_dump` un candado en el medio: mientras otra sesión
/// tenga la tabla en ACCESS EXCLUSIVE, `pg_dump` se queda esperando su ACCESS SHARE y no termina
/// nunca. Sin eso habría que cancelar «rápido» y confiar en llegar a tiempo.
async fn cancelar_no_deja_el_archivo_a_medias(handle: &Arc<ServerHandle>, schema: &str) {
    let database = handle.default_database().to_owned();
    let path = work_dir().join("cancelado.dump");
    let options = options(handle, schema, Format::Custom, path.clone());

    let bloqueo = handle.client(&database).await.unwrap();
    bloqueo
        .batch_execute(&format!(
            "BEGIN; LOCK TABLE {schema}.clientes IN ACCESS EXCLUSIVE MODE;"
        ))
        .await
        .expect("tenía que poder tomar el candado");

    let (progress, mut lines) = mpsc::channel(64);
    let drain = tokio::spawn(async move { while lines.recv().await.is_some() {} });
    let (cancel, cancelled) = oneshot::channel();

    let corriendo = {
        let handle = Arc::clone(handle);
        let options = options.clone();
        tokio::spawn(async move { backup::run(&handle, &options, progress, cancelled).await })
    };

    let esperando = espera_el_candado(handle, &database).await;
    let _ = cancel.send(());

    let resultado = corriendo.await.expect("la tarea no tenía que panickear");
    let _ = drain.await;
    let _ = bloqueo.batch_execute("ROLLBACK").await;

    if !esperando {
        eprintln!("AVISO: pg_dump no llegó a quedarse esperando el candado, no se probó cancelar.");
        let _ = std::fs::remove_file(&path);
        return;
    }

    assert!(
        matches!(resultado, Err(pgforge_core::Error::Canceled)),
        "cancelar tiene que reportarse como cancelación y no como falla: {resultado:?}"
    );
    assert!(
        !path.exists(),
        "quedó un archivo a medias en {}",
        path.display()
    );
}

/// Espera a que `pg_dump` aparezca trabado en un candado. Devuelve `false` si no pasó en el plazo,
/// para poder decirlo en vez de dar por probado algo que no se probó.
async fn espera_el_candado(handle: &ServerHandle, database: &str) -> bool {
    let client = handle.client(database).await.unwrap();
    for _ in 0..60 {
        let waiting: i64 = client
            .query_one(
                "SELECT count(*) FROM pg_stat_activity
                  WHERE application_name = 'pgforge' AND wait_event_type = 'Lock'",
                &[],
            )
            .await
            .map(|row| row.get(0))
            .unwrap_or(0);
        if waiting > 0 {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    false
}

/// Un backup truncado que parece válido es peor que no tener backup: si `pg_dump` falla, lo que
/// haya alcanzado a escribir tiene que desaparecer.
async fn un_backup_fallido_no_deja_el_archivo_a_medias(handle: &ServerHandle, schema: &str) {
    let path = work_dir().join("fallido.dump");
    let mut options = options(handle, schema, Format::Custom, path.clone());
    options.database = format!("no_existe_{}", std::process::id());

    let error = run(handle, &options)
        .await
        .expect_err("una base inexistente tiene que fallar");
    assert!(
        !path.exists(),
        "quedó un archivo a medias en {} ({error})",
        path.display()
    );
}
