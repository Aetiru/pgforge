//! Backups con `pg_dump`.
//!
//! Igual que el resto de las mutaciones, esto se parte en dos: [`arguments`] arma la línea de
//! comando y es pura —lo único verificable sin servidor, y la garantía de que lo que la interfaz
//! muestra es exactamente lo que se va a ejecutar— y [`run`] la ejecuta.
//!
//! `pg_dump` no informa nada mientras trabaja salvo que se le pase `--verbose`, y aun así lo
//! escribe por stderr. Por eso el progreso se transmite línea a línea a medida que llega, que es lo
//! mismo que hace el mantenimiento con los `NOTICE` del servidor: un backup de media hora sin una
//! sola señal de vida es indistinguible de uno colgado.

pub mod tools;

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};

use crate::conn::profile::{ConnectionProfile, SslMode};
use crate::conn::ServerHandle;
use crate::error::{Error, Result};
use tools::Tool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Format {
    /// Un script SQL. Se restaura con `psql`, no con `pg_restore`.
    Plain,
    /// El formato propio de `pg_dump`: comprimido y con restore selectivo.
    Custom,
    /// Un directorio con un archivo por tabla. Es el único que admite paralelismo.
    Directory,
    Tar,
}

impl Format {
    fn flag(self) -> &'static str {
        match self {
            Format::Plain => "p",
            Format::Custom => "c",
            Format::Directory => "d",
            Format::Tar => "t",
        }
    }

    fn admite_compresion(self) -> bool {
        matches!(self, Format::Custom | Format::Directory)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupOptions {
    pub database: String,
    pub format: Format,
    /// Archivo de salida, o directorio en el formato correspondiente.
    pub path: PathBuf,
    /// Vacío quiere decir todos los esquemas, que es lo que hace `pg_dump` sin `--schema`.
    #[serde(default)]
    pub schemas: Vec<String>,
    #[serde(default)]
    pub exclude_schemas: Vec<String>,
    /// Los nombres van calificados (`public.clientes`) o sin calificar, tal como los toma
    /// `--table`. Son patrones para `pg_dump`, no identificadores: ver [`quote_pattern`].
    #[serde(default)]
    pub tables: Vec<String>,
    #[serde(default)]
    pub schema_only: bool,
    #[serde(default)]
    pub data_only: bool,
    #[serde(default)]
    pub no_owner: bool,
    #[serde(default)]
    pub no_privileges: bool,
    pub compression: Option<u8>,
    /// Trabajos en paralelo. Solo el formato directorio los admite.
    pub jobs: Option<u8>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPlan {
    /// El binario y sus argumentos, listos para mostrar. La contraseña no está acá: viaja por
    /// `PGPASSWORD`, así que esto se puede enseñar y copiar sin filtrar nada.
    pub command: Vec<String>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub path: PathBuf,
    pub bytes: u64,
    pub seconds: f64,
}

/// Los argumentos de `pg_dump`, sin el binario.
///
/// Es una función pura a propósito: valida acá lo que `pg_dump` rechazaría recién al arrancar, y
/// deja que la interfaz muestre la línea exacta antes de lanzar nada.
pub fn arguments(profile: &ConnectionProfile, options: &BackupOptions) -> Result<Vec<String>> {
    if options.path.as_os_str().is_empty() {
        return Err(Error::Config(
            "hace falta indicar dónde guardar el backup".to_owned(),
        ));
    }
    if options.schema_only && options.data_only {
        return Err(Error::Config(
            "«solo el esquema» y «solo los datos» se excluyen entre sí".to_owned(),
        ));
    }
    if let Some(jobs) = options.jobs {
        if jobs == 0 {
            return Err(Error::Config(
                "la cantidad de trabajos en paralelo tiene que ser al menos 1".to_owned(),
            ));
        }
        if jobs > 1 && options.format != Format::Directory {
            return Err(Error::Config(
                "solo el formato directorio admite varios trabajos en paralelo".to_owned(),
            ));
        }
    }
    if let Some(level) = options.compression {
        if level > 9 {
            return Err(Error::Config(
                "el nivel de compresión va de 0 a 9".to_owned(),
            ));
        }
        if !options.format.admite_compresion() {
            return Err(Error::Config(
                "solo los formatos custom y directorio se comprimen".to_owned(),
            ));
        }
    }

    let mut args = vec![
        format!("--host={}", profile.host),
        format!("--port={}", profile.port),
        format!("--username={}", profile.user),
        format!("--dbname={}", options.database),
        format!("--format={}", options.format.flag()),
        format!("--file={}", options.path.display()),
        // Sin esto no hay una sola línea de progreso.
        "--verbose".to_owned(),
        // Si no, `pg_dump` se queda esperando una contraseña por consola que nadie va a escribir.
        "--no-password".to_owned(),
    ];

    for schema in &options.schemas {
        args.push(format!("--schema={}", quote_pattern(schema)));
    }
    for schema in &options.exclude_schemas {
        args.push(format!("--exclude-schema={}", quote_pattern(schema)));
    }
    for table in &options.tables {
        args.push(format!("--table={}", quote_pattern(table)));
    }

    if options.schema_only {
        args.push("--schema-only".to_owned());
    }
    if options.data_only {
        args.push("--data-only".to_owned());
    }
    if options.no_owner {
        args.push("--no-owner".to_owned());
    }
    if options.no_privileges {
        args.push("--no-privileges".to_owned());
    }
    if let Some(level) = options.compression {
        args.push(format!("--compress={level}"));
    }
    if let Some(jobs) = options.jobs {
        args.push(format!("--jobs={jobs}"));
    }

    Ok(args)
}

/// Lo que conviene saber antes de lanzar el backup, o `None` si no hay nada que decir.
pub fn warning(options: &BackupOptions) -> Option<&'static str> {
    if options.data_only {
        return Some(
            "Un backup solo de datos no alcanza para recrear la base: no lleva las tablas, los \
             índices ni las restricciones.",
        );
    }
    if options.format == Format::Plain {
        return Some(
            "El formato plano es un script SQL: se restaura con psql y no permite elegir qué \
             restaurar. Para un restore selectivo hace falta el formato custom o directorio.",
        );
    }
    None
}

/// La línea de comando completa y su advertencia, con el binario ya ubicado.
///
/// Es acá y no en [`arguments`] donde se compara la versión del binario con la del servidor: es lo
/// único de esta función que necesita tocar el disco, y es la falla más común —`pg_dump` lee
/// servidores más viejos que él, nunca más nuevos— que además falla tarde y con un mensaje que no
/// dice qué hacer.
pub async fn plan(handle: &ServerHandle, options: &BackupOptions) -> Result<BackupPlan> {
    let binary = tools::require(Tool::PgDump)?;
    let version = tools::version(&binary).await?;
    let server = handle.caps.version;

    if version.major() < server.major() {
        return Err(Error::Config(format!(
            "el pg_dump encontrado es {version} y el servidor es {server}: pg_dump puede leer \
             servidores más viejos que él, pero no más nuevos. Instalá las herramientas cliente \
             de PostgreSQL {} o indicá la ruta en PGFORGE_PG_DUMP.",
            server.major()
        )));
    }

    let mut command = vec![binary.display().to_string()];
    command.extend(arguments(&handle.profile, options)?);

    Ok(BackupPlan {
        command,
        warning: warning(options).map(str::to_owned),
    })
}

/// Ejecuta el backup, transmitiendo el progreso por `progress` y abortando si llega algo por
/// `cancel`.
///
/// Sin límite de tiempo, a diferencia de [`crate::ddl::pg_dump::dump_object`]: un backup tarda lo
/// que tarda, y matarlo a los treinta segundos sería peor que no tenerlo.
pub async fn run(
    handle: &ServerHandle,
    options: &BackupOptions,
    progress: mpsc::Sender<String>,
    cancel: oneshot::Receiver<()>,
) -> Result<Outcome> {
    let plan = plan(handle, options).await?;
    let (binary, args) = plan.command.split_first().expect("el plan trae el binario");

    let mut command = Command::new(binary);
    command
        .args(args)
        .env("PGSSLMODE", ssl_mode_env(handle.profile.ssl_mode))
        .env("PGAPPNAME", "pgforge")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    if let Some(cert) = &handle.profile.root_cert {
        command.env("PGSSLROOTCERT", cert.as_os_str());
    }
    if let Some(password) = handle.password() {
        command.env("PGPASSWORD", password.expose());
    }

    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|e| Error::Config(format!("no se pudo ejecutar {binary}: {e}")))?;

    // Las últimas líneas de stderr son las que explican un fallo, así que se guardan además de
    // transmitirse: cuando `pg_dump` termina mal, su código de salida no dice nada por sí solo.
    let stderr = child.stderr.take().expect("stderr pedido como pipe");
    let (tail_tx, mut tail_rx) = mpsc::channel::<String>(16);
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = progress.send(line.clone()).await;
            let _ = tail_tx.send(line).await;
        }
    });

    let mut cancel = cancel;
    let mut tail = Tail::default();
    let status = loop {
        tokio::select! {
            biased;

            line = tail_rx.recv() => match line {
                Some(line) => tail.push(line),
                // stderr llegó a su fin: solo queda esperar a que el proceso termine.
                None => break child.wait().await,
            },

            status = child.wait() => break status,

            // Matar el proceso y no solo dejar de esperarlo: si no, `pg_dump` seguiría escribiendo
            // el archivo que la aplicación ya dio por cancelado.
            _ = &mut cancel => {
                let _ = child.kill().await;
                cleanup(options);
                return Err(Error::Canceled);
            }
        }
    };

    // El proceso ya terminó, pero pueden quedar líneas sin leer: son justo las que explican el
    // fallo, si lo hubo.
    while let Some(line) = tail_rx.recv().await {
        tail.push(line);
    }

    let status = status.map_err(|e| Error::Config(format!("falló la espera de pg_dump: {e}")))?;

    if !status.success() {
        cleanup(options);
        return Err(Error::Config(format!(
            "pg_dump terminó con error: {}",
            tail.last().unwrap_or("sin detalle")
        )));
    }

    let bytes = size_of(options);
    Ok(Outcome {
        path: options.path.clone(),
        bytes,
        seconds: started.elapsed().as_secs_f64(),
    })
}

/// Las últimas líneas de stderr.
///
/// `pg_dump` con `--verbose` escribe una línea por objeto: guardarlas todas para poder citar la
/// última sería quedarse con miles de líneas que nadie va a leer.
#[derive(Default)]
struct Tail(Vec<String>);

impl Tail {
    const MAX: usize = 5;

    fn push(&mut self, line: String) {
        if self.0.len() == Self::MAX {
            self.0.remove(0);
        }
        self.0.push(line);
    }

    fn last(&self) -> Option<&str> {
        self.0.last().map(String::as_str)
    }
}

fn ssl_mode_env(mode: SslMode) -> &'static str {
    match mode {
        SslMode::Disable => "disable",
        SslMode::Prefer => "prefer",
        SslMode::Require => "require",
        SslMode::VerifyCa => "verify-ca",
        SslMode::VerifyFull => "verify-full",
    }
}

/// Borra la salida a medio escribir.
///
/// Un backup truncado que parece válido es peor que no tener backup: el error aparece el día que
/// hace falta restaurar, que es el peor momento posible para descubrirlo.
fn cleanup(options: &BackupOptions) {
    let _ = if options.format == Format::Directory {
        std::fs::remove_dir_all(&options.path)
    } else {
        std::fs::remove_file(&options.path)
    };
}

/// Tamaño de lo generado. En el formato directorio hay que sumar los archivos que quedaron adentro.
fn size_of(options: &BackupOptions) -> u64 {
    if options.format != Format::Directory {
        return std::fs::metadata(&options.path)
            .map(|meta| meta.len())
            .unwrap_or(0);
    }

    let Ok(entries) = std::fs::read_dir(&options.path) else {
        return 0;
    };
    entries
        .flatten()
        .filter_map(|entry| entry.metadata().ok())
        .map(|meta| meta.len())
        .sum()
}

/// `pg_dump` interpreta `--schema` y `--table` como patrones, así que los comodines de un nombre
/// literal tienen que quedar dentro de comillas dobles. Mismo criterio que
/// [`crate::ddl::pg_dump::table_pattern`].
fn quote_pattern(name: &str) -> String {
    if name.contains('.') {
        let (schema, table) = name.split_once('.').expect("tiene un punto");
        return format!("{}.{}", quote_pattern(schema), quote_pattern(table));
    }
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> ConnectionProfile {
        ConnectionProfile::from_url("test", "postgres://ana@servidor:5433/ventas")
            .expect("URL válida")
            .0
    }

    fn options(format: Format) -> BackupOptions {
        BackupOptions {
            database: "ventas".into(),
            format,
            path: PathBuf::from("/tmp/ventas.dump"),
            schemas: vec![],
            exclude_schemas: vec![],
            tables: vec![],
            schema_only: false,
            data_only: false,
            no_owner: false,
            no_privileges: false,
            compression: None,
            jobs: None,
        }
    }

    fn args(options: &BackupOptions) -> Vec<String> {
        arguments(&profile(), options).expect("tenía que armar los argumentos")
    }

    #[test]
    fn arma_la_linea_basica() {
        let args = args(&options(Format::Custom));
        assert!(args.contains(&"--host=servidor".to_owned()));
        assert!(args.contains(&"--port=5433".to_owned()));
        assert!(args.contains(&"--username=ana".to_owned()));
        assert!(args.contains(&"--dbname=ventas".to_owned()));
        assert!(args.contains(&"--format=c".to_owned()));
        assert!(args.contains(&"--verbose".to_owned()));
        assert!(args.contains(&"--no-password".to_owned()));
    }

    #[test]
    fn cada_formato_tiene_su_letra() {
        assert!(args(&options(Format::Plain)).contains(&"--format=p".to_owned()));
        assert!(args(&options(Format::Custom)).contains(&"--format=c".to_owned()));
        assert!(args(&options(Format::Directory)).contains(&"--format=d".to_owned()));
        assert!(args(&options(Format::Tar)).contains(&"--format=t".to_owned()));
    }

    #[test]
    fn agrega_un_argumento_por_esquema_y_por_tabla() {
        let mut opts = options(Format::Custom);
        opts.schemas = vec!["public".into(), "app".into()];
        opts.exclude_schemas = vec!["temporal".into()];
        opts.tables = vec!["public.clientes".into()];

        let args = args(&opts);
        assert!(args.contains(&"--schema=\"public\"".to_owned()));
        assert!(args.contains(&"--schema=\"app\"".to_owned()));
        assert!(args.contains(&"--exclude-schema=\"temporal\"".to_owned()));
        assert!(args.contains(&"--table=\"public\".\"clientes\"".to_owned()));
    }

    /// Un nombre con comodines tiene que quedar literal: `pg_dump` lo trataría como patrón.
    #[test]
    fn cita_los_patrones() {
        let mut opts = options(Format::Custom);
        opts.tables = vec!["a_b%".into()];
        assert!(args(&opts).contains(&"--table=\"a_b%\"".to_owned()));
    }

    #[test]
    fn la_ruta_vacia_no_se_acepta() {
        let mut opts = options(Format::Custom);
        opts.path = PathBuf::new();
        assert!(arguments(&profile(), &opts).is_err());
    }

    #[test]
    fn esquema_y_datos_se_excluyen() {
        let mut opts = options(Format::Custom);
        opts.schema_only = true;
        opts.data_only = true;
        assert!(arguments(&profile(), &opts).is_err());
    }

    #[test]
    fn el_paralelismo_es_solo_del_formato_directorio() {
        let mut opts = options(Format::Custom);
        opts.jobs = Some(4);
        assert!(arguments(&profile(), &opts).is_err());

        opts.format = Format::Directory;
        assert!(args(&opts).contains(&"--jobs=4".to_owned()));

        // Un solo trabajo es el comportamiento por omisión y no contradice al formato.
        opts.format = Format::Custom;
        opts.jobs = Some(1);
        assert!(arguments(&profile(), &opts).is_ok());
    }

    #[test]
    fn la_compresion_no_va_en_los_formatos_que_no_la_admiten() {
        let mut opts = options(Format::Plain);
        opts.compression = Some(9);
        assert!(arguments(&profile(), &opts).is_err());

        opts.format = Format::Custom;
        assert!(args(&opts).contains(&"--compress=9".to_owned()));

        opts.compression = Some(10);
        assert!(arguments(&profile(), &opts).is_err());
    }

    /// La contraseña viaja por el entorno: si apareciera acá, se filtraría en la vista previa que
    /// la interfaz muestra y deja copiar.
    #[test]
    fn la_contrasena_no_aparece_en_la_linea_de_comando() {
        let (profile, password) =
            ConnectionProfile::from_url("test", "postgres://ana:secreta@servidor:5433/ventas")
                .expect("URL válida");
        assert!(password.is_some(), "la URL traía contraseña");

        let args = arguments(&profile, &options(Format::Custom)).unwrap();
        assert!(!args.iter().any(|arg| arg.contains("secreta")), "{args:?}");
    }

    #[test]
    fn avisa_de_lo_que_conviene_saber_antes() {
        let mut opts = options(Format::Custom);
        assert!(warning(&opts).is_none());

        opts.format = Format::Plain;
        assert!(warning(&opts).is_some());

        opts.format = Format::Custom;
        opts.data_only = true;
        assert!(warning(&opts).is_some());
    }
}
