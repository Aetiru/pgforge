//! Grafo de relaciones de un esquema contra servidores reales.
//!
//! Se ejecuta contra todas las instancias listadas en `PGFORGE_TEST_URLS`, separadas por coma. Lo
//! que se verifica acá no es que la consulta ande, sino que el grafo diga la verdad sobre el
//! modelo: claves compuestas en el orden de la restricción, autorreferencias, referencias que
//! salen del esquema y particiones que no duplican la clave foránea de su padre.

use std::sync::Arc;

use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::ddl::RefAction;
use pgforge_core::introspect::{self, GraphEdge, NodeKind, SchemaGraph};

fn test_urls() -> Vec<String> {
    std::env::var("PGFORGE_TEST_URLS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `{s}` es el esquema del diagrama y `{x}` uno vecino, para la referencia que sale de él.
const FIXTURE: &str = r#"
CREATE TABLE {x}.catalogo (
    codigo text PRIMARY KEY,
    detalle text
);

CREATE TABLE {s}.clientes (
    id     bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    nombre text NOT NULL
);

CREATE TABLE {s}.sucursales (
    empresa  int,
    sucursal int,
    nombre   text NOT NULL,
    PRIMARY KEY (empresa, sucursal)
);

CREATE TABLE {s}.empleados (
    id            bigserial PRIMARY KEY,
    empresa_id    int NOT NULL,
    sucursal_num  int NOT NULL,
    jefe_id       bigint REFERENCES {s}.empleados(id),
    CONSTRAINT empleados_sucursal_fk
        FOREIGN KEY (sucursal_num, empresa_id)
        REFERENCES {s}.sucursales (sucursal, empresa)
        ON UPDATE CASCADE ON DELETE SET NULL
);

CREATE TABLE {s}.ventas (
    id         bigint NOT NULL,
    cliente_id bigint NOT NULL REFERENCES {s}.clientes(id) ON DELETE CASCADE,
    fecha      date NOT NULL
) PARTITION BY RANGE (fecha);

CREATE TABLE {s}.ventas_2026 PARTITION OF {s}.ventas
    FOR VALUES FROM ('2026-01-01') TO ('2027-01-01');

CREATE TABLE {s}.pedidos (
    id     bigserial PRIMARY KEY,
    codigo text NOT NULL REFERENCES {x}.catalogo(codigo)
);

CREATE VIEW {s}.clientes_activos AS SELECT id, nombre FROM {s}.clientes;

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
    format!("pgforge_graph_{}", std::process::id())
}

async fn setup(handle: &ServerHandle, schema: &str) {
    let client = handle.client(handle.default_database()).await.unwrap();
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE;
             DROP SCHEMA IF EXISTS {schema}_ext CASCADE;
             CREATE SCHEMA {schema};
             CREATE SCHEMA {schema}_ext;"
        ))
        .await
        .expect("no se pudieron crear los esquemas de prueba");
    client
        .batch_execute(
            &FIXTURE
                .replace("{s}", schema)
                .replace("{x}", &format!("{schema}_ext")),
        )
        .await
        .expect("no se pudo crear el fixture");
}

async fn teardown(handle: &ServerHandle, schema: &str) {
    if let Ok(client) = handle.client(handle.default_database()).await {
        let _ = client
            .batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {schema} CASCADE;
                 DROP SCHEMA IF EXISTS {schema}_ext CASCADE;"
            ))
            .await;
    }
}

#[tokio::test]
async fn arma_el_grafo_de_un_esquema_contra_servidores_reales() {
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
        let schema_name = schema_name();
        setup(&handle, &schema_name).await;

        // Las comprobaciones corren en una tarea aparte para que un fallo no impida borrar los
        // esquemas de prueba: dejarlos colgados ensucia la base para la corrida siguiente.
        let outcome = {
            let handle = Arc::clone(&handle);
            let schema_name = schema_name.clone();
            tokio::spawn(async move { assertions(&handle, &schema_name).await }).await
        };
        teardown(&handle, &schema_name).await;

        if let Err(join) = outcome {
            std::panic::resume_unwind(join.into_panic());
        }
        eprintln!("ok contra PostgreSQL {version} ({url})");
    }
}

async fn assertions(handle: &ServerHandle, schema_name: &str) {
    let database = handle.default_database().to_owned();
    let graph = introspect::schema_graph(handle, &database, schema_name)
        .await
        .expect("no se pudo armar el grafo");

    // Solo tablas: la vista no entra al diagrama porque no participa de ninguna clave foránea.
    let names: Vec<&str> = graph.tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "clientes",
            "empleados",
            "pedidos",
            "sucursales",
            "ventas",
            "ventas_2026"
        ],
        "tablas del diagrama, ordenadas por nombre"
    );

    let clientes = table(&graph, "clientes");
    assert_eq!(clientes.kind, NodeKind::Table);
    assert_eq!(clientes.comment.as_deref(), Some("Padrón de clientes"));
    assert_eq!(table(&graph, "ventas").kind, NodeKind::PartitionedTable);

    let id = clientes
        .columns
        .iter()
        .find(|c| c.name == "id")
        .expect("falta la columna id");
    assert!(id.primary_key);
    assert!(id.not_null);
    assert!(id.type_name.contains("bigint"), "{}", id.type_name);

    // Clave primaria compuesta: las dos columnas quedan marcadas.
    let sucursales = table(&graph, "sucursales");
    let claves: Vec<&str> = sucursales
        .columns
        .iter()
        .filter(|c| c.primary_key)
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(claves, vec!["empresa", "sucursal"]);

    // Clave foránea compuesta: los nombres van en el orden en que los escribe la restricción, no
    // en el de la tabla.
    let compuesta = edge(&graph, "empleados_sucursal_fk");
    assert_eq!(
        compuesta.source_columns,
        vec!["sucursal_num".to_owned(), "empresa_id".to_owned()]
    );
    assert_eq!(
        compuesta.target_columns,
        vec!["sucursal".to_owned(), "empresa".to_owned()]
    );
    assert_eq!(compuesta.on_update, RefAction::Cascade);
    assert_eq!(compuesta.on_delete, RefAction::SetNull);
    assert_eq!(compuesta.target, table(&graph, "sucursales").oid);
    assert!(compuesta.target_label.is_none());

    // Las columnas que participan de una clave foránea saliente quedan marcadas.
    let empleados = table(&graph, "empleados");
    for name in ["sucursal_num", "empresa_id", "jefe_id"] {
        let column = empleados
            .columns
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("falta la columna {name}"));
        assert!(column.foreign_key, "{name} debería estar marcada como FK");
    }
    assert!(
        !empleados
            .columns
            .iter()
            .find(|c| c.name == "id")
            .unwrap()
            .foreign_key
    );

    // Autorreferencia: una arista que empieza y termina en la misma tabla.
    let propia = graph
        .edges
        .iter()
        .find(|e| e.source == empleados.oid && e.target == empleados.oid)
        .expect("falta la autorreferencia de empleados");
    assert_eq!(propia.source_columns, vec!["jefe_id".to_owned()]);
    assert_eq!(propia.target_columns, vec!["id".to_owned()]);

    // Referencia que sale del esquema: la arista se conserva con el nombre completo de la referida
    // y sin nodo, porque esconderla mentiría sobre el modelo.
    let externa = graph
        .edges
        .iter()
        .find(|e| e.source == table(&graph, "pedidos").oid)
        .expect("falta la arista hacia el otro esquema");
    assert_eq!(
        externa.target_label.as_deref(),
        Some(format!("{schema_name}_ext.catalogo").as_str())
    );
    assert!(
        !graph.tables.iter().any(|t| t.oid == externa.target),
        "la tabla de otro esquema no debe aparecer como nodo"
    );

    // La partición hereda la clave foránea de su padre, pero el diagrama la dibuja una sola vez.
    let ventas = table(&graph, "ventas");
    let particion = table(&graph, "ventas_2026");
    let hacia_clientes: Vec<&GraphEdge> = graph
        .edges
        .iter()
        .filter(|e| e.target == clientes.oid)
        .collect();
    assert_eq!(
        hacia_clientes.len(),
        1,
        "la FK de la tabla particionada se duplicó por partición: {hacia_clientes:?}"
    );
    assert_eq!(hacia_clientes[0].source, ventas.oid);
    assert_eq!(hacia_clientes[0].on_delete, RefAction::Cascade);
    assert!(
        !graph.edges.iter().any(|e| e.source == particion.oid),
        "la partición no tiene aristas propias"
    );

    // Un esquema que no existe no puede devolver un diagrama vacío: sería indistinguible de uno
    // sin tablas.
    let error = introspect::schema_graph(handle, &database, "no_existe_este_esquema").await;
    assert!(error.is_err(), "un esquema inexistente tiene que fallar");
}

fn table<'a>(graph: &'a SchemaGraph, name: &str) -> &'a pgforge_core::introspect::GraphTable {
    graph
        .tables
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("falta la tabla «{name}» en el grafo"))
}

fn edge<'a>(graph: &'a SchemaGraph, name: &str) -> &'a GraphEdge {
    graph
        .edges
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("falta la restricción «{name}» en el grafo"))
}
