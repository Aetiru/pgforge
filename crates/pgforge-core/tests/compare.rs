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

// ------------------------------------------------------------------------------------------------
// Funciones, disparadores, políticas y permisos.
//
// Es la misma prueba que arriba —comparar, aplicar el script, volver a comparar— sobre lo que el
// catálogo escribe distinto según la versión: `pg_get_functiondef`, `pg_get_triggerdef`, las ACL
// con `acldefault` y `pg_policy`. Los roles son del servidor y no del esquema, así que se crean
// con el pid en el nombre y se borran al final.

/// Lo que se quiere. `{a}` y `{b}` son roles.
const LOGIC_SOURCE: &str = r#"
CREATE TABLE {s}.pedidos (id bigint PRIMARY KEY, tenant int NOT NULL DEFAULT 1, total numeric);
CREATE SEQUENCE {s}.folio;

-- La misma función con dos firmas: la sobrecarga no se puede mezclar.
CREATE FUNCTION {s}.doble(x integer) RETURNS integer LANGUAGE sql IMMUTABLE AS $$ SELECT x * 2 $$;
CREATE FUNCTION {s}.doble(x text) RETURNS text LANGUAGE sql AS $$ SELECT x || x $$;

-- Igual que en el destino salvo el formato: no puede ser una diferencia.
CREATE FUNCTION {s}.igual() RETURNS integer LANGUAGE sql AS $$ SELECT 1 $$;

CREATE FUNCTION {s}.auditar() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    -- completa el total
    NEW.total := COALESCE(NEW.total, 0);
    RETURN NEW;
END;
$$;
CREATE PROCEDURE {s}.limpiar() LANGUAGE sql AS $$ SELECT 1 $$;

CREATE TRIGGER pedidos_auditar BEFORE INSERT ON {s}.pedidos
    FOR EACH ROW EXECUTE FUNCTION {s}.auditar();

ALTER TABLE {s}.pedidos ENABLE ROW LEVEL SECURITY;
CREATE POLICY por_tenant ON {s}.pedidos FOR SELECT USING (tenant = 1);
CREATE POLICY solo_a ON {s}.pedidos AS RESTRICTIVE FOR ALL TO {a} USING (total > 0);

GRANT SELECT, INSERT ON {s}.pedidos TO {a};
GRANT SELECT ON {s}.pedidos TO {b} WITH GRANT OPTION;
GRANT USAGE ON SEQUENCE {s}.folio TO {a};
REVOKE EXECUTE ON PROCEDURE {s}.limpiar() FROM PUBLIC;
GRANT EXECUTE ON PROCEDURE {s}.limpiar() TO {a};
"#;

/// Lo que hay: difiere en todo.
const LOGIC_TARGET: &str = r#"
CREATE TABLE {s}.pedidos (id bigint PRIMARY KEY, tenant int NOT NULL DEFAULT 1, total numeric);
CREATE TABLE {s}.viejas (id int);
CREATE SEQUENCE {s}.folio;

CREATE FUNCTION {s}.doble(x integer) RETURNS integer LANGUAGE sql IMMUTABLE AS $$ SELECT x * 3 $$;
CREATE FUNCTION {s}.sobrante(a integer) RETURNS integer LANGUAGE sql AS $$ SELECT a $$;
CREATE FUNCTION {s}.igual() RETURNS integer LANGUAGE sql AS $$
    SELECT   1
$$;

-- Un disparador que sobra sobre una tabla que se queda: hay que borrarlo antes que su función.
CREATE FUNCTION {s}.trg_sobra() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$;
CREATE TRIGGER sobra BEFORE UPDATE ON {s}.pedidos
    FOR EACH ROW EXECUTE FUNCTION {s}.trg_sobra();
-- Y uno que se va con su tabla: no tiene que aparecer aparte.
CREATE TRIGGER de_viejas BEFORE UPDATE ON {s}.viejas
    FOR EACH ROW EXECUTE FUNCTION {s}.trg_sobra();

CREATE POLICY por_tenant ON {s}.pedidos FOR SELECT USING (tenant = 2);
CREATE POLICY vieja ON {s}.pedidos FOR SELECT USING (true);

GRANT SELECT ON {s}.pedidos TO {a};
GRANT DELETE ON {s}.pedidos TO {b};
"#;

fn logic_roles() -> (String, String) {
    let pid = std::process::id();
    (
        format!("pgforge_cmp_a_{pid}"),
        format!("pgforge_cmp_b_{pid}"),
    )
}

async fn setup_logic(handle: &ServerHandle, schema: &str, fixture: &str) {
    let (a, b) = logic_roles();
    let client = handle.client(handle.default_database()).await.unwrap();
    for role in [&a, &b] {
        client
            .batch_execute(&format!(
                "DO $$ BEGIN IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '{role}') \
                 THEN CREATE ROLE {role}; END IF; END $$;"
            ))
            .await
            .expect("no se pudo crear el rol de prueba");
    }
    setup(
        handle,
        schema,
        &fixture.replace("{a}", &a).replace("{b}", &b),
    )
    .await;
}

async fn teardown_logic(handle: &ServerHandle, schema: &str) {
    teardown(handle, schema).await;
    let (a, b) = logic_roles();
    if let Ok(client) = handle.client(handle.default_database()).await {
        for role in [&a, &b] {
            let _ = client
                .batch_execute(&format!("DROP ROLE IF EXISTS {role}"))
                .await;
        }
    }
}

#[tokio::test]
async fn compara_logica_y_permisos_y_los_deja_iguales_contra_servidores_reales() {
    let urls = test_urls();
    if urls.is_empty() {
        eprintln!(
            "AVISO: PGFORGE_TEST_URLS no está definida, no se verificó nada contra un servidor real."
        );
        return;
    }

    for (position, url) in urls.iter().enumerate() {
        let target_url = &urls[(position + 1) % urls.len()];

        let source = connect(url).await;
        let target = connect(target_url).await;
        // Nombres propios: este test corre a la vez que el de arriba, sobre los mismos servidores.
        let source_name = format!("pgforge_cmp_lsrc_{}", std::process::id());
        let target_name = format!("pgforge_cmp_ldst_{}", std::process::id());

        setup_logic(&source, &source_name, LOGIC_SOURCE).await;
        setup_logic(&target, &target_name, LOGIC_TARGET).await;

        let outcome = {
            let (source, target) = (Arc::clone(&source), Arc::clone(&target));
            let (source_name, target_name) = (source_name.clone(), target_name.clone());
            tokio::spawn(async move {
                logic_assertions(&source, &source_name, &target, &target_name).await
            })
            .await
        };

        teardown_logic(&source, &source_name).await;
        teardown_logic(&target, &target_name).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!(
            "ok (lógica y permisos): PostgreSQL {} ({url}) contra PostgreSQL {} ({target_url})",
            source.caps.version, target.caps.version
        );
    }
}

async fn logic_assertions(
    source: &ServerHandle,
    source_schema: &str,
    target: &ServerHandle,
    target_schema: &str,
) {
    let source_database = source.default_database().to_owned();
    let target_database = target.default_database().to_owned();
    let (role_a, role_b) = logic_roles();

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

    let find = |kind: ObjectKind, name: &str| {
        comparison
            .diff
            .entries
            .iter()
            .find(|entry| entry.kind == kind && entry.name == name)
            .unwrap_or_else(|| {
                panic!(
                    "falta {kind:?} «{name}» en el informe; hay {:?}",
                    comparison
                        .diff
                        .entries
                        .iter()
                        .map(|e| (e.kind, e.name.as_str()))
                        .collect::<Vec<_>>()
                )
            })
    };
    let absent = |kind: ObjectKind, name: &str| {
        assert!(
            !comparison
                .diff
                .entries
                .iter()
                .any(|entry| entry.kind == kind && entry.name == name),
            "{kind:?} «{name}» no tendría que estar en el informe"
        );
    };

    // Funciones: por firma, con la sobrecarga separada y el formato sin contar.
    assert_eq!(
        find(ObjectKind::Function, "doble(integer)").status,
        Status::Different
    );
    assert_eq!(
        find(ObjectKind::Function, "doble(text)").status,
        Status::OnlySource
    );
    assert_eq!(
        find(ObjectKind::Function, "sobrante(integer)").status,
        Status::OnlyTarget
    );
    assert_eq!(
        find(ObjectKind::Procedure, "limpiar()").status,
        Status::OnlySource
    );
    absent(ObjectKind::Function, "igual()");

    // Disparadores: el de una tabla que se queda sí; el de una tabla que se va, no.
    assert_eq!(
        find(ObjectKind::Trigger, "pedidos.pedidos_auditar").status,
        Status::OnlySource
    );
    assert_eq!(
        find(ObjectKind::Trigger, "pedidos.sobra").status,
        Status::OnlyTarget
    );
    absent(ObjectKind::Trigger, "viejas.de_viejas");

    // Políticas y seguridad por filas.
    let tenant = find(ObjectKind::Policy, "pedidos.por_tenant");
    assert_eq!(tenant.status, Status::Different);
    assert_eq!(tenant.details[0].name, "USING");
    assert_eq!(
        find(ObjectKind::Policy, "pedidos.solo_a").status,
        Status::OnlySource
    );
    assert_eq!(
        find(ObjectKind::Policy, "pedidos.vieja").status,
        Status::OnlyTarget
    );
    assert_eq!(
        find(ObjectKind::RowSecurity, "pedidos").details[0].name,
        "habilitada"
    );

    // Permisos: por rol, con las ACL de fábrica expandidas.
    let pedidos = find(ObjectKind::Grant, "pedidos");
    let grantee = |name: &str| {
        pedidos
            .details
            .iter()
            .find(|detail| detail.name == name)
            .unwrap_or_else(|| panic!("falta el permiso de «{name}»"))
    };
    assert_eq!(grantee(&role_a).status, Status::Different);
    assert_eq!(grantee(&role_b).status, Status::Different);
    assert!(grantee(&role_b)
        .source
        .as_deref()
        .unwrap()
        .contains("GRANT OPTION"));
    assert_eq!(
        find(ObjectKind::Grant, "folio").details[0].status,
        Status::OnlySource
    );

    // El script: lo destructivo al final, y un disparador se borra antes que su función.
    let sql: Vec<&str> = comparison
        .plan
        .statements
        .iter()
        .map(|statement| statement.sql.as_str())
        .collect();
    let at = |needle: &str| {
        sql.iter()
            .position(|statement| statement.contains(needle))
            .unwrap_or_else(|| panic!("el script no tiene «{needle}»: {sql:#?}"))
    };
    assert!(at("DROP TRIGGER sobra") < at("DROP FUNCTION"));
    assert!(at("CREATE OR REPLACE FUNCTION") < at("CREATE TRIGGER pedidos_auditar"));
    assert!(at("CREATE POLICY solo_a") < at("ENABLE ROW LEVEL SECURITY"));
    assert!(at("ENABLE ROW LEVEL SECURITY") < at("GRANT"));

    // Una instantánea guardada tiene que poder leerse de vuelta (la validación no puede rechazar
    // lo que escribe el propio servidor) y dar el mismo informe.
    let path = std::env::temp_dir().join(format!("pgforge_cmp_{}.json", std::process::id()));
    compare::file::save(source, &source_database, source_schema, &path)
        .await
        .expect("no se pudo guardar la instantánea");
    let file = compare::file::load(&path).expect("la instantánea guardada no se pudo leer");
    let _ = std::fs::remove_file(&path);
    let from_file = compare::compare_with_file(&file, target, &target_database, target_schema)
        .await
        .expect("no se pudo comparar contra la instantánea");
    assert_eq!(
        from_file.diff.entries.len(),
        comparison.diff.entries.len(),
        "la instantánea y el servidor vivo tienen que dar el mismo informe"
    );

    apply(target, &target_database, &comparison.plan.statements).await;

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
            .map(|entry| (entry.kind, entry.name.as_str(), entry.status))
            .collect::<Vec<_>>()
    );
    assert!(
        after.plan.statements.is_empty(),
        "no quedó nada por hacer, pero el script no está vacío: {:?}",
        after
            .plan
            .statements
            .iter()
            .map(|s| &s.sql)
            .collect::<Vec<_>>()
    );
}
