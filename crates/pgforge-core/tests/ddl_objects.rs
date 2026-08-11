//! Secuencias, tipos, dominios, esquemas, particiones y comentarios contra servidores reales.
//!
//! Lo que se verifica acá no se puede verificar sin servidor: que `pg_sequences` devuelva lo mismo
//! que se pidió al crear la secuencia, que `ALTER TYPE … ADD VALUE` sí corra adentro de una
//! transacción en todo el rango soportado —la prohibición se levantó en PG 12 y el piso del
//! proyecto es la 13—, que un dominio con `CHECK` rechace de verdad lo que no cumple, que
//! `ATTACH PARTITION` valide las filas que ya están, y que el comentario vuelva por
//! `obj_description`.
//!
//! Las bases quedan afuera a propósito: `CREATE DATABASE` no corre adentro de una transacción y
//! crear una base por corrida contra cada servidor de `PGFORGE_TEST_URLS` dejaría basura si el test
//! se interrumpe. Esa parte se prueba a mano con la CLI.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::ddl::comment::{self, CommentChange, CommentTarget};
use pgforge_core::ddl::domain::{self, DomainChange, DomainConstraint};
use pgforge_core::ddl::partition::{self, PartitionBound, PartitionChange};
use pgforge_core::ddl::schema::{self, SchemaChange};
use pgforge_core::ddl::sequence::{self, SequenceChange, SequenceOptions, SequenceOwner};
use pgforge_core::ddl::types::{self, Field, TypeChange, TypeKind};

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
    format!("pgforge_objetos_{}", std::process::id())
}

async fn setup(handle: &ServerHandle, schema: &str) {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema};"
        ))
        .await
        .expect("no se pudo crear el esquema de prueba");
}

async fn teardown(handle: &ServerHandle, schema: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .await;
    }
}

async fn relation_oid(handle: &ServerHandle, schema: &str, name: &str) -> Option<u32> {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_opt(
            "SELECT c.oid FROM pg_class c
               JOIN pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = $1 AND c.relname = $2",
            &[&schema, &name],
        )
        .await
        .unwrap()
        .map(|row| row.get(0))
}

async fn type_oid(handle: &ServerHandle, schema: &str, name: &str) -> Option<u32> {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .query_opt(
            "SELECT t.oid FROM pg_type t
               JOIN pg_namespace n ON n.oid = t.typnamespace
              WHERE n.nspname = $1 AND t.typname = $2",
            &[&schema, &name],
        )
        .await
        .unwrap()
        .map(|row| row.get(0))
}

#[tokio::test]
async fn crea_cambia_y_borra_objetos_contra_servidores_reales() {
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
                secuencias(&handle, &schema).await;
                enumeraciones(&handle, &schema).await;
                compuestos(&handle, &schema).await;
                dominios(&handle, &schema).await;
                particiones(&handle, &schema).await;
                comentarios(&handle, &schema).await;
                esquemas(&handle).await;
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

async fn secuencias(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    sequence::apply(
        handle,
        &database,
        &[SequenceChange::CreateSequence {
            schema: schema.to_owned(),
            name: "folios".into(),
            if_not_exists: false,
            options: SequenceOptions {
                data_type: Some("integer".into()),
                increment: Some(5),
                min_value: Some(100),
                max_value: Some(1000),
                start: Some(100),
                cache: Some(1),
                cycle: Some(true),
                owned_by: None,
            },
        }],
    )
    .await
    .expect("tenía que crear la secuencia");

    let oid = relation_oid(handle, schema, "folios")
        .await
        .expect("la secuencia tenía que existir");

    let info = sequence::info(handle, &database, oid)
        .await
        .expect("tenía que leer la secuencia");

    assert_eq!(info.data_type, "integer");
    assert_eq!(info.increment, 5);
    assert_eq!(info.min_value, 100);
    assert_eq!(info.max_value, 1000);
    assert!(info.cycle);
    // Sin haberla usado todavía, `pg_sequences.last_value` es nulo.
    assert_eq!(info.last_value, None);

    // Mover la secuencia y volver a leerla: `RESTART` cambia el valor actual, no el `START WITH`.
    let client = handle.client(&database).await.unwrap();
    client
        .batch_execute(&format!("SELECT nextval('{schema}.folios')"))
        .await
        .expect("tenía que poder consumir un número");

    let info = sequence::info(handle, &database, oid).await.unwrap();
    assert_eq!(info.last_value, Some(100));

    sequence::apply(
        handle,
        &database,
        &[SequenceChange::RestartSequence {
            schema: schema.to_owned(),
            name: "folios".into(),
            value: Some(500),
        }],
    )
    .await
    .expect("tenía que reiniciar la secuencia");

    client
        .batch_execute(&format!("SELECT nextval('{schema}.folios')"))
        .await
        .unwrap();
    let info = sequence::info(handle, &database, oid).await.unwrap();
    assert_eq!(info.last_value, Some(500), "RESTART tenía que moverla");
    assert_eq!(info.start, 100, "RESTART no toca el START WITH");

    // Atarla a una columna: al borrar la tabla se tiene que ir con ella.
    client
        .batch_execute(&format!(
            "CREATE TABLE {schema}.comprobantes (folio integer)"
        ))
        .await
        .unwrap();

    sequence::apply(
        handle,
        &database,
        &[SequenceChange::AlterSequence {
            schema: schema.to_owned(),
            name: "folios".into(),
            options: SequenceOptions {
                owned_by: Some(SequenceOwner::Column {
                    schema: schema.to_owned(),
                    table: "comprobantes".into(),
                    column: "folio".into(),
                }),
                ..SequenceOptions::default()
            },
        }],
    )
    .await
    .expect("tenía que atar la secuencia a la columna");

    let info = sequence::info(handle, &database, oid).await.unwrap();
    let owned = info.owned_by.expect("tenía que quedar atada a la columna");
    assert_eq!(owned.table, "comprobantes");
    assert_eq!(owned.column, "folio");

    client
        .batch_execute(&format!("DROP TABLE {schema}.comprobantes"))
        .await
        .unwrap();
    assert!(
        relation_oid(handle, schema, "folios").await.is_none(),
        "la secuencia atada tenía que irse con la tabla"
    );
}

async fn enumeraciones(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    types::apply(
        handle,
        &database,
        &[TypeChange::CreateEnum {
            schema: schema.to_owned(),
            name: "estado".into(),
            labels: vec!["activo".into(), "inactivo".into()],
        }],
    )
    .await
    .expect("tenía que crear la enumeración");

    let oid = type_oid(handle, schema, "estado")
        .await
        .expect("el tipo tenía que existir");

    // Esto es lo que no se puede probar sin servidor: `apply` mete el cambio en una transacción, y
    // hasta PG 11 `ADD VALUE` no se podía usar ahí adentro. Desde la 12 sí, y el piso del proyecto
    // es la 13, así que tiene que pasar contra todas las versiones soportadas.
    types::apply(
        handle,
        &database,
        &[TypeChange::AddEnumValue {
            schema: schema.to_owned(),
            name: "estado".into(),
            value: "pausado".into(),
            position: Some(pgforge_core::ddl::types::EnumPosition::After {
                value: "activo".into(),
            }),
            if_not_exists: false,
        }],
    )
    .await
    .expect("agregar un valor tenía que funcionar adentro de la transacción");

    let info = types::info(handle, &database, oid).await.unwrap();
    assert_eq!(info.kind, TypeKind::Enum);
    assert_eq!(
        info.labels,
        vec!["activo", "pausado", "inactivo"],
        "el valor nuevo tenía que entrar en la posición pedida"
    );

    types::apply(
        handle,
        &database,
        &[TypeChange::RenameEnumValue {
            schema: schema.to_owned(),
            name: "estado".into(),
            from: "pausado".into(),
            to: "en_pausa".into(),
        }],
    )
    .await
    .expect("tenía que renombrar el valor");

    let info = types::info(handle, &database, oid).await.unwrap();
    assert_eq!(info.labels, vec!["activo", "en_pausa", "inactivo"]);
}

async fn compuestos(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    types::apply(
        handle,
        &database,
        &[TypeChange::CreateComposite {
            schema: schema.to_owned(),
            name: "direccion".into(),
            fields: vec![
                Field {
                    name: "calle".into(),
                    data_type: "text".into(),
                    collation: None,
                },
                Field {
                    name: "numero".into(),
                    data_type: "integer".into(),
                    collation: None,
                },
            ],
        }],
    )
    .await
    .expect("tenía que crear el compuesto");

    let oid = type_oid(handle, schema, "direccion").await.unwrap();

    types::apply(
        handle,
        &database,
        &[
            TypeChange::AddCompositeField {
                schema: schema.to_owned(),
                name: "direccion".into(),
                field: Field {
                    name: "piso".into(),
                    data_type: "text".into(),
                    collation: None,
                },
            },
            TypeChange::AlterCompositeFieldType {
                schema: schema.to_owned(),
                name: "direccion".into(),
                field: "numero".into(),
                data_type: "bigint".into(),
                collation: None,
                cascade: false,
            },
        ],
    )
    .await
    .expect("tenía que agregar y cambiar campos");

    let info = types::info(handle, &database, oid).await.unwrap();
    assert_eq!(info.kind, TypeKind::Composite);
    let campos: Vec<(&str, &str)> = info
        .fields
        .iter()
        .map(|field| (field.name.as_str(), field.data_type.as_str()))
        .collect();
    assert_eq!(
        campos,
        vec![("calle", "text"), ("numero", "bigint"), ("piso", "text")]
    );

    types::apply(
        handle,
        &database,
        &[TypeChange::DropType {
            schema: schema.to_owned(),
            name: "direccion".into(),
            cascade: false,
        }],
    )
    .await
    .expect("tenía que borrar el compuesto");

    assert!(type_oid(handle, schema, "direccion").await.is_none());
}

async fn dominios(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    domain::apply(
        handle,
        &database,
        &[DomainChange::CreateDomain {
            schema: schema.to_owned(),
            name: "positivo".into(),
            data_type: "integer".into(),
            collation: None,
            default: Some("1".into()),
            not_null: true,
            constraints: vec![DomainConstraint {
                name: Some("mayor_que_cero".into()),
                check: "VALUE > 0".into(),
                not_valid: false,
            }],
        }],
    )
    .await
    .expect("tenía que crear el dominio");

    let oid = type_oid(handle, schema, "positivo").await.unwrap();
    let info = domain::info(handle, &database, oid)
        .await
        .expect("tenía que leer el dominio");

    assert_eq!(info.data_type, "integer");
    assert!(info.not_null);
    assert_eq!(info.default.as_deref(), Some("1"));
    assert_eq!(info.constraints.len(), 1);
    assert_eq!(
        info.constraints[0].name.as_deref(),
        Some("mayor_que_cero"),
        "la restricción tenía que conservar su nombre"
    );
    assert!(
        info.constraints[0].check.contains("VALUE > 0"),
        "el CHECK vuelve del servidor sin el envoltorio: {}",
        info.constraints[0].check
    );

    // Que el dominio realmente rechace lo que no cumple es lo único que no se puede afirmar
    // generando el SQL: pide que el servidor lo evalúe.
    let client = handle.client(&database).await.unwrap();
    client
        .batch_execute(&format!(
            "CREATE TABLE {schema}.medidas (v {schema}.positivo)"
        ))
        .await
        .unwrap();

    let rechazado = client
        .batch_execute(&format!("INSERT INTO {schema}.medidas VALUES (-1)"))
        .await;
    assert!(
        rechazado.is_err(),
        "el dominio tenía que rechazar el valor negativo"
    );

    client
        .batch_execute(&format!("INSERT INTO {schema}.medidas VALUES (7)"))
        .await
        .expect("un valor válido tenía que entrar");

    domain::apply(
        handle,
        &database,
        &[DomainChange::DropDomainConstraint {
            schema: schema.to_owned(),
            name: "positivo".into(),
            constraint: "mayor_que_cero".into(),
            if_exists: false,
            cascade: false,
        }],
    )
    .await
    .expect("tenía que borrar la restricción");

    let info = domain::info(handle, &database, oid).await.unwrap();
    assert!(info.constraints.is_empty());

    client
        .batch_execute(&format!("DROP TABLE {schema}.medidas"))
        .await
        .unwrap();
}

async fn particiones(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();
    let client = handle.client(&database).await.unwrap();

    client
        .batch_execute(&format!(
            "CREATE TABLE {schema}.ventas (id integer, dia date NOT NULL)
                 PARTITION BY RANGE (dia)"
        ))
        .await
        .expect("tenía que crear la tabla madre");

    partition::apply(
        handle,
        &database,
        &[PartitionChange::CreatePartition {
            parent_schema: schema.to_owned(),
            parent: "ventas".into(),
            schema: schema.to_owned(),
            name: "ventas_2024".into(),
            bound: PartitionBound::Range {
                from: vec!["'2024-01-01'".into()],
                to: vec!["'2025-01-01'".into()],
            },
            partition_by: None,
        }],
    )
    .await
    .expect("tenía que crear la partición");

    let parent = relation_oid(handle, schema, "ventas").await.unwrap();
    let info = partition::info(handle, &database, parent)
        .await
        .expect("tenía que leer la partición");
    assert!(
        info.strategy.starts_with("RANGE"),
        "estrategia inesperada: {}",
        info.strategy
    );
    assert_eq!(info.partitions.len(), 1);
    assert_eq!(info.partitions[0].name, "ventas_2024");

    // Enganchar una tabla que ya tiene filas: el servidor las revisa contra el límite, y eso es
    // exactamente lo que no se puede verificar generando el SQL.
    client
        .batch_execute(&format!(
            "CREATE TABLE {schema}.ventas_2023 (id integer, dia date NOT NULL);
             INSERT INTO {schema}.ventas_2023 VALUES (1, '2023-06-01');"
        ))
        .await
        .unwrap();

    let fuera_de_rango = partition::apply(
        handle,
        &database,
        &[PartitionChange::AttachPartition {
            parent_schema: schema.to_owned(),
            parent: "ventas".into(),
            schema: schema.to_owned(),
            name: "ventas_2023".into(),
            bound: PartitionBound::Range {
                from: vec!["'2025-01-01'".into()],
                to: vec!["'2026-01-01'".into()],
            },
        }],
    )
    .await;
    assert!(
        fuera_de_rango.is_err(),
        "enganchar con filas fuera del límite tenía que fallar"
    );

    partition::apply(
        handle,
        &database,
        &[PartitionChange::AttachPartition {
            parent_schema: schema.to_owned(),
            parent: "ventas".into(),
            schema: schema.to_owned(),
            name: "ventas_2023".into(),
            bound: PartitionBound::Range {
                from: vec!["'2023-01-01'".into()],
                to: vec!["'2024-01-01'".into()],
            },
        }],
    )
    .await
    .expect("con el límite correcto tenía que engancharse");

    let info = partition::info(handle, &database, parent).await.unwrap();
    assert_eq!(info.partitions.len(), 2);

    // `CONCURRENTLY` solo desde la 14: el mismo cambio tiene que fallar antes y andar después.
    let sin_bloquear = PartitionChange::DetachPartition {
        parent_schema: schema.to_owned(),
        parent: "ventas".into(),
        schema: schema.to_owned(),
        name: "ventas_2023".into(),
        concurrently: true,
        finalize: false,
    };

    let resultado = partition::apply(handle, &database, std::slice::from_ref(&sin_bloquear)).await;
    if handle.caps.has_detach_partition_concurrently() {
        resultado.expect("desde la 14 tenía que separarse sin bloquear");
    } else {
        assert!(
            resultado.is_err(),
            "antes de la 14 tenía que rechazarse con un mensaje propio"
        );
        partition::apply(
            handle,
            &database,
            &[PartitionChange::DetachPartition {
                parent_schema: schema.to_owned(),
                parent: "ventas".into(),
                schema: schema.to_owned(),
                name: "ventas_2023".into(),
                concurrently: false,
                finalize: false,
            }],
        )
        .await
        .expect("sin CONCURRENTLY tenía que separarse igual");
    }

    let info = partition::info(handle, &database, parent).await.unwrap();
    assert_eq!(info.partitions.len(), 1, "quedó una sola partición");
}

async fn comentarios(handle: &ServerHandle, schema: &str) {
    let database = handle.default_database().to_owned();

    comment::apply(
        handle,
        &database,
        &[
            CommentChange {
                target: CommentTarget::Table {
                    schema: schema.to_owned(),
                    name: "ventas".into(),
                },
                comment: Some("ventas particionadas por día".into()),
            },
            CommentChange {
                target: CommentTarget::Column {
                    schema: schema.to_owned(),
                    table: "ventas".into(),
                    column: "dia".into(),
                },
                comment: Some("clave de partición".into()),
            },
        ],
    )
    .await
    .expect("tenía que comentar la tabla y la columna");

    let client = handle.client(&database).await.unwrap();
    let oid = relation_oid(handle, schema, "ventas").await.unwrap();

    let tabla: Option<String> = client
        .query_one("SELECT obj_description($1, 'pg_class')", &[&oid])
        .await
        .unwrap()
        .get(0);
    assert_eq!(tabla.as_deref(), Some("ventas particionadas por día"));

    let columna: Option<String> = client
        .query_one(
            "SELECT col_description($1, a.attnum)
               FROM pg_attribute a
              WHERE a.attrelid = $1 AND a.attname = 'dia'",
            &[&oid],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(columna.as_deref(), Some("clave de partición"));

    // Sin texto borra: un comentario vacío no es lo mismo que ninguno.
    comment::apply(
        handle,
        &database,
        &[CommentChange {
            target: CommentTarget::Table {
                schema: schema.to_owned(),
                name: "ventas".into(),
            },
            comment: None,
        }],
    )
    .await
    .expect("tenía que borrar el comentario");

    let tabla: Option<String> = client
        .query_one("SELECT obj_description($1, 'pg_class')", &[&oid])
        .await
        .unwrap()
        .get(0);
    assert_eq!(tabla, None);
}

async fn esquemas(handle: &ServerHandle) {
    let database = handle.default_database().to_owned();
    let nombre = format!("pgforge_esquema_{}", std::process::id());
    let renombrado = format!("{nombre}_bis");

    schema::apply(
        handle,
        &database,
        &[SchemaChange::CreateSchema {
            name: nombre.clone(),
            authorization: None,
            if_not_exists: true,
        }],
    )
    .await
    .expect("tenía que crear el esquema");

    schema::apply(
        handle,
        &database,
        &[SchemaChange::RenameSchema {
            name: nombre.clone(),
            new_name: renombrado.clone(),
        }],
    )
    .await
    .expect("tenía que renombrar el esquema");

    let client = handle.client(&database).await.unwrap();
    let existe: bool = client
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1)",
            &[&renombrado],
        )
        .await
        .unwrap()
        .get(0);
    assert!(existe, "el esquema renombrado tenía que existir");

    schema::apply(
        handle,
        &database,
        &[SchemaChange::DropSchema {
            name: renombrado.clone(),
            if_exists: true,
            cascade: true,
        }],
    )
    .await
    .expect("tenía que borrar el esquema");

    let existe: bool = client
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1)",
            &[&renombrado],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!existe, "el esquema tenía que quedar borrado");
}
