//! Lectura y edición de datos contra servidores reales.
//!
//! Lo que se verifica acá no se puede verificar sin servidor: que la paginación por clave no
//! saltee ni repita una sola fila, que el `UPDATE` con guarda de concurrencia detecte de verdad a
//! otro que tocó la fila, y que un lote fallido no deje nada aplicado.

use std::collections::BTreeMap;
use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::data::{self, Change, Cursor, TableShape, Values};
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
    format!("pgforge_data_{}", std::process::id())
}

const FIXTURE: &str = r#"
CREATE TYPE {s}.estado AS ENUM ('nuevo', 'activo', 'baja');

CREATE TABLE {s}.clientes (
    id      bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    nombre  text NOT NULL,
    apodo   text,
    estado  {s}.estado NOT NULL DEFAULT 'nuevo',
    total   numeric(12,2),
    creado  timestamptz,
    etiquetas text[],
    firma   bytea
);

INSERT INTO {s}.clientes (nombre) SELECT 'cliente ' || n FROM generate_series(1, 5000) n;

-- Clave compuesta, para la paginación por más de una columna.
CREATE TABLE {s}.ventas (
    anio  int  NOT NULL,
    folio int  NOT NULL,
    monto numeric(12,2),
    PRIMARY KEY (anio, folio)
);
INSERT INTO {s}.ventas (anio, folio)
    SELECT 2024 + (n / 700), n FROM generate_series(1, 2100) n;

-- Sin clave primaria ni índice único: tiene que quedar en solo lectura.
CREATE TABLE {s}.suelta (valor text);

-- Índice único sobre una columna que admite nulos: no sirve como identidad.
CREATE TABLE {s}.floja (codigo text UNIQUE, nota text);
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

async fn oid_of(handle: &ServerHandle, schema: &str, table: &str) -> u32 {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(
            "SELECT c.oid FROM pg_class c
               JOIN pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = $1 AND c.relname = $2",
            &[&schema, &table],
        )
        .await
        .unwrap_or_else(|e| panic!("no se encontró {schema}.{table}: {e}"))
        .get(0)
}

async fn shape_of(handle: &ServerHandle, schema: &str, table: &str) -> TableShape {
    let oid = oid_of(handle, schema, table).await;
    data::shape(handle, handle.default_database(), oid)
        .await
        .unwrap_or_else(|e| panic!("no se pudo leer la forma de {schema}.{table}: {e}"))
}

fn values(pairs: &[(&str, Option<&str>)]) -> Values {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.map(str::to_owned)))
        .collect()
}

#[tokio::test]
async fn lee_y_edita_contra_servidores_reales() {
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
                reconoce_la_clave(&handle, &schema).await;
                recorre_todas_las_paginas(&handle, &schema).await;
                recorre_con_clave_compuesta(&handle, &schema).await;
                ordena_y_filtra_contra_el_servidor(&handle, &schema).await;
                da_de_alta_modifica_y_borra(&handle, &schema).await;
                escribe_cualquier_tipo(&handle, &schema).await;
                detecta_que_otro_toco_la_fila(&handle, &schema).await;
                un_lote_fallido_no_deja_nada(&handle, &schema).await;
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

async fn reconoce_la_clave(handle: &ServerHandle, schema: &str) {
    let clientes = shape_of(handle, schema, "clientes").await;
    assert_eq!(clientes.key_columns(), ["id"]);
    assert!(clientes.editable(), "{:?}", clientes.read_only);
    assert!(
        clientes.column("id").unwrap().generated,
        "la identidad se calcula en el servidor y no se puede escribir"
    );

    let ventas = shape_of(handle, schema, "ventas").await;
    assert_eq!(ventas.key_columns(), ["anio", "folio"]);

    let suelta = shape_of(handle, schema, "suelta").await;
    assert!(suelta.key.is_none());
    assert!(
        suelta.read_only.is_some(),
        "sin clave la grilla tiene que decir por qué no se puede editar"
    );

    // Un índice único sobre una columna que admite nulos no identifica nada: en PostgreSQL dos
    // filas con NULL no se consideran iguales, así que el valor puede repetirse.
    let floja = shape_of(handle, schema, "floja").await;
    assert!(
        floja.key.is_none(),
        "un único sobre columna nullable no sirve como clave"
    );
}

/// Lo que tiene que salir bien de la paginación por clave: cada fila exactamente una vez.
async fn recorre_todas_las_paginas(handle: &ServerHandle, schema: &str) {
    let shape = shape_of(handle, schema, "clientes").await;
    let database = handle.default_database();

    let mut vistas: Vec<String> = Vec::new();
    let mut cursor: Option<Cursor> = None;
    let mut paginas = 0;

    loop {
        let page = data::page(
            handle,
            database,
            &shape,
            cursor.as_ref(),
            300,
            &data::PageView::default(),
        )
        .await
        .expect("no se pudo traer la página");

        vistas.extend(page.rows.iter().map(|row| row[0].clone().unwrap()));
        paginas += 1;

        match page.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
        assert!(paginas < 50, "el cursor no avanza: bucle infinito");
    }

    assert_eq!(vistas.len(), 5_000, "faltan o sobran filas");
    let unicas: std::collections::HashSet<&String> = vistas.iter().collect();
    assert_eq!(unicas.len(), 5_000, "alguna fila salió repetida");
    assert!(paginas > 1, "el fixture tenía que necesitar varias páginas");
}

/// Ordenar y filtrar del lado del servidor: lo que no se puede comprobar con el SQL armado a mano
/// es que la paginación siga sin repetir ni saltear filas cuando el orden ya no es el de la clave.
async fn ordena_y_filtra_contra_el_servidor(handle: &ServerHandle, schema: &str) {
    let shape = shape_of(handle, schema, "clientes").await;
    let database = handle.default_database();

    // Descendente por identidad: la primera fila tiene que ser la última que se insertó.
    let vista = data::PageView {
        order: Some(data::PageOrder {
            column: "id".to_owned(),
            descending: true,
        }),
        filter: None,
    };
    let page = data::page(handle, database, &shape, None, 10, &vista)
        .await
        .expect("no se pudo ordenar contra el servidor");
    assert_eq!(page.rows[0][0].as_deref(), Some("5000"));

    // Con un orden elegido no vale el cursor por clave: la página siguiente va por posición.
    assert!(
        matches!(page.next, Some(Cursor::Offset { rows: 10 })),
        "con orden propio hay que paginar por posición: {:?}",
        page.next
    );

    // El filtro lo resuelve el servidor, así que alcanza a filas que la primera tanda ni tocó.
    let filtrada = data::PageView {
        order: None,
        filter: Some("nombre = 'cliente 4999'".to_owned()),
    };
    let page = data::page(handle, database, &shape, None, 200, &filtrada)
        .await
        .expect("no se pudo filtrar contra el servidor");
    assert_eq!(
        page.rows.len(),
        1,
        "el filtro tenía que dejar una sola fila"
    );
    assert_eq!(page.rows[0][0].as_deref(), Some("4999"));
    assert!(page.next.is_none());

    // Ordenar por una columna que no existe se rechaza antes de llegar al servidor.
    let inventada = data::PageView {
        order: Some(data::PageOrder {
            column: "apellido".to_owned(),
            descending: false,
        }),
        filter: None,
    };
    assert!(data::page(handle, database, &shape, None, 10, &inventada)
        .await
        .is_err());

    // Un predicado inválido lo rechaza el servidor, con su propio error.
    let rota = data::PageView {
        order: None,
        filter: Some("no_existe = 1".to_owned()),
    };
    assert!(matches!(
        data::page(handle, database, &shape, None, 10, &rota).await,
        Err(Error::Database { .. })
    ));

    // Recorrer entero con orden propio: ni una fila repetida ni una que falte.
    let orden_texto = data::PageView {
        order: Some(data::PageOrder {
            column: "nombre".to_owned(),
            descending: false,
        }),
        filter: Some("id <= 1000".to_owned()),
    };
    let mut vistas: Vec<String> = Vec::new();
    let mut cursor: Option<Cursor> = None;
    loop {
        let page = data::page(handle, database, &shape, cursor.as_ref(), 300, &orden_texto)
            .await
            .unwrap();
        vistas.extend(page.rows.iter().map(|row| row[0].clone().unwrap()));
        match page.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
        assert!(vistas.len() <= 1000, "el cursor no termina de avanzar");
    }
    let unicas: std::collections::HashSet<&String> = vistas.iter().collect();
    assert_eq!(vistas.len(), 1000, "faltan o sobran filas con orden propio");
    assert_eq!(unicas.len(), 1000, "alguna fila salió repetida");
}

async fn recorre_con_clave_compuesta(handle: &ServerHandle, schema: &str) {
    let shape = shape_of(handle, schema, "ventas").await;
    let database = handle.default_database();

    let mut vistas: Vec<(String, String)> = Vec::new();
    let mut cursor: Option<Cursor> = None;

    loop {
        let page = data::page(
            handle,
            database,
            &shape,
            cursor.as_ref(),
            250,
            &data::PageView::default(),
        )
        .await
        .unwrap();
        vistas.extend(
            page.rows
                .iter()
                .map(|row| (row[0].clone().unwrap(), row[1].clone().unwrap())),
        );

        match page.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }

    assert_eq!(vistas.len(), 2_100);
    let unicas: std::collections::HashSet<&(String, String)> = vistas.iter().collect();
    assert_eq!(
        unicas.len(),
        2_100,
        "comparar columna por columna en vez de por fila entera saltea o repite filas"
    );

    // La comparación por fila entera también tiene que respetar el orden.
    let mut ordenadas = vistas.clone();
    ordenadas
        .sort_by_key(|(anio, folio)| (anio.parse::<i64>().unwrap(), folio.parse::<i64>().unwrap()));
    assert_eq!(vistas, ordenadas, "las filas no vinieron ordenadas");
}

async fn da_de_alta_modifica_y_borra(handle: &ServerHandle, schema: &str) {
    let shape = shape_of(handle, schema, "clientes").await;
    let database = handle.default_database();

    data::apply(
        handle,
        database,
        &shape,
        &[Change::Insert {
            values: values(&[("nombre", Some("Ana")), ("apodo", None)]),
        }],
    )
    .await
    .expect("no se pudo insertar");

    let id = id_de(handle, schema, "Ana").await;

    let applied = data::apply(
        handle,
        database,
        &shape,
        &[Change::Update {
            key: vec![id.clone()],
            original: values(&[("nombre", Some("Ana")), ("apodo", None)]),
            changes: values(&[("nombre", Some("Ana María")), ("apodo", Some("Anita"))]),
        }],
    )
    .await
    .expect("no se pudo modificar");
    assert_eq!(applied.updated, 1);

    let fila = fila_de(handle, schema, &id).await;
    assert_eq!(fila.get("nombre").unwrap().as_deref(), Some("Ana María"));
    assert_eq!(fila.get("apodo").unwrap().as_deref(), Some("Anita"));

    data::apply(
        handle,
        database,
        &shape,
        &[Change::Delete {
            key: vec![id.clone()],
        }],
    )
    .await
    .expect("no se pudo borrar");

    assert!(
        !existe(handle, schema, &id).await,
        "la fila tenía que quedar borrada"
    );
}

/// El doble casteo `::text::tipo` tiene que servir para todo, no solo para texto y números.
async fn escribe_cualquier_tipo(handle: &ServerHandle, schema: &str) {
    let shape = shape_of(handle, schema, "clientes").await;
    let database = handle.default_database();

    data::apply(
        handle,
        database,
        &shape,
        &[Change::Insert {
            values: values(&[
                ("nombre", Some("Tipos")),
                ("estado", Some("activo")),
                ("total", Some("12345.67")),
                ("creado", Some("2026-07-31 22:00:00+00")),
                ("etiquetas", Some("{uno,dos}")),
                ("firma", Some("\\x6162")),
            ]),
        }],
    )
    .await
    .expect("no se pudo insertar con todos los tipos");

    let id = id_de(handle, schema, "Tipos").await;
    let fila = fila_de(handle, schema, &id).await;

    assert_eq!(fila.get("estado").unwrap().as_deref(), Some("activo"));
    assert_eq!(fila.get("total").unwrap().as_deref(), Some("12345.67"));
    assert_eq!(fila.get("etiquetas").unwrap().as_deref(), Some("{uno,dos}"));
    assert_eq!(fila.get("firma").unwrap().as_deref(), Some("\\x6162"));
    assert!(fila
        .get("creado")
        .unwrap()
        .as_deref()
        .unwrap_or_default()
        .starts_with("2026-07-31"));

    // Y volver a dejar una columna en NULL también tiene que poder hacerse.
    data::apply(
        handle,
        database,
        &shape,
        &[Change::Update {
            key: vec![id.clone()],
            original: values(&[("total", Some("12345.67"))]),
            changes: values(&[("total", None)]),
        }],
    )
    .await
    .expect("no se pudo vaciar la columna");

    assert_eq!(
        fila_de(handle, schema, &id).await.get("total").unwrap(),
        &None
    );
}

async fn detecta_que_otro_toco_la_fila(handle: &ServerHandle, schema: &str) {
    let shape = shape_of(handle, schema, "clientes").await;
    let database = handle.default_database();

    data::apply(
        handle,
        database,
        &shape,
        &[Change::Insert {
            values: values(&[("nombre", Some("Disputada"))]),
        }],
    )
    .await
    .unwrap();
    let id = id_de(handle, schema, "Disputada").await;

    // Otro cambia la fila por su cuenta, como haría otra sesión.
    let client = handle.client(database).await.unwrap();
    client
        .execute(
            &format!("UPDATE {schema}.clientes SET nombre = 'La cambió otro' WHERE id = $1::text::bigint"),
            &[&id],
        )
        .await
        .unwrap();

    let error = data::apply(
        handle,
        database,
        &shape,
        &[Change::Update {
            key: vec![id.clone()],
            original: values(&[("nombre", Some("Disputada"))]),
            changes: values(&[("nombre", Some("Mi versión"))]),
        }],
    )
    .await
    .expect_err("tenía que detectar el conflicto en vez de pisar el cambio ajeno");

    assert!(
        matches!(error, Error::Conflict(_)),
        "un conflicto no es un error del servidor ni del usuario: {error}"
    );

    let fila = fila_de(handle, schema, &id).await;
    assert_eq!(
        fila.get("nombre").unwrap().as_deref(),
        Some("La cambió otro"),
        "el cambio ajeno tenía que quedar en pie"
    );
}

async fn un_lote_fallido_no_deja_nada(handle: &ServerHandle, schema: &str) {
    let shape = shape_of(handle, schema, "clientes").await;
    let database = handle.default_database();

    let error = data::apply(
        handle,
        database,
        &shape,
        &[
            Change::Insert {
                values: values(&[("nombre", Some("Lote uno"))]),
            },
            Change::Insert {
                values: values(&[("nombre", Some("Lote dos"))]),
            },
            // La tercera falla: `nombre` es NOT NULL.
            Change::Insert {
                values: values(&[("nombre", None)]),
            },
        ],
    )
    .await
    .expect_err("la tercera tenía que fallar");

    assert!(!matches!(error, Error::Conflict(_)), "{error}");
    assert_eq!(
        contar(handle, schema, "Lote %").await,
        0,
        "las dos primeras del lote tenían que revertirse con la transacción"
    );
}

// --------------------------------------------------------------------------
// Ayudantes de comprobación, que consultan por su cuenta para no verificar el
// código contra sí mismo.
// --------------------------------------------------------------------------

async fn id_de(handle: &ServerHandle, schema: &str, nombre: &str) -> String {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(
            &format!("SELECT id::text FROM {schema}.clientes WHERE nombre = $1"),
            &[&nombre],
        )
        .await
        .unwrap_or_else(|e| panic!("no se encontró la fila «{nombre}»: {e}"))
        .get(0)
}

async fn fila_de(
    handle: &ServerHandle,
    schema: &str,
    id: &str,
) -> BTreeMap<String, Option<String>> {
    let client = handle.client(handle.default_database()).await.unwrap();
    let row = client
        .query_one(
            &format!(
                "SELECT nombre::text, apodo::text, estado::text, total::text,
                        creado::text, etiquetas::text, firma::text
                   FROM {schema}.clientes WHERE id = $1::text::bigint"
            ),
            &[&id],
        )
        .await
        .unwrap();

    [
        "nombre",
        "apodo",
        "estado",
        "total",
        "creado",
        "etiquetas",
        "firma",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, name)| (name.to_owned(), row.get(index)))
    .collect()
}

async fn existe(handle: &ServerHandle, schema: &str, id: &str) -> bool {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(
            &format!("SELECT count(*)::int8 FROM {schema}.clientes WHERE id = $1::text::bigint"),
            &[&id],
        )
        .await
        .unwrap()
        .get::<_, i64>(0)
        > 0
}

async fn contar(handle: &ServerHandle, schema: &str, patron: &str) -> i64 {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_one(
            &format!("SELECT count(*)::int8 FROM {schema}.clientes WHERE nombre LIKE $1"),
            &[&patron],
        )
        .await
        .unwrap()
        .get(0)
}
