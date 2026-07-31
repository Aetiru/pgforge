//! Interfaz de línea de comandos de pgforge.
//!
//! Existe para que el núcleo sea verificable sin levantar la interfaz gráfica. Los subcomandos de
//! conexión e introspección se agregan junto con los módulos correspondientes del core.

// La CLI escribe a stdout porque esa es su salida. La restricción de `clippy.toml` apunta al
// núcleo, que no debe imprimir: quien lo consume decide cómo presentar los resultados.
#![allow(clippy::disallowed_macros)]

use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pgforge", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Muestra la versión y las versiones de PostgreSQL soportadas.
    Info,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Info => {
            let min = pgforge_core::ServerVersion::from_num(
                pgforge_core::caps::MIN_SUPPORTED_VERSION_NUM,
            );
            println!("pgforge {}", env!("CARGO_PKG_VERSION"));
            println!("PostgreSQL soportado: {} o superior", min.major());
            ExitCode::SUCCESS
        }
    }
}
