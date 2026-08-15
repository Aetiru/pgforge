//! Comparación de esquemas contra servidores reales.
//!
//! Lo que no se puede verificar sin servidor son las dos puntas: que lo que se lee del catálogo sea
//! de verdad la forma del esquema, y que el SQL propuesto lo acepte PostgreSQL. Que la comparación
//! empareje bien y que el texto salga como se espera ya lo cubren los tests unitarios de
//! `compare::diff` y `compare::sync`, que corren sin red.
//!
//! La prueba fuerte es la última: se aplica el script contra el destino y se vuelve a comparar. Si
//! algo quedó afuera —una restricción que no se agregó, un tipo que faltaba— la segunda comparación
//! lo dice. Un script que corre sin error pero deja los esquemas distintos no sirve para nada.
//!
//! Con dos URLs en `PGFORGE_TEST_URLS` los dos lados son servidores distintos, que es el caso de
//! uso real (y, si son de versiones distintas, el que más se puede romper). Con una sola, los dos
//! lados son el mismo servidor y esquemas distintos: sigue valiendo, porque los nombres de esquema
//! distintos son justamente lo que ejercita la reescritura.

use std::sync::Arc;

use pgforge_core::compare::{self, ObjectKind, Status};
use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};

fn test_urls() -> Vec<String> {
    std::env::var("PGFORGE_TEST_URLS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
        .collect()
}

/// El esquema que se quiere: el que hace de origen.
const SOURCE_FIXTURE: &str = r#"
CREATE TYPE {s}.estado AS ENUM ('activo', 'pausa', 'baja');

CREATE SEQUENCE {s}.folio START WITH 5 INCREMENT BY 2;

CREATE TABLE {s}.clientes (
    id     bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    nombre text NOT NULL,
    email  text,
    estado {s}.estado NOT NULL DEFAULT 'activo',
    creado timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX clientes_nombre_idx ON {s}.clientes (nombre);

CREATE TABLE {s}.pedidos (
    id         bigint PRIMARY KEY,
    cliente_id bigint NOT NULL REFERENCES {s}.clientes(id) ON DELETE CASCADE,
    total      numeric(12,2) NOT NULL CONSTRAINT pedidos_total_check CHECK (total >= 0)
);

CREATE VIEW {s}.clientes_activos AS
    SELECT id, nombre FROM {s}.clientes WHERE estado = 'activo';
"#;

/// El esquema que hay: le falta lo de arriba, le sobra lo suyo y difiere en lo que comparten.
const TARGET_FIXTURE: &str = r#"
CREATE TYPE {s}.estado AS ENUM ('activo', 'baja');

CREATE SEQUENCE {s}.folio START WITH 5 INCREMENT BY 1;

CREATE TABLE {s}.clientes (
    id     bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    nombre varchar(50) NOT NULL,
    estado {s}.estado NOT NULL DEFAULT 'activo',
    creado timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE {s}.sobrante (
    id int PRIMARY KEY
);

CREATE VIEW {s}.clientes_activos AS
    SELECT id FROM {s}.clientes;
"#;

async fn connect(url: &str) -> Arc<ServerHandle> {
    let (profile, password) = ConnectionProfile::from_url("test", url)
        .unwrap_or_else(|e| panic!("URL de prueba inválida ({url}): {e}"));
    let manager = ConnectionManager::new();
    manager
        .connect(profile, password)
        .await
        .unwrap_or_else(|e| panic!("no se pudo conectar a {url}: {e}"))
}

fn source_schema() -> String {
    format!("pgforge_cmp_src_{}", std::process::id())
}

fn target_schema() -> String {
    format!("pgforge_cmp_dst_{}", std::process::id())
}

async fn setup(handle: &ServerHandle, schema: &str, fixture: &str) {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema};"
        ))
        .await
        .expect("no se pudo crear el esquema de prueba");
    client
        .batch_execute(&fixture.replace("{s}", schema))
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
async fn compara_dos_esquemas_y_los_deja_iguales_contra_servidores_reales() {
    let urls = test_urls();
    if urls.is_empty() {
        eprintln!(
            "AVISO: PGFORGE_TEST_URLS no está definida, no se verificó nada contra un servidor real."
        );
        return;
    }

    // Cada URL hace de origen y la siguiente de destino. Con una sola queda comparándose contra sí
    // misma, en dos esquemas distintos; con dos o más, cada par cruza versiones.
    for (position, url) in urls.iter().enumerate() {
        let target_url = &urls[(position + 1) % urls.len()];

        let source = connect(url).await;
        let target = connect(target_url).await;
        let (source_name, target_name) = (source_schema(), target_schema());

        setup(&source, &source_name, SOURCE_FIXTURE).await;
        setup(&target, &target_name, TARGET_FIXTURE).await;

        // Las comprobaciones corren aparte para que un fallo no deje los esquemas de prueba
        // colgados en ninguno de los dos servidores.
        let outcome = {
            let (source, target) = (Arc::clone(&source), Arc::clone(&target));
            let (source_name, target_name) = (source_name.clone(), target_name.clone());
            tokio::spawn(
                async move { assertions(&source, &source_name, &target, &target_name).await },
            )
            .await
        };

        teardown(&source, &source_name).await;
        teardown(&target, &target_name).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!(
            "ok: PostgreSQL {} ({url}) contra PostgreSQL {} ({target_url})",
            source.caps.version, target.caps.version
        );
    }
}

async fn assertions(
    source: &ServerHandle,
    source_schema: &str,
    target: &ServerHandle,
    target_schema: &str,
) {
    let source_database = source.default_database().to_owned();
    let target_database = target.default_database().to_owned();

    let comparison = compare::compare(
        source,
        &source_database,
        source_schema,
        target,
        &target_database,
        target_schema,
    )
    .await
    .expect("no se pudo comparar");

    let entry = |name: &str| {
        comparison
            .diff
            .entries
            .iter()
            .find(|entry| entry.name == name)
            .unwrap_or_else(|| {
                panic!(
                    "falta «{name}» en el informe; hay {:?}",
                    comparison
                        .diff
                        .entries
                        .iter()
                        .map(|e| e.name.as_str())
                        .collect::<Vec<_>>()
                )
            })
    };

    // Lo que falta entero de un lado.
    assert_eq!(entry("pedidos").status, Status::OnlySource);
    assert_eq!(entry("pedidos").kind, ObjectKind::Table);
    assert_eq!(entry("sobrante").status, Status::OnlyTarget);

    // Lo que está de los dos lados y difiere.
    let clientes = entry("clientes");
    assert_eq!(clientes.status, Status::Different);
    let detail = |name: &str| {
        clientes
            .details
            .iter()
            .find(|detail| detail.name == name)
            .unwrap_or_else(|| panic!("falta el detalle «{name}» de clientes"))
    };
    assert_eq!(detail("email").status, Status::OnlySource);
    assert_eq!(detail("nombre").status, Status::Different);
    assert_eq!(detail("clientes_nombre_idx").status, Status::OnlySource);
    // La clave primaria y la columna de identidad son iguales en los dos: no tienen por qué
    // aparecer.
    assert!(
        clientes.details.iter().all(|detail| detail.name != "id"),
        "una columna idéntica no es una diferencia"
    );

    let estado = entry("estado");
    assert_eq!(estado.kind, ObjectKind::Enum);
    assert_eq!(estado.details[0].name, "pausa");
    assert_eq!(entry("folio").kind, ObjectKind::Sequence);
    assert_eq!(entry("clientes_activos").kind, ObjectKind::View);

    assert!(
        !comparison.plan.statements.is_empty(),
        "hay diferencias pero el script salió vacío"
    );

    apply(target, &target_database, &comparison.plan.statements).await;

    // Y ahora la prueba de verdad: después de correr el script, los dos esquemas son el mismo.
    let after = compare::compare(
        source,
        &source_database,
        source_schema,
        target,
        &target_database,
        target_schema,
    )
    .await
    .expect("no se pudo volver a comparar");

    assert!(
        after.diff.is_empty(),
        "el script corrió pero quedaron diferencias: {:?}",
        after
            .diff
            .entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry.status))
            .collect::<Vec<_>>()
    );
    assert!(after.diff.equal > 0, "no se comparó ningún objeto");
}

/// Corre el script sentencia por sentencia, no todo junto.
///
/// `batch_execute` con varias sentencias las mete en una transacción implícita, y ahí un
/// `ALTER TYPE … ADD VALUE` no puede usarse después en la misma tanda. Una por una es además cómo
/// lo va a correr quien copie el script en el editor.
async fn apply(handle: &ServerHandle, database: &str, statements: &[compare::SyncStatement]) {
    let client = handle.client(database).await.unwrap();
    for statement in statements {
        client
            .batch_execute(&statement.sql)
            .await
            .unwrap_or_else(|e| {
                // El `Display` de un error de tokio-postgres es apenas «db error»: lo que dice qué pasó
                // está en el error de la base.
                let detail = e
                    .as_db_error()
                    .map(|db| db.message().to_owned())
                    .unwrap_or_else(|| e.to_string());
                panic!("el servidor rechazó «{}»: {detail}", statement.sql)
            });
    }
}
