//! Ensayo de scripts contra servidores reales: nada queda aplicado.
//!
//! Lo que importa verificar no se ve sin servidor: que el `ROLLBACK` llegue incluso cuando una
//! sentencia falla, que las filas afectadas sean las de `CommandComplete` y que los locks se lean
//! de `pg_locks` en todas las versiones. Corre contra todas las instancias de `PGFORGE_TEST_URLS`.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::sql::rehearsal::{rehearse, RehearsalOptions};
use pgforge_core::sql::{Limits, Outcome, QuerySession, TxStatus};
use pgforge_core::{Error, Password};

fn test_urls() -> Vec<String> {
    std::env::var("PGFORGE_TEST_URLS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
        .collect()
}

fn profile_of(url: &str) -> (ConnectionProfile, Option<Password>) {
    ConnectionProfile::from_url("test", url)
        .unwrap_or_else(|e| panic!("URL de prueba inválida ({url}): {e}"))
}

async fn connect(url: &str, read_only: bool) -> Arc<ServerHandle> {
    let (mut profile, password) = profile_of(url);
    profile.read_only = read_only;
    // Con timeout a propósito: el ensayo no puede romperlo ni depender de que no esté.
    profile.statement_timeout_ms = Some(30_000);
    ConnectionManager::new()
        .connect(profile, password)
        .await
        .unwrap_or_else(|e| panic!("no se pudo conectar a {url}: {e}"))
}

fn schema_name() -> String {
    format!("pgforge_ensayo_{}", std::process::id())
}

async fn open(handle: &ServerHandle) -> QuerySession {
    QuerySession::open(handle, handle.default_database())
        .await
        .expect("no se pudo abrir la sesión de consulta")
}

async fn scalar(session: &QuerySession, sql: &str) -> String {
    match session.run(sql, Limits::default()).await.unwrap() {
        Outcome::Rows { rows, .. } => rows[0][0].clone().expect("no es NULL"),
        Outcome::Command { tag, .. } => panic!("se esperaban filas y llegó {tag}"),
    }
}

#[tokio::test]
async fn ensaya_sin_dejar_nada_aplicado_contra_servidores_reales() {
    let urls = test_urls();
    if urls.is_empty() {
        eprintln!(
            "AVISO: PGFORGE_TEST_URLS no está definida, no se verificó nada contra un servidor real."
        );
        return;
    }

    for url in urls {
        let handle = connect(&url, false).await;
        let read_only = connect(&url, true).await;
        let version = handle.caps.version;
        let schema = schema_name();

        {
            let client = handle.client(handle.default_database()).await.unwrap();
            client
                .batch_execute(&format!(
                    "CREATE SCHEMA {schema}; \
                     CREATE TABLE {schema}.notas (id int, texto text); \
                     INSERT INTO {schema}.notas VALUES (1, 'a'), (2, 'b'), (3, 'c')"
                ))
                .await
                .expect("no se pudo preparar el esquema de prueba");
        }

        let outcome = {
            let handle = Arc::clone(&handle);
            let read_only = Arc::clone(&read_only);
            let schema = schema.clone();
            tokio::spawn(async move {
                cuenta_filas_y_locks_y_deja_la_tabla_intacta(&handle, &schema).await;
                un_error_no_deja_nada_y_la_sesion_sigue_sana(&handle, &schema).await;
                rechaza_lo_que_no_corre_en_transaccion_sin_ejecutar_nada(&handle, &schema).await;
                no_ensaya_sobre_una_transaccion_ajena(&handle, &schema).await;
                en_solo_lectura_el_servidor_rechaza_la_escritura(&read_only, &schema).await;
            })
            .await
        };

        if let Ok(client) = handle.client(handle.default_database()).await {
            let _ = client
                .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
                .await;
        }

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({url})");
    }
}

async fn cuenta_filas_y_locks_y_deja_la_tabla_intacta(handle: &ServerHandle, schema: &str) {
    let session = open(handle).await;
    let otra = open(handle).await;

    let report = rehearse(
        &session,
        &format!(
            "UPDATE {schema}.notas SET texto = 'x' WHERE id <= 2;\n\
             DELETE FROM {schema}.notas;\n\
             ALTER TABLE {schema}.notas ADD COLUMN extra int"
        ),
        RehearsalOptions::default(),
    )
    .await
    .unwrap();

    assert!(report.blocked.is_empty());
    assert!(report.rolled_back, "{:?}", report.rollback_error);
    assert_eq!(report.steps.len(), 3);
    let affected: Vec<u64> = report
        .steps
        .iter()
        .map(|step| match step.outcome.as_ref().expect("no falló") {
            Outcome::Command { affected, .. } => *affected,
            Outcome::Rows { .. } => panic!("no devuelve filas"),
        })
        .collect();
    assert_eq!(affected, [2, 3, 0]);

    assert!(report.locks_available);
    let tabla = format!("{schema}.notas");
    let access_exclusive = report.locks.iter().any(|lock| {
        lock.relation.as_deref() == Some(tabla.as_str()) && lock.mode == "AccessExclusiveLock"
    });
    assert!(
        access_exclusive,
        "el ALTER TABLE toma ACCESS EXCLUSIVE: {:?}",
        report.locks
    );

    // Intacta, vista desde otra sesión y desde la misma.
    for s in [&otra, &session] {
        assert_eq!(
            scalar(s, &format!("SELECT count(*) FROM {schema}.notas")).await,
            "3"
        );
        assert_eq!(
            scalar(
                s,
                &format!("SELECT count(*) FROM {schema}.notas WHERE texto = 'x'")
            )
            .await,
            "0"
        );
        assert_eq!(
            scalar(
                s,
                &format!(
                    "SELECT count(*) FROM information_schema.columns \
                     WHERE table_schema = '{schema}' AND column_name = 'extra'"
                )
            )
            .await,
            "0"
        );
    }
    assert_eq!(session.tx_status().await.unwrap(), TxStatus::Idle);
}

async fn un_error_no_deja_nada_y_la_sesion_sigue_sana(handle: &ServerHandle, schema: &str) {
    let session = open(handle).await;

    let report = rehearse(
        &session,
        &format!(
            "DELETE FROM {schema}.notas WHERE id = 1;\n\
             SELECT * FROM {schema}.no_existe;\n\
             DELETE FROM {schema}.notas"
        ),
        RehearsalOptions::default(),
    )
    .await
    .unwrap();

    assert_eq!(report.total, 3);
    assert_eq!(report.steps.len(), 2, "se corta en la que falla");
    assert!(report.steps[0].error.is_none());
    assert!(report.steps[1].error.is_some());
    assert!(!report.locks_available);
    assert!(report.rolled_back, "el ROLLBACK va aunque haya fallado");

    assert_eq!(session.tx_status().await.unwrap(), TxStatus::Idle);
    assert_eq!(
        scalar(&session, &format!("SELECT count(*) FROM {schema}.notas")).await,
        "3"
    );
}

async fn rechaza_lo_que_no_corre_en_transaccion_sin_ejecutar_nada(
    handle: &ServerHandle,
    schema: &str,
) {
    let session = open(handle).await;

    let report = rehearse(
        &session,
        &format!(
            "DELETE FROM {schema}.notas;\n\
             CREATE INDEX CONCURRENTLY ix_ensayo ON {schema}.notas (id)"
        ),
        RehearsalOptions::default(),
    )
    .await
    .unwrap();

    assert_eq!(report.blocked.len(), 1);
    assert_eq!(report.blocked[0].index, 1);
    assert!(
        report.steps.is_empty(),
        "con un bloqueante no corre ni la primera"
    );
    assert_eq!(
        scalar(&session, &format!("SELECT count(*) FROM {schema}.notas")).await,
        "3"
    );
}

async fn no_ensaya_sobre_una_transaccion_ajena(handle: &ServerHandle, schema: &str) {
    let session = open(handle).await;
    session.begin_if_needed().await.unwrap();

    let error = rehearse(
        &session,
        &format!("DELETE FROM {schema}.notas"),
        RehearsalOptions::default(),
    )
    .await
    .expect_err("había una transacción abierta");
    assert!(matches!(error, Error::Config(_)), "{error}");

    // La del usuario sigue ahí: el ensayo no la tocó.
    assert_eq!(session.tx_status().await.unwrap(), TxStatus::Active);
    session.rollback().await.unwrap();
}

async fn en_solo_lectura_el_servidor_rechaza_la_escritura(handle: &ServerHandle, schema: &str) {
    let session = open(handle).await;

    let report = rehearse(
        &session,
        &format!("DELETE FROM {schema}.notas"),
        RehearsalOptions::default(),
    )
    .await
    .unwrap();

    assert!(
        report.steps[0].error.is_some(),
        "lo rechaza el servidor, no una lista nuestra"
    );
    assert!(report.rolled_back);
}
