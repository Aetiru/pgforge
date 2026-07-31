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
