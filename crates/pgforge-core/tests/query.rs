//! Ejecución de SQL contra servidores reales.
//!
//! Mismo criterio que `schema.rs`: corre contra todas las instancias de `PGFORGE_TEST_URLS` para
//! verificar de una pasada que el comportamiento es igual en el rango de versiones soportado. Lo
//! que se prueba acá no se puede probar sin servidor: cómo llegan los tipos formateados, dónde cae
//! la posición de un error, y que una consulta larga se pueda cancelar de verdad.

use std::sync::Arc;
use std::time::Duration;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::sql::{self, Limits, Outcome, QuerySession};
use pgforge_core::Error;

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
    format!("pgforge_query_{}", std::process::id())
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
async fn ejecuta_consultas_contra_servidores_reales() {
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

        let outcome = {
            let handle = Arc::clone(&handle);
            let schema = schema.clone();
            tokio::spawn(async move {
                devuelve_los_valores_formateados(&handle).await;
                distingue_filas_de_comandos(&handle, &schema).await;
                corta_el_script_donde_falla(&handle, &schema).await;
                ubica_el_error_en_el_texto(&handle).await;
                recorta_los_resultados_grandes(&handle).await;
                cancela_una_consulta_larga(&handle).await;
                mantiene_el_estado_entre_ejecuciones(&handle).await;
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

async fn limpiar(handle: &ServerHandle, schema: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .await;
    }
}

/// El protocolo simple existe justamente para esto: que un enum, un compuesto o un `bytea` lleguen
/// mostrables sin que pgforge sepa decodificarlos.
async fn devuelve_los_valores_formateados(handle: &ServerHandle) {
    let session = session(handle).await;

    let outcome = session
        .run(
            "SELECT NULL::text            AS nulo,
                    ''::text              AS vacio,
                    12345.6789::numeric   AS numero,
                    'ab'::bytea           AS binario,
                    ARRAY[1, 2, 3]        AS arreglo,
                    ROW(1, 'x')           AS compuesto,
                    '{\"a\": 1}'::jsonb   AS documento",
            Limits::default(),
        )
        .await
        .expect("no se pudo ejecutar la consulta de tipos");

    let Outcome::Rows {
        columns,
        rows,
        row_count,
        truncated,
        ..
    } = &outcome
    else {
        panic!("se esperaban filas");
    };

    assert_eq!(*row_count, 1);
    assert!(!truncated);
    assert_eq!(columns[0], "nulo");

    let fila = &rows[0];
    assert_eq!(fila[0], None, "el NULL tiene que llegar como ausencia");
    assert_eq!(
        fila[1],
        Some(String::new()),
        "la cadena vacía no es un NULL, y confundirlas haría mentir a la grilla"
    );
    assert_eq!(fila[2].as_deref(), Some("12345.6789"));
    assert_eq!(fila[3].as_deref(), Some("\\x6162"));
    assert_eq!(fila[4].as_deref(), Some("{1,2,3}"));
    assert_eq!(fila[5].as_deref(), Some("(1,x)"));
    assert!(fila[6].as_deref().unwrap_or_default().contains("\"a\""));
}

async fn distingue_filas_de_comandos(handle: &ServerHandle, schema: &str) {
    let session = session(handle).await;

    for statement in [
        format!("DROP SCHEMA IF EXISTS {schema} CASCADE"),
        format!("CREATE SCHEMA {schema}"),
        format!("CREATE TABLE {schema}.t (id int, nota text)"),
        format!("INSERT INTO {schema}.t VALUES (1, 'uno'), (2, 'dos'), (3, NULL)"),
    ] {
        let outcome = session
            .run(&statement, Limits::default())
            .await
            .unwrap_or_else(|e| panic!("falló «{statement}»: {e}"));

        if let Outcome::Rows { .. } = outcome {
            panic!("«{statement}» no devuelve filas");
        }
    }

    let insertadas = session
        .run(
            &format!("UPDATE {schema}.t SET nota = 'x' WHERE id <= 2"),
            Limits::default(),
        )
        .await
        .unwrap();
    match insertadas {
        Outcome::Command { tag, affected, .. } => {
            assert_eq!(tag, "UPDATE");
            assert_eq!(affected, 2, "el UPDATE tiene que informar cuántas tocó");
        }
        Outcome::Rows { .. } => panic!("un UPDATE sin RETURNING no devuelve filas"),
    }

    // Un SELECT sin resultados sigue siendo un SELECT: la interfaz debe mostrar una grilla vacía,
    // no un mensaje de comando ejecutado.
    let vacio = session
        .run(
            &format!("SELECT * FROM {schema}.t WHERE false"),
            Limits::default(),
        )
        .await
        .unwrap();
    let Outcome::Rows { columns, rows, .. } = &vacio else {
        panic!("un SELECT sin filas sigue teniendo columnas");
    };
    assert_eq!(columns, &["id", "nota"]);
    assert!(rows.is_empty());
}

/// Un error corta el resto del script: seguir dejaría un script a medio aplicar.
async fn corta_el_script_donde_falla(handle: &ServerHandle, schema: &str) {
    let session = session(handle).await;
    let script = format!(
        "INSERT INTO {schema}.t VALUES (10, 'diez');
         INSERT INTO {schema}.t VALUES (11, 'once');
         INSERT INTO {schema}.t VALUES (12, 'doce') WHERE;
         INSERT INTO {schema}.t VALUES (13, 'trece');"
    );

    let statements = sql::split(&script);
    assert_eq!(statements.len(), 4);

    let mut ejecutadas = 0;
    for statement in &statements {
        if session
            .run(&statement.text, Limits::default())
            .await
            .is_err()
        {
            break;
        }
        ejecutadas += 1;
    }
    assert_eq!(ejecutadas, 2, "la tercera falla y la cuarta no debe correr");

    let total = session
        .run(
            &format!("SELECT count(*)::int FROM {schema}.t WHERE id >= 10"),
            Limits::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        rows_of(&total)[0][0].as_deref(),
        Some("2"),
        "las dos primeras sí se aplicaron"
    );
}

/// La posición del error viene relativa a la sentencia; sumada al desplazamiento que devuelve
/// `split` tiene que caer sobre el carácter que la rompió dentro del script completo.
async fn ubica_el_error_en_el_texto(handle: &ServerHandle) {
    let session = session(handle).await;
    let script = "SELECT 1;\nSELECT * FROM no_existe_esta_tabla;";

    let statements = sql::split(script);
    let error = session
        .run(&statements[1].text, Limits::default())
        .await
        .expect_err("la tabla no existe");

    let Error::Database { position, code, .. } = &error else {
        panic!("se esperaba un error del servidor y llegó: {error}");
    };
    assert_eq!(code, "42P01", "«relation does not exist»");

    let position = position.expect("el servidor informa dónde está el problema");
    // `position` viene con base 1 sobre la sentencia; el desplazamiento con base 0 sobre el script.
    let en_el_script = statements[1].offset + position as usize - 1;
    let resto: String = script.chars().skip(en_el_script).collect();
    assert!(
        resto.starts_with("no_existe_esta_tabla"),
        "la marca caería sobre «{}»",
        resto.chars().take(20).collect::<String>()
    );
}

async fn recorta_los_resultados_grandes(handle: &ServerHandle) {
    let session = session(handle).await;

    let outcome = session
        .run(
            "SELECT * FROM generate_series(1, 50000)",
            Limits { max_rows: 100 },
        )
        .await
        .expect("no se pudo ejecutar la consulta grande");

    let Outcome::Rows {
        rows,
        row_count,
        truncated,
        ..
    } = &outcome
    else {
        panic!("se esperaban filas");
    };

    assert_eq!(rows.len(), 100, "se guarda hasta el techo pedido");
    assert_eq!(*row_count, 50_000, "y se informa cuántas había en realidad");
    assert!(truncated);

    // Lo importante del recorte: la conexión queda usable, no a medio leer.
    let despues = session.run("SELECT 1", Limits::default()).await.unwrap();
    assert_eq!(rows_of(&despues)[0][0].as_deref(), Some("1"));
}

async fn cancela_una_consulta_larga(handle: &ServerHandle) {
    let session = session(handle).await;
    let token = session.cancel_token();

    // Las dos mitades corren a la vez sobre el mismo préstamo: la cancelación tiene que viajar por
    // otra conexión justo mientras la primera está ocupada, que es lo único difícil del asunto.
    let cancelar = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        handle.cancel(&token).await
    };
    let (resultado, cancelacion) = tokio::join!(
        session.run("SELECT pg_sleep(30)", Limits::default()),
        cancelar
    );

    cancelacion.expect("no se pudo cancelar");
    let error = resultado.expect_err("la consulta tenía que quedar cancelada");
    assert!(
        matches!(error, Error::Canceled),
        "una cancelación pedida por el usuario no es una falla: {error}"
    );
}

/// La sesión es de la pestaña, no de la ejecución: un `SET` o una tabla temporal tienen que seguir
/// valiendo en la consulta siguiente.
async fn mantiene_el_estado_entre_ejecuciones(handle: &ServerHandle) {
    let session = session(handle).await;

    session
        .run("CREATE TEMP TABLE temporal (id int)", Limits::default())
        .await
        .unwrap();
    session
        .run("INSERT INTO temporal VALUES (7)", Limits::default())
        .await
        .unwrap();

    let outcome = session
        .run("SELECT id FROM temporal", Limits::default())
        .await
        .expect("la tabla temporal se perdió entre ejecuciones");
    assert_eq!(rows_of(&outcome)[0][0].as_deref(), Some("7"));
}
