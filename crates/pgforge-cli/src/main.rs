//! Interfaz de línea de comandos de pgforge.
//!
//! Existe para que el núcleo sea verificable sin levantar la interfaz gráfica: si algo se puede
//! hacer desde la ventana pero no desde acá, es que la lógica se filtró a la capa equivocada.

// La CLI escribe a stdout porque esa es su salida. La restricción de `clippy.toml` apunta al
// núcleo, que no debe imprimir: quien lo consume decide cómo presentar los resultados.
#![allow(clippy::disallowed_macros)]

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::introspect::{self, NodeKind, TreeNode, TreeOptions};
use pgforge_core::sql::{self, ExplainOptions, Limits, Outcome, QuerySession};
use pgforge_core::{caps::MIN_SUPPORTED_VERSION_NUM, ddl, Error, Result, ServerVersion};

#[derive(Parser)]
#[command(name = "pgforge", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Muestra la versión y el rango de PostgreSQL soportado.
    Info,

    /// Conecta y describe el servidor.
    Server {
        /// Cadena de conexión, por ejemplo postgres://usuario@localhost:5432/base
        #[arg(long)]
        url: String,
    },

    /// Recorre el árbol de objetos.
    Tree {
        #[arg(long)]
        url: String,
        /// Cuántos niveles expandir.
        #[arg(long, default_value_t = 3)]
        depth: usize,
        /// Incluir pg_catalog, information_schema y los esquemas temporales.
        #[arg(long)]
        system: bool,
    },

    /// Imprime el DDL de un objeto, indicado como esquema.nombre
    Ddl {
        #[arg(long)]
        url: String,
        /// Por ejemplo public.clientes
        object: String,
    },

    /// Ejecuta SQL y muestra el resultado.
    Query {
        #[arg(long)]
        url: String,
        /// El script a ejecutar. Puede tener varias sentencias separadas por punto y coma.
        #[arg(long)]
        sql: String,
        /// Base sobre la que ejecutar. Por omisión, la del perfil.
        #[arg(long)]
        database: Option<String>,
        /// Cuántas filas traer como máximo.
        #[arg(long, default_value_t = sql::DEFAULT_MAX_ROWS)]
        max_rows: usize,
        /// Muestra el plan de ejecución en vez de ejecutar la consulta.
        #[arg(long)]
        explain: bool,
        /// Con --explain, mide tiempos reales. Ojo: ejecuta la sentencia.
        #[arg(long)]
        analyze: bool,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Info => {
            info();
            Ok(())
        }
        Command::Server { url } => server(&url).await,
        Command::Tree { url, depth, system } => tree(&url, depth, system).await,
        Command::Ddl { url, object } => show_ddl(&url, &object).await,
        Command::Query {
            url,
            sql,
            database,
            max_rows,
            explain,
            analyze,
        } => {
            let options = explain.then_some(ExplainOptions {
                analyze,
                buffers: analyze,
                verbose: false,
            });
            query(&url, &sql, database.as_deref(), max_rows, options).await
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn info() {
    let min = ServerVersion::from_num(MIN_SUPPORTED_VERSION_NUM);
    println!("pgforge {}", env!("CARGO_PKG_VERSION"));
    println!("PostgreSQL soportado: {} o superior", min.major());
    match ddl::pg_dump::find_binary() {
        Some(path) => println!("pg_dump: {}", path.display()),
        None => println!("pg_dump: no encontrado (el DDL de tablas no va a estar disponible)"),
    }
}

async fn connect(url: &str) -> Result<std::sync::Arc<ServerHandle>> {
    let (profile, password) = ConnectionProfile::from_url("cli", url)?;
    let manager = ConnectionManager::new();
    manager.connect(profile, password).await
}

async fn server(url: &str) -> Result<()> {
    let handle = connect(url).await?;
    let caps = &handle.caps;

    println!("servidor:    {}", handle.profile.target());
    println!("versión:     PostgreSQL {}", caps.version);
    println!("usuario:     {}", caps.current_user);
    println!("base:        {}", caps.current_database);
    println!("superusuario: {}", si_no(caps.is_superuser));
    println!(
        "puede señalar backends: {}",
        si_no(caps.can_signal_backends)
    );
    println!(
        "puede leer todas las estadísticas: {}",
        si_no(caps.can_read_all_stats)
    );

    println!("\nbases:");
    for db in handle.databases().await? {
        println!("  {:<24} {:<16} {}", db.name, db.owner, db.encoding);
    }
    Ok(())
}

fn si_no(value: bool) -> &'static str {
    if value {
        "sí"
    } else {
        "no"
    }
}

async fn tree(url: &str, depth: usize, system: bool) -> Result<()> {
    let handle = connect(url).await?;
    let options = TreeOptions {
        show_system_schemas: system,
    };

    // Recorrido con pila explícita en vez de recursión: un futuro recursivo necesita boxearse en
    // cada nivel y acá no aporta nada.
    let roots = introspect::children(&handle, None, options).await?;
    let mut stack: Vec<(TreeNode, usize)> = roots.into_iter().rev().map(|node| (node, 0)).collect();

    while let Some((node, level)) = stack.pop() {
        let sangria = "  ".repeat(level);
        match &node.detail {
            Some(detail) => println!("{sangria}{} · {detail}", node.label),
            None => println!("{sangria}{}", node.label),
        }

        if level + 1 < depth && node.has_children {
            let children = introspect::children(&handle, Some(&node), options).await?;
            for child in children.into_iter().rev() {
                stack.push((child, level + 1));
            }
        }
    }

    Ok(())
}

async fn show_ddl(url: &str, object: &str) -> Result<()> {
    let (schema_name, object_name) = object.split_once('.').ok_or_else(|| {
        Error::Config(
            "indicá el objeto como esquema.nombre, por ejemplo public.clientes".to_owned(),
        )
    })?;

    let handle = connect(url).await?;
    let node = find_object(&handle, schema_name, object_name).await?;
    let ddl = ddl::object_ddl(&handle, &node).await?;

    println!("{}", ddl.sql);
    Ok(())
}

async fn query(
    url: &str,
    script: &str,
    database: Option<&str>,
    max_rows: usize,
    explain: Option<ExplainOptions>,
) -> Result<()> {
    let handle = connect(url).await?;
    let database = database.unwrap_or_else(|| handle.default_database());
    let session = QuerySession::open(&handle, database).await?;

    let statements = sql::split(script);
    if statements.is_empty() {
        return Err(Error::Config(
            "el script no tiene ninguna sentencia".to_owned(),
        ));
    }

    for (index, statement) in statements.iter().enumerate() {
        if statements.len() > 1 {
            println!(
                "-- [{}/{}] línea {}",
                index + 1,
                statements.len(),
                statement.line
            );
        }

        match explain {
            Some(options) => {
                if let Some(aviso) = sql::explain::warning(&statement.text, options) {
                    eprintln!("aviso: {aviso}");
                }
                print_plan(&sql::explain::explain(&session, &statement.text, options).await?);
            }
            // Se corta en el primer error: seguir con las que faltan es lo que convierte un script
            // a medio aplicar en un problema difícil de reconstruir.
            None => print_outcome(&session.run(&statement.text, Limits { max_rows }).await?),
        }
    }

    Ok(())
}

fn print_plan(plan: &sql::Plan) {
    fn walk(node: &sql::PlanNode, level: usize) {
        let sangria = "  ".repeat(level);
        let objeto = match (&node.index, &node.relation) {
            (Some(index), Some(relation)) => format!(" sobre {relation} vía {index}"),
            (_, Some(relation)) => format!(" sobre {relation}"),
            _ => String::new(),
        };

        println!(
            "{sangria}{}{objeto}  (costo {:.2})",
            node.node_type, node.total_cost
        );

        match (node.actual_rows, node.self_ms) {
            (Some(rows), Some(propio)) => println!(
                "{sangria}  filas {} estimadas / {} reales{}  ·  propio {:.3} ms",
                node.plan_rows,
                rows,
                if node.misestimated { " ⚠" } else { "" },
                propio
            ),
            _ => println!("{sangria}  filas {} estimadas", node.plan_rows),
        }

        if let Some(condition) = &node.condition {
            println!("{sangria}  {condition}");
        }

        for child in &node.children {
            walk(child, level + 1);
        }
    }

    walk(&plan.root, 0);

    if let Some(planning) = plan.planning_ms {
        println!("planificación: {planning:.3} ms");
    }
    if let Some(execution) = plan.execution_ms {
        println!("ejecución: {execution:.3} ms");
    }
}

fn print_outcome(outcome: &Outcome) {
    match outcome {
        Outcome::Command {
            tag,
            affected,
            seconds,
        } => println!("{tag}: {affected} · {seconds:.3} s"),

        Outcome::Rows {
            columns,
            rows,
            row_count,
            truncated,
            seconds,
        } => {
            print_grid(columns, rows);
            if *truncated {
                println!(
                    "({row_count} filas, se muestran {} · {seconds:.3} s)",
                    rows.len()
                );
            } else {
                println!("({row_count} filas · {seconds:.3} s)");
            }
        }
    }
}

/// Grilla de ancho fijo por columna, acotada para que una columna con un JSON adentro no rompa la
/// alineación de todo lo demás.
fn print_grid(columns: &[String], rows: &[Vec<Option<String>>]) {
    const MAX_WIDTH: usize = 40;
    const NULL: &str = "∅";

    let cell = |value: &Option<String>| value.as_deref().unwrap_or(NULL).replace('\n', " ");

    let widths: Vec<usize> = columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            rows.iter()
                .filter_map(|row| row.get(index))
                .map(|value| cell(value).chars().count())
                .chain(std::iter::once(column.chars().count()))
                .max()
                .unwrap_or(0)
                .min(MAX_WIDTH)
        })
        .collect();

    let line = |values: Vec<String>| {
        let padded: Vec<String> = values
            .iter()
            .zip(&widths)
            .map(|(value, width)| {
                let mut text: String = value.chars().take(*width).collect();
                let padding = width.saturating_sub(text.chars().count());
                text.extend(std::iter::repeat_n(' ', padding));
                text
            })
            .collect();
        println!("{}", padded.join(" | ").trim_end());
    };

    line(columns.to_vec());
    println!(
        "{}",
        widths
            .iter()
            .map(|w| "-".repeat(*w))
            .collect::<Vec<_>>()
            .join("-+-")
    );
    for row in rows {
        line(row.iter().map(cell).collect());
    }
}

/// Busca el objeto recorriendo las carpetas del esquema. Es el mismo camino que hace la interfaz
/// al expandir el árbol, con lo cual verifica el recorrido además del DDL.
async fn find_object(handle: &ServerHandle, schema: &str, object: &str) -> Result<TreeNode> {
    let options = TreeOptions {
        show_system_schemas: true,
    };

    let database = TreeNode::database(handle.default_database().to_owned());
    let db_children = introspect::children(handle, Some(&database), options).await?;

    let schemas_folder = db_children
        .first()
        .ok_or_else(|| Error::Config("la base no devolvió esquemas".to_owned()))?;
    let schemas = introspect::children(handle, Some(schemas_folder), options).await?;

    let schema_node = schemas
        .iter()
        .find(|node| node.label == schema)
        .ok_or_else(|| Error::Config(format!("no existe el esquema {schema}")))?;

    for folder in introspect::children(handle, Some(schema_node), options).await? {
        if !folder.has_children || !matches!(folder.kind, NodeKind::Folder(_)) {
            continue;
        }
        let found = introspect::children(handle, Some(&folder), options)
            .await?
            .into_iter()
            // Las funciones se listan con su firma, así que alcanza con que empiece con el nombre.
            .find(|node| node.label == object || node.label.starts_with(&format!("{object}(")));

        if let Some(node) = found {
            return Ok(node);
        }
    }

    Err(Error::Config(format!("no se encontró {schema}.{object}")))
}
