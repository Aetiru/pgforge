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
use pgforge_core::conn::{
    tunnel, ConnectionManager, ConnectionProfile, HostKeyPolicy, Password, ServerHandle, SshTunnel,
};
use pgforge_core::data;
use pgforge_core::ddl::RefAction;
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

    /// Imprime el grafo de relaciones de un esquema: las tablas y sus claves foráneas.
    Graph {
        #[arg(long)]
        url: String,
        /// Base donde vive el esquema. Por omisión, la de la cadena de conexión.
        #[arg(long)]
        database: Option<String>,
        /// Por ejemplo public
        schema: String,
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

    /// Exporta una tabla o una consulta a un archivo con COPY.
    Export {
        #[arg(long)]
        url: String,
        /// Archivo de salida.
        #[arg(long)]
        out: PathBuf,
        /// Tabla a exportar, como esquema.tabla. Excluyente con --sql.
        table: Option<String>,
        /// Consulta a exportar en vez de una tabla. Excluyente con la tabla.
        #[arg(long)]
        sql: Option<String>,
        #[arg(long, value_enum, default_value_t = CopyFormatArg::Csv)]
        format: CopyFormatArg,
        /// Primera línea con los nombres de columna (solo CSV).
        #[arg(long)]
        header: bool,
        /// Base sobre la que exportar. Por omisión, la del perfil.
        #[arg(long)]
        database: Option<String>,
        /// Muestra el COPY y no ejecuta nada.
        #[arg(long)]
        dry_run: bool,
    },

    /// Importa un archivo a una tabla con COPY.
    Import {
        #[arg(long)]
        url: String,
        /// Archivo de entrada.
        #[arg(long)]
        file: PathBuf,
        /// Tabla destino, como esquema.tabla.
        table: String,
        #[arg(long, value_enum, default_value_t = CopyFormatArg::Csv)]
        format: CopyFormatArg,
        /// La primera línea trae los nombres de columna y se saltea (solo CSV).
        #[arg(long)]
        header: bool,
        /// Base sobre la que importar. Por omisión, la del perfil.
        #[arg(long)]
        database: Option<String>,
        /// Muestra el COPY y no ejecuta nada.
        #[arg(long)]
        dry_run: bool,
    },

    /// Abre un túnel SSH a un servidor y deja el puerto local escuchando, para probarlo a mano.
    ///
    /// El secreto SSH (contraseña del bastión o frase de la clave) se lee de la variable de entorno
    /// PGFORGE_SSH_PASSWORD, para no dejarlo en el historial de comandos.
    Tunnel {
        /// Host del bastión SSH.
        #[arg(long)]
        ssh_host: String,
        #[arg(long, default_value_t = 22)]
        ssh_port: u16,
        #[arg(long)]
        ssh_user: String,
        /// Clave privada. Si se omite, se autentica por contraseña.
        #[arg(long)]
        key: Option<PathBuf>,
        /// Host del servidor destino, tal como lo ve el bastión.
        #[arg(long)]
        target_host: String,
        #[arg(long, default_value_t = 5432)]
        target_port: u16,
        /// Acepta y registra en known_hosts una clave de host desconocida, en vez de rechazarla.
        #[arg(long)]
        trust: bool,
        /// Solo prueba el túnel y sale, sin dejar el puerto abierto.
        #[arg(long)]
        test: bool,
    },
}

/// Los formatos de `COPY` que la CLI expone.
#[derive(Clone, Copy, ValueEnum)]
enum CopyFormatArg {
    Csv,
    Text,
    Binary,
}

impl From<CopyFormatArg> for data::CopyFormat {
    fn from(value: CopyFormatArg) -> Self {
        match value {
            CopyFormatArg::Csv => data::CopyFormat::Csv,
            CopyFormatArg::Text => data::CopyFormat::Text,
            CopyFormatArg::Binary => data::CopyFormat::Binary,
        }
    }
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
        Command::Graph {
            url,
            database,
            schema,
        } => graph(&url, database.as_deref(), &schema).await,
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
        Command::Export {
            url,
            out,
            table,
            sql,
            format,
            header,
            database,
            dry_run,
        } => run_export(&url, out, table, sql, format, header, database, dry_run).await,
        Command::Import {
            url,
            file,
            table,
            format,
            header,
            database,
            dry_run,
        } => run_import(&url, file, &table, format, header, database, dry_run).await,
        Command::Tunnel {
            ssh_host,
            ssh_port,
            ssh_user,
            key,
            target_host,
            target_port,
            trust,
            test,
        } => {
            run_tunnel(
                SshTunnel {
                    host: ssh_host,
                    port: ssh_port,
                    user: ssh_user,
                    private_key: key,
                },
                &target_host,
                target_port,
                trust,
                test,
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

#[allow(clippy::too_many_arguments)]
async fn run_export(
    url: &str,
    out: PathBuf,
    table: Option<String>,
    sql: Option<String>,
    format: CopyFormatArg,
    header: bool,
    database: Option<String>,
    dry_run: bool,
) -> Result<()> {
    let handle = connect(url).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let source = match (table, sql) {
        (Some(table), None) => {
            let (schema, table) = split_qualified(&table);
            data::ExportSource::Table {
                schema,
                table,
                columns: vec![],
            }
        }
        (None, Some(sql)) => data::ExportSource::Query { sql },
        (Some(_), Some(_)) => {
            return Err(Error::Config(
                "indicá una tabla o --sql, pero no ambos".to_owned(),
            ))
        }
        (None, None) => {
            return Err(Error::Config(
                "falta la tabla a exportar o --sql con una consulta".to_owned(),
            ))
        }
    };

    let spec = data::ExportSpec {
        source,
        format: format.into(),
        options: data::TextOptions {
            header,
            ..Default::default()
        },
    };

    println!("{}", data::export_command(&spec)?.sql);
    if dry_run {
        return Ok(());
    }

    let (progress, mut bytes_rx) = mpsc::channel(64);
    let printer = tokio::spawn(async move {
        while let Some(written) = bytes_rx.recv().await {
            eprint!("\r{} exportados", bytes(written));
        }
        eprintln!();
    });

    let (_cancel, never) = oneshot::channel();
    let outcome = data::export_to_file(&handle, &database, &spec, &out, progress, never).await;
    let _ = printer.await;
    let outcome = outcome?;

    println!("listo: {} ({})", out.display(), bytes(outcome.bytes));
    Ok(())
}

async fn run_import(
    url: &str,
    file: PathBuf,
    table: &str,
    format: CopyFormatArg,
    header: bool,
    database: Option<String>,
    dry_run: bool,
) -> Result<()> {
    let handle = connect(url).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let (schema, table) = split_qualified(table);
    let spec = data::ImportSpec {
        schema,
        table,
        columns: vec![],
        format: format.into(),
        options: data::TextOptions {
            header,
            ..Default::default()
        },
    };

    println!("{}", data::import_command(&spec)?.sql);
    if dry_run {
        return Ok(());
    }

    let (progress, mut bytes_rx) = mpsc::channel(64);
    let printer = tokio::spawn(async move {
        while let Some(read) = bytes_rx.recv().await {
            eprint!("\r{} leídos", bytes(read));
        }
        eprintln!();
    });

    let (_cancel, never) = oneshot::channel();
    let outcome = data::import_from_file(&handle, &database, &spec, &file, progress, never).await;
    let _ = printer.await;
    let outcome = outcome?;

    println!(
        "listo: {} filas ({})",
        outcome.rows.unwrap_or(0),
        bytes(outcome.bytes)
    );
    Ok(())
}

/// Parte «esquema.tabla» en sus dos mitades. Sin punto, el esquema es `public`, como en psql.
fn split_qualified(name: &str) -> (String, String) {
    match name.split_once('.') {
        Some((schema, table)) => (schema.to_owned(), table.to_owned()),
        None => ("public".to_owned(), name.to_owned()),
    }
}

/// Abre —o solo prueba— un túnel SSH. Sirve para ejercitar el forward sin levantar la ventana, que
/// es la única forma de probarlo de punta a punta (el test de integración no incluye un `sshd`).
async fn run_tunnel(
    spec: SshTunnel,
    target_host: &str,
    target_port: u16,
    trust: bool,
    test: bool,
) -> Result<()> {
    // El secreto va por entorno, no por argumento: los argumentos quedan en el historial del shell.
    let secret = std::env::var("PGFORGE_SSH_PASSWORD")
        .ok()
        .map(Password::new);
    let policy = if trust {
        HostKeyPolicy::TrustOnFirstUse
    } else {
        HostKeyPolicy::Strict
    };

    if test {
        tunnel::test_connection(&spec, secret.as_ref(), target_host, target_port, policy).await?;
        println!("túnel OK: el bastión alcanza {target_host}:{target_port}");
        return Ok(());
    }

    let forward =
        tunnel::open_tunnel(&spec, secret.as_ref(), target_host, target_port, policy).await?;
    let port = forward.local_port();
    println!("túnel abierto: 127.0.0.1:{port} -> {target_host}:{target_port}");
    println!("conectá con: pgforge query --url postgres://usuario@127.0.0.1:{port}/base --sql \"SELECT 1\"");
    println!("Ctrl-C para cerrar el túnel.");

    tokio::signal::ctrl_c()
        .await
        .map_err(|e| Error::Ssh(format!("no se pudo esperar la señal de cierre: {e}")))?;
    println!("cerrando el túnel.");
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

async fn graph(url: &str, database: Option<&str>, schema: &str) -> Result<()> {
    let handle = connect(url).await?;
    let database = database.unwrap_or_else(|| handle.default_database());
    let graph = introspect::schema_graph(&handle, database, schema).await?;

    for table in &graph.tables {
        println!("{}", table.name);
        for column in &table.columns {
            let mut marcas = Vec::new();
            if column.primary_key {
                marcas.push("PK");
            }
            if column.foreign_key {
                marcas.push("FK");
            }
            if column.not_null {
                marcas.push("not null");
            }

            let marcas = if marcas.is_empty() {
                String::new()
            } else {
                format!(" · {}", marcas.join(" "))
            };
            println!("  {} {}{marcas}", column.name, column.type_name);
        }
    }

    if graph.edges.is_empty() {
        println!("\n(sin claves foráneas)");
        return Ok(());
    }

    println!("\nreferencias:");
    for edge in &graph.edges {
        let origen = graph
            .tables
            .iter()
            .find(|t| t.oid == edge.source)
            .map(|t| t.name.as_str())
            .unwrap_or("?");
        // La referida puede estar fuera del esquema; ahí el nombre completo lo trae la arista.
        let destino = edge
            .target_label
            .clone()
            .or_else(|| {
                graph
                    .tables
                    .iter()
                    .find(|t| t.oid == edge.target)
                    .map(|t| t.name.clone())
            })
            .unwrap_or_else(|| "?".to_owned());

        println!(
            "  {origen}({}) -> {destino}({}) · {} · ON UPDATE {} ON DELETE {}",
            edge.source_columns.join(", "),
            edge.target_columns.join(", "),
            edge.name,
            accion(edge.on_update),
            accion(edge.on_delete),
        );
    }

    Ok(())
}

fn accion(action: RefAction) -> &'static str {
    match action {
        RefAction::NoAction => "NO ACTION",
        RefAction::Restrict => "RESTRICT",
        RefAction::Cascade => "CASCADE",
        RefAction::SetNull => "SET NULL",
        RefAction::SetDefault => "SET DEFAULT",
    }
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
