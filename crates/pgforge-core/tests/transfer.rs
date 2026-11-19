//! Saltos por clave foránea, origen de un resultado y copia de tablas contra servidores reales.
//!
//! Lo que no se puede verificar sin servidor: que el catálogo devuelva las claves foráneas en el
//! orden de la restricción, que `prepare` diga de qué tabla sale cada columna, y que la copia cree la
//! tabla, mueva las filas y se deshaga entera cuando algo falla.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::data::{self, TransferMode, TransferRoute, TransferSpec};
use pgforge_core::sql::{completion, QuerySession};
use pgforge_core::Error;
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

async fn exec(handle: &ServerHandle, sql: &str) {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .batch_execute(sql)
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"));
}

async fn count(handle: &ServerHandle, table: &str) -> i64 {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(&format!("SELECT count(*) FROM {table}"), &[])
        .await
        .unwrap()
        .get(0)
}

#[tokio::test]
async fn claves_foraneas_origen_de_un_resultado_y_copia_de_tablas() {
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
        let origen = format!("pgforge_tr_o_{}", std::process::id());
        let destino = format!("pgforge_tr_d_{}", std::process::id());
        exec(
            &handle,
            &format!(
                "DROP SCHEMA IF EXISTS {origen} CASCADE; DROP SCHEMA IF EXISTS {destino} CASCADE;
                 CREATE SCHEMA {origen}; CREATE SCHEMA {destino};
                 CREATE TABLE {origen}.clientes (
                     id bigint PRIMARY KEY, nombre text NOT NULL, estado text DEFAULT 'nuevo',
                     CHECK (id > 0));
                 CREATE INDEX clientes_nombre_idx ON {origen}.clientes (nombre);
                 INSERT INTO {origen}.clientes (id, nombre) SELECT n, 'c' || n FROM generate_series(1, 1000) n;
                 CREATE TABLE {origen}.pedidos (
                     id bigint PRIMARY KEY,
                     cliente_id bigint NOT NULL REFERENCES {origen}.clientes (id));"
            ),
        )
        .await;

        let outcome = {
            let handle = Arc::clone(&handle);
            let (origen, destino) = (origen.clone(), destino.clone());
            tokio::spawn(async move {
                claves_foraneas(&handle, &origen).await;
                origen_de_un_resultado(&handle, &origen).await;
                copia_de_tablas(&handle, &origen, &destino).await;
            })
            .await
        };

        exec(
            &handle,
            &format!(
                "DROP SCHEMA IF EXISTS {origen} CASCADE; DROP SCHEMA IF EXISTS {destino} CASCADE"
            ),
        )
        .await;
        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({url})");
    }
}

async fn claves_foraneas(handle: &ServerHandle, schema: &str) {
    let db = handle.default_database();
    let pedidos = data::shape_by_name(handle, db, schema, "pedidos")
        .await
        .unwrap();
    assert_eq!(pedidos.foreign_keys.len(), 1);
    assert_eq!(pedidos.foreign_keys[0].columns, ["cliente_id"]);
    assert_eq!(pedidos.foreign_keys[0].table, "clientes");
    assert_eq!(pedidos.foreign_keys[0].other_columns, ["id"]);
    assert!(pedidos.referenced_by.is_empty());

    let clientes = data::shape_by_name(handle, db, schema, "clientes")
        .await
        .unwrap();
    assert_eq!(clientes.referenced_by.len(), 1);
    assert_eq!(clientes.referenced_by[0].table, "pedidos");
    // Vista desde la referida: las columnas propias son las de la clave que se apunta.
    assert_eq!(clientes.referenced_by[0].columns, ["id"]);
    assert_eq!(clientes.referenced_by[0].other_columns, ["cliente_id"]);

    let snapshot = completion::snapshot(handle, db).await.unwrap();
    assert!(snapshot
        .foreign_keys
        .iter()
        .any(|fk| fk.schema == schema && fk.table == "pedidos" && fk.ref_table == "clientes"));
}

async fn origen_de_un_resultado(handle: &ServerHandle, schema: &str) {
    let session = QuerySession::open(handle, handle.default_database())
        .await
        .unwrap();

    let source = session
        .edit_source(&format!(
            "SELECT id, nombre, upper(nombre) AS mayus FROM {schema}.clientes"
        ))
        .await
        .unwrap();
    assert_eq!(
        source.columns,
        [Some("id".to_owned()), Some("nombre".to_owned()), None],
        "una expresión no tiene tabla de origen"
    );

    let varias = session
        .edit_source(&format!(
            "SELECT c.id, p.id FROM {schema}.clientes c JOIN {schema}.pedidos p ON p.cliente_id = c.id"
        ))
        .await;
    assert!(varias.is_err(), "dos tablas de origen no se editan");

    let calculado = session.edit_source("SELECT 1").await;
    assert!(calculado.is_err());
}

fn spec(origen: &str, destino: &str, mode: TransferMode) -> TransferSpec {
    TransferSpec {
        schema: origen.to_owned(),
        table: "clientes".into(),
        target_schema: destino.to_owned(),
        target_table: "clientes".into(),
        mode,
        with_data: true,
        with_indexes: mode == TransferMode::Create,
        filter: None,
    }
}

async fn copiar(handle: &ServerHandle, spec: &TransferSpec) -> Result<Option<u64>, Error> {
    let db = handle.default_database();
    let plan = data::transfer_prepare(handle, db, spec).await?;
    let (progress, _rx) = mpsc::channel(64);
    let (_cancel, cancelled) = oneshot::channel();
    data::transfer_run(
        TransferRoute {
            source: handle,
            source_database: db,
            target: handle,
            target_database: db,
        },
        spec,
        &plan,
        progress,
        cancelled,
    )
    .await
    .map(|outcome| outcome.rows)
}

async fn copia_de_tablas(handle: &ServerHandle, origen: &str, destino: &str) {
    let tabla = format!("{destino}.clientes");

    let filas = copiar(handle, &spec(origen, destino, TransferMode::Create))
        .await
        .unwrap();
    assert_eq!(filas, Some(1000));
    assert_eq!(count(handle, &tabla).await, 1000);

    // Estructura: la clave primaria y el índice llegaron.
    let client = handle.client(handle.default_database()).await.unwrap();
    let indices: i64 = client
        .query_one(
            "SELECT count(*) FROM pg_indexes WHERE schemaname = $1 AND tablename = 'clientes'",
            &[&destino],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(indices, 2, "la clave primaria y el índice por nombre");

    // Crear sobre una tabla que ya existe no toca nada.
    let repetido = copiar(handle, &spec(origen, destino, TransferMode::Create)).await;
    assert!(matches!(repetido, Err(Error::Conflict(_))), "{repetido:?}");
    assert_eq!(count(handle, &tabla).await, 1000);

    // Agregar con las mismas claves falla entero y no deja filas a medias.
    let choque = copiar(handle, &spec(origen, destino, TransferMode::Append)).await;
    assert!(choque.is_err(), "las claves se repiten");
    assert_eq!(count(handle, &tabla).await, 1000);

    // Reemplazar con filtro: queda solo lo filtrado.
    let mut parcial = spec(origen, destino, TransferMode::Truncate);
    parcial.filter = Some("id <= 10".into());
    assert_eq!(copiar(handle, &parcial).await.unwrap(), Some(10));
    assert_eq!(count(handle, &tabla).await, 10);

    // Un filtro inválido falla sin vaciar: el TRUNCATE va en la misma transacción.
    parcial.filter = Some("columna_que_no_existe = 1".into());
    assert!(copiar(handle, &parcial).await.is_err());
    assert_eq!(count(handle, &tabla).await, 10);
}
