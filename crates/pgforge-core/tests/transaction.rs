//! Transacciones explícitas y conexiones de solo lectura, contra servidores reales.
//!
//! Nada de esto se puede verificar sin servidor: el estado transaccional se averigua preguntándoselo
//! a PostgreSQL, y el solo lectura lo impone el propio servidor a partir de una opción de arranque.
//! Corre contra todas las instancias de `PGFORGE_TEST_URLS` porque es justo el tipo de cosa donde una
//! versión podría contestar distinto.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
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

async fn connect(url: &str) -> Arc<ServerHandle> {
    let (profile, password) = profile_of(url);
    ConnectionManager::new()
        .connect(profile, password)
        .await
        .unwrap_or_else(|e| panic!("no se pudo conectar a {url}: {e}"))
}

/// El mismo servidor, pero con el perfil marcado como conexión de solo lectura.
async fn connect_read_only(url: &str) -> Arc<ServerHandle> {
    let (mut profile, password) = profile_of(url);
    profile.read_only = true;
    // Junto con el timeout a propósito: los dos viajan en la misma opción de arranque, y el error
    // fácil es que uno pise al otro.
    profile.statement_timeout_ms = Some(30_000);

    ConnectionManager::new()
        .connect(profile, password)
        .await
        .unwrap_or_else(|e| panic!("no se pudo conectar a {url} en solo lectura: {e}"))
}

fn schema_name() -> String {
    format!("pgforge_tx_{}", std::process::id())
}

async fn session(handle: &ServerHandle) -> QuerySession {
    QuerySession::open(handle, handle.default_database())
        .await
        .expect("no se pudo abrir la sesión de consulta")
}

fn rows_of(outcome: &Outcome) -> &Vec<Vec<Option<String>>> {
    match outcome {
        Outcome::Rows { rows, .. } => rows,
        Outcome::Command { tag, .. } => panic!("se esperaban filas y llegó un comando: {tag}"),
    }
}

#[tokio::test]
async fn maneja_transacciones_y_solo_lectura_contra_servidores_reales() {
    let urls = test_urls();
    if urls.is_empty() {
        eprintln!(
            "AVISO: PGFORGE_TEST_URLS no está definida, no se verificó nada contra un servidor real."
        );
        return;
    }

    for url in urls {
        let handle = connect(&url).await;
        let read_only = connect_read_only(&url).await;
        let version = handle.caps.version;
        let schema = schema_name();

        preparar(&handle, &schema).await;

        let outcome = {
            let handle = Arc::clone(&handle);
            let read_only = Arc::clone(&read_only);
            let schema = schema.clone();
            tokio::spawn(async move {
                revierte_lo_que_no_se_confirmo(&handle, &schema).await;
                confirma_lo_que_se_pide(&handle, &schema).await;
                reporta_la_transaccion_abortada(&handle).await;
                abre_transaccion_solo_si_no_habia(&handle).await;
                rechaza_las_escrituras_en_solo_lectura(&read_only, &schema).await;
            })
            .await
        };

        limpiar(&handle, &schema).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({url})");
    }
}

async fn preparar(handle: &ServerHandle, schema: &str) {
    let client = handle
        .client(handle.default_database())
        .await
        .expect("no se pudo tomar una conexión del pool");
    client
        .batch_execute(&format!(
            "CREATE SCHEMA {schema}; CREATE TABLE {schema}.notas (id int)"
        ))
        .await
        .expect("no se pudo preparar el esquema de prueba");
}

async fn limpiar(handle: &ServerHandle, schema: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .await;
    }
}

async fn contar(session: &QuerySession, schema: &str) -> i64 {
    let outcome = session
        .run(
            &format!("SELECT count(*) FROM {schema}.notas"),
            Limits::default(),
        )
        .await
        .expect("no se pudo contar las filas");
    rows_of(&outcome)[0][0]
        .as_deref()
        .expect("count nunca devuelve NULL")
        .parse()
        .expect("count devuelve un entero")
}

/// Lo que hace útil al botón Rollback: lo insertado desde el `BEGIN` desaparece.
async fn revierte_lo_que_no_se_confirmo(handle: &ServerHandle, schema: &str) {
    let session = session(handle).await;
    assert_eq!(session.tx_status().await.unwrap(), TxStatus::Idle);

    assert_eq!(session.begin_if_needed().await.unwrap(), TxStatus::Active);
    session
        .run(
            &format!("INSERT INTO {schema}.notas VALUES (1)"),
            Limits::default(),
        )
        .await
        .unwrap();
    assert_eq!(session.tx_status().await.unwrap(), TxStatus::Active);

    session.rollback().await.unwrap();
    assert_eq!(session.tx_status().await.unwrap(), TxStatus::Idle);
    assert_eq!(contar(&session, schema).await, 0);
}

async fn confirma_lo_que_se_pide(handle: &ServerHandle, schema: &str) {
    let otra = session(handle).await;
    let session = session(handle).await;

    session.begin_if_needed().await.unwrap();
    session
        .run(
            &format!("INSERT INTO {schema}.notas VALUES (2)"),
            Limits::default(),
        )
        .await
        .unwrap();
    session.commit().await.unwrap();

    assert_eq!(session.tx_status().await.unwrap(), TxStatus::Idle);
    // Se cuenta desde otra sesión: lo confirmado tiene que verse desde afuera, no solo acá.
    assert_eq!(contar(&otra, schema).await, 1);
}

/// El tercer estado, y el motivo por el que la sonda no puede ser un simple `true`/`false`: adentro
/// de una transacción rota el servidor rechaza todo hasta el `ROLLBACK`, incluida la sonda misma.
async fn reporta_la_transaccion_abortada(handle: &ServerHandle) {
    let session = session(handle).await;

    session.begin_if_needed().await.unwrap();
    session
        .run("SELECT no_existe_esta_funcion()", Limits::default())
        .await
        .expect_err("la sentencia inválida tenía que fallar");

    assert_eq!(session.tx_status().await.unwrap(), TxStatus::Failed);
    session.rollback().await.unwrap();
    assert_eq!(session.tx_status().await.unwrap(), TxStatus::Idle);
}

/// Con autocommit apagado, cada ejecución llama a `begin_if_needed`: la segunda no puede abrir una
/// transacción anidada ni perder la que ya estaba.
async fn abre_transaccion_solo_si_no_habia(handle: &ServerHandle) {
    let session = session(handle).await;

    assert_eq!(session.begin_if_needed().await.unwrap(), TxStatus::Active);
    assert_eq!(session.begin_if_needed().await.unwrap(), TxStatus::Active);

    let outcome = session
        .run("SELECT txid_current() = txid_current()", Limits::default())
        .await
        .expect("la transacción tenía que seguir viva");
    assert_eq!(rows_of(&outcome)[0][0].as_deref(), Some("t"));

    session.rollback().await.unwrap();
}

/// El perfil de solo lectura no deshabilita nada del lado de la aplicación: es el servidor el que
/// rechaza, así vale igual para el editor de SQL que para cualquier otro camino.
async fn rechaza_las_escrituras_en_solo_lectura(handle: &ServerHandle, schema: &str) {
    let session = session(handle).await;

    // Leer sigue funcionando: lo que se corta es la escritura, no la conexión.
    assert_eq!(contar(&session, schema).await, 1);

    let error = session
        .run(
            &format!("INSERT INTO {schema}.notas VALUES (3)"),
            Limits::default(),
        )
        .await
        .expect_err("el INSERT tenía que ser rechazado");
    assert!(
        matches!(&error, Error::Database { code, .. } if code == "25006"),
        "se esperaba read_only_sql_transaction y llegó: {error}"
    );

    let error = session
        .run(
            &format!("CREATE TABLE {schema}.otra (id int)"),
            Limits::default(),
        )
        .await
        .expect_err("el DDL tenía que ser rechazado");
    assert!(
        matches!(&error, Error::Database { code, .. } if code == "25006"),
        "se esperaba read_only_sql_transaction y llegó: {error}"
    );

    // El otro parámetro de arranque tiene que haber sobrevivido a la mezcla.
    let outcome = session
        .run("SHOW statement_timeout", Limits::default())
        .await
        .expect("no se pudo leer statement_timeout");
    assert_eq!(rows_of(&outcome)[0][0].as_deref(), Some("30s"));
}
