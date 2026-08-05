//! Exportar e importar datos con `COPY` contra servidores reales.
//!
//! Lo que no se puede verificar sin servidor: que una tabla exportada y vuelta a importar quede
//! idéntica —incluidos los NULL y un valor con comas y comillas que obliga a citar en CSV—, que se
//! pueda exportar el resultado de una consulta, y que un `COPY FROM` con una fila inválida no deje
//! nada a medias, porque una sola sentencia entra entera o no entra.

use std::path::PathBuf;
use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::data::{self, CopyFormat, ExportSource, ExportSpec, ImportSpec, TextOptions};
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
    format!("pgforge_io_{}", std::process::id())
}

const FIXTURE: &str = r#"
CREATE TABLE {s}.origen (
    id     int  PRIMARY KEY,
    nombre text,
    nota   text
);
INSERT INTO {s}.origen (id, nombre, nota)
    SELECT n, 'cliente ' || n, CASE WHEN n % 3 = 0 THEN NULL ELSE 'nota ' || n END
      FROM generate_series(1, 500) n;

-- Un valor que en CSV obliga a citar: coma, comillas y un salto de línea.
INSERT INTO {s}.origen (id, nombre, nota)
    VALUES (0, 'a,b"c', E'con\nsalto');

-- Copia vacía de la misma forma, destino de la importación.
CREATE TABLE {s}.destino (LIKE {s}.origen INCLUDING ALL);

-- Una sola columna entera, para exportar el resultado de una consulta y para probar una importación
-- inválida.
CREATE TABLE {s}.numeros (n int);
"#;

async fn setup(handle: &ServerHandle, schema: &str) {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema};"
        ))
        .await
        .expect("no se pudo crear el esquema de prueba");
    client
        .batch_execute(&FIXTURE.replace("{s}", schema))
        .await
        .expect("no se pudo crear el fixture");
}

async fn teardown(handle: &ServerHandle, schema: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .await;
    }
}

#[tokio::test]
async fn exporta_e_importa_contra_servidores_reales() {
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
        setup(&handle, &schema).await;

        let outcome = {
            let handle = Arc::clone(&handle);
            let schema = schema.clone();
            tokio::spawn(async move {
                ida_y_vuelta_conserva_todo(&handle, &schema).await;
                exporta_el_resultado_de_una_consulta(&handle, &schema).await;
                un_import_invalido_no_deja_nada(&handle, &schema).await;
            })
            .await
        };

        teardown(&handle, &schema).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({url})");
    }
}

/// Exportar una tabla y reimportarla en otra vacía tiene que dejarlas idénticas.
async fn ida_y_vuelta_conserva_todo(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database();
    let path = temp_path("origen");

    let export = data::export_to_file(
        handle,
        database,
        &ExportSpec {
            source: ExportSource::Table {
                schema: schema.to_owned(),
                table: "origen".to_owned(),
                columns: vec![],
            },
            format: CopyFormat::Csv,
            options: TextOptions {
                header: true,
                ..Default::default()
            },
        },
        &path,
        drain(),
        never_cancel(),
    )
    .await
    .expect("no se pudo exportar");
    assert!(export.bytes > 0, "el archivo salió vacío");

    let import = data::import_from_file(
        handle,
        database,
        &ImportSpec {
            schema: schema.to_owned(),
            table: "destino".to_owned(),
            columns: vec![],
            format: CopyFormat::Csv,
            options: TextOptions {
                header: true,
                ..Default::default()
            },
        },
        &path,
        drain(),
        never_cancel(),
    )
    .await
    .expect("no se pudo importar");

    let _ = std::fs::remove_file(&path);

    assert_eq!(
        import.rows,
        Some(501),
        "el COPY FROM tiene que informar cuántas filas entraron"
    );

    // Se comparan las dos tablas por su cuenta, no contra el conteo de la importación, para no
    // verificar el código contra sí mismo.
    let diferencias: i64 = scalar(
        handle,
        &format!(
            "SELECT count(*)::int8 FROM (
                 (TABLE {schema}.origen EXCEPT TABLE {schema}.destino)
                 UNION ALL
                 (TABLE {schema}.destino EXCEPT TABLE {schema}.origen)
             ) d"
        ),
    )
    .await;
    assert_eq!(
        diferencias, 0,
        "origen y destino tenían que quedar fila por fila iguales tras el ida y vuelta"
    );

    // El NULL de la nota tiene que seguir siendo NULL y no la cadena «NULL» ni vacía.
    let nulos: i64 = scalar(
        handle,
        &format!("SELECT count(*)::int8 FROM {schema}.destino WHERE nota IS NULL"),
    )
    .await;
    assert!(nulos > 0, "los NULL se perdieron en el ida y vuelta");

    // Y el valor con coma, comillas y salto de línea tiene que reconstruirse igual.
    let raro: Option<String> = scalar_opt(
        handle,
        &format!("SELECT nota FROM {schema}.destino WHERE id = 0"),
    )
    .await;
    assert_eq!(raro.as_deref(), Some("con\nsalto"));
}

/// `COPY (consulta) TO` deja exportar cualquier `SELECT`, no solo una tabla.
async fn exporta_el_resultado_de_una_consulta(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database();
    let path = temp_path("consulta");

    data::export_to_file(
        handle,
        database,
        &ExportSpec {
            source: ExportSource::Query {
                sql: format!("SELECT id FROM {schema}.origen WHERE id BETWEEN 1 AND 10"),
            },
            format: CopyFormat::Csv,
            options: TextOptions::default(),
        },
        &path,
        drain(),
        never_cancel(),
    )
    .await
    .expect("no se pudo exportar la consulta");

    let import = data::import_from_file(
        handle,
        database,
        &ImportSpec {
            schema: schema.to_owned(),
            table: "numeros".to_owned(),
            columns: vec![],
            format: CopyFormat::Csv,
            options: TextOptions::default(),
        },
        &path,
        drain(),
        never_cancel(),
    )
    .await
    .expect("no se pudo importar la consulta exportada");

    let _ = std::fs::remove_file(&path);
    assert_eq!(import.rows, Some(10));
}

/// Un `COPY FROM` es una sola sentencia: si una fila no entra, no entra ninguna.
async fn un_import_invalido_no_deja_nada(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database();
    let path = temp_path("invalido");
    // La segunda línea no es un entero: el COPY entero tiene que fallar.
    std::fs::write(&path, "1\nno-es-un-numero\n3\n").expect("no se pudo escribir el archivo");

    let error = data::import_from_file(
        handle,
        database,
        &ImportSpec {
            schema: schema.to_owned(),
            table: "numeros".to_owned(),
            columns: vec!["n".to_owned()],
            format: CopyFormat::Text,
            options: TextOptions::default(),
        },
        &path,
        drain(),
        never_cancel(),
    )
    .await
    .expect_err("una fila inválida tenía que abortar el COPY");
    let _ = std::fs::remove_file(&path);

    // «numeros» ya traía las 10 filas de la prueba anterior; lo que se verifica es que la
    // importación fallida no sumó ninguna, no que la tabla esté vacía.
    assert!(
        error.to_string().to_lowercase().contains("no-es-un-numero")
            || error.to_string().to_lowercase().contains("integer")
            || error.to_string().to_lowercase().contains("entero"),
        "el error tenía que explicar qué fila no entró: {error}"
    );
    let total: i64 = scalar(
        handle,
        &format!("SELECT count(*)::int8 FROM {schema}.numeros"),
    )
    .await;
    assert_eq!(
        total, 10,
        "el COPY fallido no tenía que dejar ninguna fila cargada"
    );
}

// --------------------------------------------------------------------------

/// Un archivo temporal propio de esta corrida, para no pisar el de otra que corra a la par.
fn temp_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("pgforge_io_{}_{tag}.dat", std::process::id()))
}

/// Consume el avance sin mirarlo: los tests verifican el resultado, no la barra de progreso.
fn drain() -> mpsc::Sender<u64> {
    let (tx, mut rx) = mpsc::channel::<u64>(64);
    tokio::spawn(async move { while rx.recv().await.is_some() {} });
    tx
}

/// Un extremo de cancelación que nunca se dispara.
fn never_cancel() -> oneshot::Receiver<()> {
    let (tx, rx) = oneshot::channel();
    // Se conserva el emisor para siempre para que el receptor no vea el canal cerrado.
    std::mem::forget(tx);
    rx
}

async fn scalar(handle: &ServerHandle, sql: &str) -> i64 {
    let client = handle.client(handle.default_database()).await.unwrap();
    client.query_one(sql, &[]).await.unwrap().get(0)
}

async fn scalar_opt(handle: &ServerHandle, sql: &str) -> Option<String> {
    let client = handle.client(handle.default_database()).await.unwrap();
    client.query_one(sql, &[]).await.unwrap().get(0)
}
