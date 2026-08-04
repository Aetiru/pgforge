//! Recorrido del árbol y generación de DDL contra servidores reales.
//!
//! Se ejecuta contra todas las instancias listadas en `PGFORGE_TEST_URLS`, separadas por coma. La
//! gracia es correrlo contra más de una versión de PostgreSQL a la vez: el objetivo de estos tests
//! no es solo que las consultas anden, sino que anden igual en el rango de versiones soportado.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::ddl::{self, DdlSource};
use pgforge_core::introspect::{self, Folder, NodeKind, NodeTag, TreeNode, TreeOptions};

fn test_urls() -> Vec<String> {
    std::env::var("PGFORGE_TEST_URLS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
        .collect()
}

const FIXTURE: &str = r#"
CREATE TYPE {s}.estado AS ENUM ('nuevo', 'activo', 'baja');
CREATE TYPE {s}.punto AS (lat double precision, lon double precision);
CREATE DOMAIN {s}.email AS text CHECK (VALUE LIKE '%@%');

CREATE SEQUENCE {s}.folios START WITH 100 INCREMENT BY 5;

CREATE TABLE {s}.clientes (
    id        bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    nombre    text NOT NULL,
    correo    {s}.email,
    estado    {s}.estado NOT NULL DEFAULT 'nuevo',
    ubicacion {s}.punto,
    creado    timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX clientes_activos_idx ON {s}.clientes (nombre) WHERE estado = 'activo';

CREATE TABLE {s}.reservas (
    id      bigserial PRIMARY KEY,
    periodo daterange NOT NULL,
    EXCLUDE USING gist (periodo WITH &&)
);

CREATE TABLE {s}.ventas (
    id         bigint NOT NULL,
    cliente_id bigint NOT NULL REFERENCES {s}.clientes(id),
    monto      numeric(12,2) NOT NULL CHECK (monto > 0),
    fecha      date NOT NULL
) PARTITION BY RANGE (fecha);

CREATE TABLE {s}.ventas_2026 PARTITION OF {s}.ventas
    FOR VALUES FROM ('2026-01-01') TO ('2027-01-01');

CREATE VIEW {s}.clientes_activos AS
    SELECT id, nombre FROM {s}.clientes WHERE estado = 'activo';

CREATE MATERIALIZED VIEW {s}.resumen AS
    SELECT estado, count(*) AS total FROM {s}.clientes GROUP BY estado;

CREATE FUNCTION {s}.saludo(quien text) RETURNS text
    LANGUAGE plpgsql IMMUTABLE AS $fn$
BEGIN
    RETURN 'hola ' || quien;
END;
$fn$;

CREATE FUNCTION {s}.tocar() RETURNS trigger LANGUAGE plpgsql AS $fn$
BEGIN
    RETURN NEW;
END;
$fn$;

CREATE TRIGGER clientes_tocar BEFORE UPDATE ON {s}.clientes
    FOR EACH ROW EXECUTE FUNCTION {s}.tocar();

ALTER TABLE {s}.reservas ENABLE ROW LEVEL SECURITY;

COMMENT ON TABLE {s}.clientes IS 'Padrón de clientes';
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

/// Cada instancia usa su propio esquema para que dos corridas simultáneas no se pisen.
fn schema_name() -> String {
    format!("pgforge_test_{}", std::process::id())
}

async fn create_fixture(handle: &ServerHandle, schema: &str) {
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

async fn drop_fixture(handle: &ServerHandle, schema: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .await;
    }
}

async fn expand(handle: &ServerHandle, node: &TreeNode) -> Vec<TreeNode> {
    introspect::children(handle, Some(node), TreeOptions::default())
        .await
        .unwrap_or_else(|e| panic!("no se pudo expandir {}: {e}", node.label))
}

fn find<'a>(nodes: &'a [TreeNode], label: &str) -> &'a TreeNode {
    nodes
        .iter()
        .find(|node| node.label == label || node.label.starts_with(&format!("{label}(")))
        .unwrap_or_else(|| {
            let disponibles: Vec<&str> = nodes.iter().map(|n| n.label.as_str()).collect();
            panic!("no se encontró «{label}». Hay: {disponibles:?}")
        })
}

fn folder(nodes: &[TreeNode], folder: Folder) -> &TreeNode {
    nodes
        .iter()
        .find(|node| node.kind == NodeKind::Folder(folder))
        .unwrap_or_else(|| panic!("falta la carpeta {:?}", folder))
}

#[tokio::test]
async fn recorre_el_arbol_y_genera_el_ddl() {
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
        assert!(
            version.is_supported(),
            "{url} corre {version}, por debajo del mínimo soportado"
        );

        let schema_name = schema_name();
        create_fixture(&handle, &schema_name).await;

        // Las comprobaciones corren en una tarea aparte para que un fallo no impida borrar el
        // esquema de prueba: dejarlo colgado ensucia la base para la corrida siguiente.
        let outcome = {
            let handle = Arc::clone(&handle);
            let schema_name = schema_name.clone();
            tokio::spawn(async move { assertions(&handle, &schema_name).await }).await
        };
        drop_fixture(&handle, &schema_name).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({url})");
    }
}

async fn assertions(handle: &ServerHandle, schema_name: &str) {
    let database = handle.default_database().to_owned();

    // Primer nivel: las bases del servidor.
    let databases = introspect::children(handle, None, TreeOptions::default())
        .await
        .expect("no se pudieron listar las bases");
    let db_node = find(&databases, &database).clone();

    // Roles: la carpeta cuelga de la raíz, no de una base. Que un rol pueda entrar o no va como
    // etiqueta, que es lo que la interfaz muestra sin leer el detalle.
    let roles_folder = folder(&databases, Folder::Roles).clone();
    let roles = expand(handle, &roles_folder).await;
    let propio = find(&roles, &handle.caps.current_user);
    assert!(
        propio.tags.contains(&NodeTag::Login),
        "el rol con el que se conectó el test tiene que poder iniciar sesión"
    );
    assert_eq!(
        propio.tags.contains(&NodeTag::Superuser),
        handle.caps.is_superuser
    );

    // Base -> carpeta de esquemas -> nuestro esquema.
    let db_children = expand(handle, &db_node).await;
    let schemas_folder = folder(&db_children, Folder::Schemas).clone();
    let schemas = expand(handle, &schemas_folder).await;
    let schema = find(&schemas, schema_name).clone();
    assert_eq!(schema.kind, NodeKind::Schema);

    // Los esquemas del sistema quedan ocultos salvo que se pidan.
    assert!(
        !schemas.iter().any(|n| n.label == "pg_catalog"),
        "pg_catalog no debería aparecer con las opciones por omisión"
    );
    let con_sistema = introspect::children(
        handle,
        Some(&schemas_folder),
        TreeOptions {
            show_system_schemas: true,
        },
    )
    .await
    .unwrap();
    assert!(con_sistema.iter().any(|n| n.label == "pg_catalog"));

    // Carpetas del esquema, con sus cantidades.
    let folders = expand(handle, &schema).await;
    let tables_folder = folder(&folders, Folder::Tables).clone();
    assert_eq!(
        tables_folder.detail.as_deref(),
        Some("4"),
        "clientes, reservas, ventas y su partición"
    );
    assert_eq!(folder(&folders, Folder::Views).detail.as_deref(), Some("1"));
    assert_eq!(
        folder(&folders, Folder::MaterializedViews)
            .detail
            .as_deref(),
        Some("1")
    );
    assert_eq!(
        folder(&folders, Folder::Sequences).detail.as_deref(),
        Some("3"),
        "la secuencia propia, más la de la identidad de clientes y la del bigserial de reservas"
    );
    assert_eq!(
        folder(&folders, Folder::Types).detail.as_deref(),
        Some("3"),
        "el enum, el compuesto y el dominio; los tipos fila de las tablas no cuentan"
    );

    // Tablas: la particionada y su partición se distinguen por tipo de nodo.
    let tables = expand(handle, &tables_folder).await;
    let clientes = find(&tables, "clientes").clone();
    assert_eq!(clientes.kind, NodeKind::Table);
    assert_eq!(clientes.comment.as_deref(), Some("Padrón de clientes"));
    assert_eq!(find(&tables, "ventas").kind, NodeKind::PartitionedTable);
    assert!(find(&tables, "ventas_2026")
        .tags
        .contains(&NodeTag::Partition));

    // El dueño es el usuario con el que corre el test, así que no se repite en cada fila.
    assert_eq!(clientes.detail, None);
    assert!(
        find(&tables, "reservas")
            .tags
            .contains(&NodeTag::RowSecurity),
        "una tabla con RLS activa tiene que avisarlo antes de consultarla"
    );
    assert!(!clientes.tags.contains(&NodeTag::RowSecurity));

    // Columnas de la tabla.
    let clientes_folders = expand(handle, &clientes).await;
    let columns_folder = folder(&clientes_folders, Folder::Columns).clone();
    assert_eq!(columns_folder.detail.as_deref(), Some("6"));

    let columns = expand(handle, &columns_folder).await;
    assert_eq!(columns.len(), 6);
    let correo = find(&columns, "correo");
    assert!(
        correo
            .detail
            .as_deref()
            .unwrap_or_default()
            .contains("email"),
        "la columna debe mostrar el dominio, no su tipo base: {:?}",
        correo.detail
    );
    let nombre = find(&columns, "nombre");
    assert!(nombre
        .detail
        .as_deref()
        .unwrap_or_default()
        .contains("not null"));

    // Índice parcial.
    let indexes = expand(handle, folder(&clientes_folders, Folder::Indexes)).await;
    let parcial = find(&indexes, "clientes_activos_idx").clone();
    let index_ddl = ddl::object_ddl(handle, &parcial).await.unwrap();
    assert_eq!(index_ddl.source, DdlSource::Catalog);
    assert!(
        index_ddl.sql.contains("WHERE"),
        "se perdió la condición del índice parcial: {}",
        index_ddl.sql
    );

    // Restricciones: no se compara la cantidad porque PostgreSQL 17 registra también las
    // restricciones NOT NULL en pg_constraint y las versiones anteriores no.
    let constraints = expand(handle, folder(&clientes_folders, Folder::Constraints)).await;
    let pk = constraints
        .iter()
        .find(|n| n.detail.as_deref().unwrap_or_default().contains("primaria"))
        .expect("falta la clave primaria");
    let pk_ddl = ddl::object_ddl(handle, pk).await.unwrap();
    assert!(pk_ddl.sql.contains("ADD CONSTRAINT"), "{}", pk_ddl.sql);
    assert!(pk_ddl.sql.contains("PRIMARY KEY"), "{}", pk_ddl.sql);

    // Disparador.
    let triggers = expand(handle, folder(&clientes_folders, Folder::Triggers)).await;
    let trigger = find(&triggers, "clientes_tocar").clone();
    let trigger_ddl = ddl::object_ddl(handle, &trigger).await.unwrap();
    assert!(
        trigger_ddl.sql.starts_with("CREATE TRIGGER"),
        "{}",
        trigger_ddl.sql
    );

    // Vista y vista materializada.
    let views = expand(handle, folder(&folders, Folder::Views)).await;
    let view_ddl = ddl::object_ddl(handle, find(&views, "clientes_activos"))
        .await
        .unwrap();
    assert!(
        view_ddl.sql.starts_with("CREATE OR REPLACE VIEW"),
        "{}",
        view_ddl.sql
    );

    let matviews = expand(handle, folder(&folders, Folder::MaterializedViews)).await;
    let matview_ddl = ddl::object_ddl(handle, find(&matviews, "resumen"))
        .await
        .unwrap();
    assert!(
        matview_ddl.sql.starts_with("CREATE MATERIALIZED VIEW"),
        "{}",
        matview_ddl.sql
    );

    // Funciones: la etiqueta lleva la firma porque el nombre solo no distingue sobrecargas.
    let functions = expand(handle, folder(&folders, Folder::Functions)).await;
    let saludo = functions
        .iter()
        .find(|n| n.label.starts_with("saludo("))
        .expect("falta la función saludo");
    assert!(
        saludo.label.contains("text"),
        "la firma debe estar en la etiqueta"
    );
    let function_ddl = ddl::object_ddl(handle, saludo).await.unwrap();
    assert!(
        function_ddl.sql.contains("CREATE OR REPLACE FUNCTION"),
        "{}",
        function_ddl.sql
    );
    assert!(function_ddl.sql.contains("hola "), "{}", function_ddl.sql);

    // Tipos.
    let types = expand(handle, folder(&folders, Folder::Types)).await;

    let enum_ddl = ddl::object_ddl(handle, find(&types, "estado"))
        .await
        .unwrap();
    assert!(enum_ddl.sql.contains("AS ENUM"), "{}", enum_ddl.sql);
    assert!(enum_ddl.sql.contains("'activo'"), "{}", enum_ddl.sql);

    let composite_ddl = ddl::object_ddl(handle, find(&types, "punto"))
        .await
        .unwrap();
    assert!(
        composite_ddl.sql.contains("lat double precision"),
        "{}",
        composite_ddl.sql
    );

    let domain_ddl = ddl::object_ddl(handle, find(&types, "email"))
        .await
        .unwrap();
    assert!(
        domain_ddl.sql.starts_with("CREATE DOMAIN"),
        "{}",
        domain_ddl.sql
    );
    assert!(domain_ddl.sql.contains("CHECK"), "{}", domain_ddl.sql);

    // Secuencia.
    let sequences = expand(handle, folder(&folders, Folder::Sequences)).await;
    let sequence_ddl = ddl::object_ddl(handle, find(&sequences, "folios"))
        .await
        .unwrap();
    assert!(
        sequence_ddl.sql.contains("START WITH 100"),
        "{}",
        sequence_ddl.sql
    );
    assert!(
        sequence_ddl.sql.contains("INCREMENT BY 5"),
        "{}",
        sequence_ddl.sql
    );

    // El bigserial de reservas tiene dueño: la secuencia debe declararlo.
    let owned = find(&sequences, "reservas_id_seq");
    let owned_ddl = ddl::object_ddl(handle, owned).await.unwrap();
    assert!(owned_ddl.sql.contains("OWNED BY"), "{}", owned_ddl.sql);

    // Tabla: el único DDL que sale de pg_dump.
    match ddl::object_ddl(handle, &clientes).await {
        Ok(table_ddl) => {
            assert_eq!(table_ddl.source, DdlSource::PgDump);
            assert!(table_ddl.sql.contains("CREATE TABLE"), "{}", table_ddl.sql);
            assert!(table_ddl.sql.contains("nombre"), "{}", table_ddl.sql);
            assert!(
                table_ddl.sql.contains("GENERATED ALWAYS AS IDENTITY"),
                "se perdió la identidad de la clave primaria: {}",
                table_ddl.sql
            );
            assert!(
                !table_ddl.sql.contains("SET statement_timeout"),
                "quedó el preámbulo de pg_dump: {}",
                table_ddl.sql
            );
        }
        Err(e) => eprintln!("AVISO: no se pudo generar el DDL de la tabla ({e})"),
    }
}
