//! Interfaz de línea de comandos de pgforge.
//!
//! Existe para que el núcleo sea verificable sin levantar la interfaz gráfica: si algo se puede
//! hacer desde la ventana pero no desde acá, es que la lógica se filtró a la capa equivocada.

// La CLI escribe a stdout porque esa es su salida. La restricción de `clippy.toml` apunta al
// núcleo, que no debe imprimir: quien lo consume decide cómo presentar los resultados.
#![allow(clippy::disallowed_macros)]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use pgforge_core::backup::restore::{self, RestoreOptions};
use pgforge_core::backup::tools::Tool;
use pgforge_core::backup::{self, BackupOptions, Format};
use pgforge_core::conn::{ConnectionManager, ConnectionProfile, ServerHandle};
use pgforge_core::data;
use pgforge_core::introspect::{self, NodeKind, TreeNode, TreeOptions};
use pgforge_core::sql::{self, ExplainOptions, Limits, Outcome, QuerySession};
use pgforge_core::{caps::MIN_SUPPORTED_VERSION_NUM, ddl, Error, Result, ServerVersion};
use tokio::sync::{mpsc, oneshot};

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

    /// Muestra los datos de una tabla, indicada como esquema.nombre
    Data {
        #[arg(long)]
        url: String,
        /// Por ejemplo public.clientes
        table: String,
        /// Cuántas filas traer.
        #[arg(long, default_value_t = data::DEFAULT_PAGE_SIZE)]
        limit: usize,
        /// Valores de clave de la última fila vista, separados por coma, para pedir la página
        /// siguiente.
        #[arg(long, value_delimiter = ',')]
        after: Option<Vec<String>>,
    },

    /// Hace un backup con pg_dump.
    Backup {
        #[arg(long)]
        url: String,
        /// Archivo de salida, o directorio con --format directory.
        #[arg(long)]
        out: PathBuf,
        /// Base a respaldar. Por omisión, la del perfil.
        #[arg(long)]
        database: Option<String>,
        #[arg(long, value_enum, default_value_t = FormatArg::Custom)]
        format: FormatArg,
        /// Respaldar solo estos esquemas. Se puede repetir.
        #[arg(long = "schema")]
        schemas: Vec<String>,
        /// Respaldar solo estas tablas, como esquema.tabla. Se puede repetir.
        #[arg(long = "table")]
        tables: Vec<String>,
        /// Sin los datos.
        #[arg(long)]
        schema_only: bool,
        /// Sin el esquema.
        #[arg(long)]
        data_only: bool,
        /// Nivel de compresión, de 0 a 9 (formatos custom y directory).
        #[arg(long)]
        compress: Option<u8>,
        /// Muestra la línea de comando y no ejecuta nada.
        #[arg(long)]
        dry_run: bool,
    },

    /// Restaura un backup con pg_restore.
    Restore {
        #[arg(long)]
        url: String,
        /// El archivo del backup, o el directorio con --format directory.
        #[arg(long)]
        source: PathBuf,
        /// Base destino, o de mantenimiento con --create. Por omisión, la del perfil.
        #[arg(long)]
        database: Option<String>,
        #[arg(long, value_enum, default_value_t = FormatArg::Custom)]
        format: FormatArg,
        /// Restaurar solo estos esquemas. Se puede repetir.
        #[arg(long = "schema")]
        schemas: Vec<String>,
        /// Restaurar solo estas tablas, como esquema.tabla. Se puede repetir.
        #[arg(long = "table")]
        tables: Vec<String>,
        /// Sin los datos.
        #[arg(long)]
        schema_only: bool,
        /// Sin el esquema.
        #[arg(long)]
        data_only: bool,
        /// Elimina cada objeto antes de recrearlo.
        #[arg(long)]
        clean: bool,
        /// Que el borrado de --clean no falle si el objeto no existe.
        #[arg(long)]
        if_exists: bool,
        /// Crea la base destino en vez de cargar sobre una existente.
        #[arg(long)]
        create: bool,
        /// Todo o nada: revierte si algo falla.
        #[arg(long)]
        single_transaction: bool,
        /// Trabajos en paralelo (formatos custom y directory).
        #[arg(long)]
        jobs: Option<u8>,
        /// Muestra la línea de comando y no ejecuta nada.
        #[arg(long)]
        dry_run: bool,
    },
}

/// Los formatos de `pg_dump`, con los nombres que usa su propia documentación.
#[derive(Clone, Copy, ValueEnum)]
enum FormatArg {
    Plain,
    Custom,
    Directory,
    Tar,
}

impl From<FormatArg> for Format {
    fn from(value: FormatArg) -> Self {
        match value {
            FormatArg::Plain => Format::Plain,
            FormatArg::Custom => Format::Custom,
            FormatArg::Directory => Format::Directory,
            FormatArg::Tar => Format::Tar,
        }
    }
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
        Command::Data {
            url,
            table,
            limit,
            after,
        } => show_data(&url, &table, limit, after).await,
        Command::Backup {
            url,
            out,
            database,
            format,
            schemas,
            tables,
            schema_only,
            data_only,
            compress,
            dry_run,
        } => {
            run_backup(
                &url,
                BackupOptions {
                    // Se completa con la base del perfil una vez conectados.
                    database: database.unwrap_or_default(),
                    format: format.into(),
                    path: out,
                    schemas,
                    exclude_schemas: vec![],
                    tables,
                    schema_only,
                    data_only,
                    no_owner: false,
                    no_privileges: false,
                    compression: compress,
                    jobs: None,
                },
                dry_run,
            )
            .await
        }
        Command::Restore {
            url,
            source,
            database,
            format,
            schemas,
            tables,
            schema_only,
            data_only,
            clean,
            if_exists,
            create,
            single_transaction,
            jobs,
            dry_run,
        } => {
            run_restore(
                &url,
                RestoreOptions {
                    source,
                    format: format.into(),
                    // Se completa con la base del perfil una vez conectados.
                    database: database.unwrap_or_default(),
                    schemas,
                    tables,
                    schema_only,
                    data_only,
                    clean,
                    if_exists,
                    create,
                    no_owner: false,
                    no_privileges: false,
                    single_transaction,
                    jobs,
                },
                dry_run,
            )
            .await
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
    match backup::tools::find(Tool::PgDump) {
        Some(path) => println!("pg_dump: {}", path.display()),
        None => println!("pg_dump: no encontrado (sin DDL de tablas ni backups)"),
    }
    match backup::tools::find(Tool::PgRestore) {
        Some(path) => println!("pg_restore: {}", path.display()),
        None => println!("pg_restore: no encontrado"),
    }
}

async fn run_backup(url: &str, mut options: BackupOptions, dry_run: bool) -> Result<()> {
    let handle = connect(url).await?;
    if options.database.is_empty() {
        options.database = handle.default_database().to_owned();
    }

    let plan = backup::plan(&handle, &options).await?;
    println!("{}", plan.command.join(" "));
    if let Some(warning) = &plan.warning {
        eprintln!("aviso: {warning}");
    }
    if dry_run {
        return Ok(());
    }

    // El progreso se imprime a medida que llega, que es de lo que se trata `--verbose`.
    let (progress, mut lines) = mpsc::channel(64);
    let printer = tokio::spawn(async move {
        while let Some(line) = lines.recv().await {
            eprintln!("{line}");
        }
    });

    // La CLI no tiene cómo cancelar: el extremo que envía se suelta acá mismo y la espera nunca se
    // resuelve, que es exactamente lo que hace falta.
    let (_cancel, never) = oneshot::channel();

    let outcome = backup::run(&handle, &options, progress, never).await;
    let _ = printer.await;
    let outcome = outcome?;

    println!(
        "listo: {} ({}) en {:.1}s",
        outcome.path.display(),
        bytes(outcome.bytes),
        outcome.seconds
    );
    Ok(())
}

async fn run_restore(url: &str, mut options: RestoreOptions, dry_run: bool) -> Result<()> {
    let handle = connect(url).await?;
    if options.database.is_empty() {
        options.database = handle.default_database().to_owned();
    }

    let plan = restore::plan(&handle, &options).await?;
    println!("{}", plan.command.join(" "));
    if let Some(warning) = &plan.warning {
        eprintln!("aviso: {warning}");
    }
    if dry_run {
        return Ok(());
    }

    let (progress, mut lines) = mpsc::channel(64);
    let printer = tokio::spawn(async move {
        while let Some(line) = lines.recv().await {
            eprintln!("{line}");
        }
    });

    // La CLI no tiene cómo cancelar: el extremo que envía se suelta acá mismo y la espera nunca se
    // resuelve, que es exactamente lo que hace falta.
    let (_cancel, never) = oneshot::channel();

    let outcome = restore::run(&handle, &options, progress, never).await;
    let _ = printer.await;
    let outcome = outcome?;

    print!(
        "listo: se restauró sobre {} en {:.1}s",
        outcome.database, outcome.seconds
    );
    if outcome.ignored_errors > 0 {
        print!(
            " (se ignoraron {} errores; ver el detalle arriba)",
            outcome.ignored_errors
        );
    }
    println!();
    Ok(())
}

/// Tamaño legible. La interfaz tiene su propia versión en `format.ts`; acá alcanza con esto.
fn bytes(value: u64) -> String {
    const UNITS: [&str; 4] = ["B", "kB", "MB", "GB"];
    let mut size = value as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
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

async fn show_data(url: &str, table: &str, limit: usize, after: Option<Vec<String>>) -> Result<()> {
    let (schema_name, table_name) = table.split_once('.').ok_or_else(|| {
        Error::Config("indicá la tabla como esquema.nombre, por ejemplo public.clientes".to_owned())
    })?;

    let handle = connect(url).await?;
    let node = find_object(&handle, schema_name, table_name).await?;
    let oid = node
        .oid
        .ok_or_else(|| Error::Config(format!("{table} no tiene oid en el catálogo")))?;

    let database = handle.default_database().to_owned();
    let shape = data::shape(&handle, &database, oid).await?;

    match (&shape.key, &shape.read_only) {
        (Some(key), None) => println!("-- editable por {} ({})", key.name, key.columns.join(", ")),
        (_, Some(motivo)) => println!("-- solo lectura: {motivo}"),
        (None, None) => {}
    }

    let cursor = after.map(|key| data::Cursor::After { key });
    let page = data::page(&handle, &database, &shape, cursor.as_ref(), limit).await?;

    print_grid(&page.columns, &page.rows);
    println!("({} filas)", page.rows.len());

    // Se imprime el cursor para poder encadenar la llamada siguiente a mano, que es justamente lo
    // que hace verificable la paginación desde la línea de comandos.
    match page.next {
        Some(data::Cursor::After { key }) => println!("-- siguiente: --after {}", key.join(",")),
        Some(data::Cursor::Offset { rows }) => println!("-- siguiente: offset {rows}"),
        None => println!("-- no hay más filas"),
    }

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
