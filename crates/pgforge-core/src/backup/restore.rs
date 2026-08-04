//! Restore con `pg_restore`.
//!
//! El reverso del backup, con el mismo molde de dos pasos: [`arguments`] arma la línea de comando y
//! es pura —lo único verificable sin servidor—, [`plan`] le agrega el binario ubicado y la
//! comparación de versiones, y [`run`] la ejecuta transmitiendo el progreso. El andamiaje del
//! proceso hijo lo comparte con el backup ([`super::spawn_streaming`]): el trabajo es el mismo, un
//! binario externo que solo da señales de vida por stderr cuando se le pasa `--verbose`.
//!
//! No restaura el formato plano: eso es un script SQL que se pasa por `psql`, no por `pg_restore`.
//! La diferencia con el backup al terminar es que acá no hay archivo que borrar si algo sale mal
//! —lo escrito va a la base—, y contra eso la defensa es `--single-transaction`, no un `cleanup`.

use std::path::PathBuf;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use super::tools::{self, Tool};
use super::{quote_pattern, Ended, Format};
use crate::conn::profile::ConnectionProfile;
use crate::conn::ServerHandle;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreOptions {
    /// El archivo del backup, o el directorio en el formato correspondiente.
    pub source: PathBuf,
    /// Con qué formato se hizo el backup. `pg_restore` lo detecta solo salvo en el formato
    /// directorio, pero se pide igual para validar antes —el plano no se restaura con esta
    /// herramienta— y para rotular la operación.
    pub format: Format,
    /// Base a la que conectarse. Con `create` es la base de mantenimiento desde la que se crea la
    /// nueva; sin él, la base destino donde se cargan los objetos.
    pub database: String,
    /// Vacío quiere decir todo lo que haya en el backup.
    #[serde(default)]
    pub schemas: Vec<String>,
    #[serde(default)]
    pub tables: Vec<String>,
    #[serde(default)]
    pub schema_only: bool,
    #[serde(default)]
    pub data_only: bool,
    /// `--clean`: elimina cada objeto antes de recrearlo.
    #[serde(default)]
    pub clean: bool,
    /// `--if-exists`: que el borrado de `clean` no falle si el objeto todavía no existe. Solo tiene
    /// sentido junto con `clean`.
    #[serde(default)]
    pub if_exists: bool,
    /// `--create`: crea la base destino —y se conecta a ella— en vez de cargar sobre una existente.
    #[serde(default)]
    pub create: bool,
    #[serde(default)]
    pub no_owner: bool,
    #[serde(default)]
    pub no_privileges: bool,
    /// `--single-transaction`: todo o nada. Un error a mitad de camino no deja la base a medias.
    #[serde(default)]
    pub single_transaction: bool,
    /// Trabajos en paralelo. Solo los formatos custom y directorio, y nunca junto a
    /// `single_transaction`.
    pub jobs: Option<u8>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePlan {
    /// El binario y sus argumentos, listos para mostrar. La contraseña no está acá: viaja por
    /// `PGPASSWORD`.
    pub command: Vec<String>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreOutcome {
    /// La base sobre la que se restauró. No hay un tamaño que informar como en el backup: lo
    /// escrito quedó en el servidor, no en un archivo.
    pub database: String,
    pub seconds: f64,
    /// Cuántos errores ignoró `pg_restore` por el camino. Sin `--single-transaction` los va
    /// salteando y termina con un código de error aunque el grueso se haya cargado; el caso más
    /// común es restaurar un dump de una versión más nueva que el servidor, que trae un `SET` de un
    /// parámetro que el servidor viejo no conoce. Cero cuando salió todo limpio.
    pub ignored_errors: u64,
}

/// Los argumentos de `pg_restore`, sin el binario.
///
/// Pura a propósito, igual que [`super::arguments`]: valida acá lo que `pg_restore` rechazaría recién
/// al arrancar y deja que la interfaz muestre la línea exacta antes de tocar la base.
pub fn arguments(profile: &ConnectionProfile, options: &RestoreOptions) -> Result<Vec<String>> {
    if options.source.as_os_str().is_empty() {
        return Err(Error::Config(
            "hace falta indicar el archivo del backup a restaurar".to_owned(),
        ));
    }
    if options.format == Format::Plain {
        return Err(Error::Config(
            "el formato plano es un script SQL: se restaura con psql, no con pg_restore".to_owned(),
        ));
    }
    if options.schema_only && options.data_only {
        return Err(Error::Config(
            "«solo el esquema» y «solo los datos» se excluyen entre sí".to_owned(),
        ));
    }
    if options.if_exists && !options.clean {
        return Err(Error::Config(
            "«si existe» solo tiene sentido junto con «limpiar»".to_owned(),
        ));
    }
    if let Some(jobs) = options.jobs {
        if jobs == 0 {
            return Err(Error::Config(
                "la cantidad de trabajos en paralelo tiene que ser al menos 1".to_owned(),
            ));
        }
        if jobs > 1 {
            if options.format == Format::Tar {
                return Err(Error::Config(
                    "el formato tar no admite trabajos en paralelo".to_owned(),
                ));
            }
            // `pg_restore` los rechaza junto: no puede repartir en varios procesos lo que tiene que
            // caber en una sola transacción.
            if options.single_transaction {
                return Err(Error::Config(
                    "«una sola transacción» y los trabajos en paralelo se excluyen entre sí"
                        .to_owned(),
                ));
            }
        }
    }

    let mut args = vec![
        format!("--host={}", profile.host),
        format!("--port={}", profile.port),
        format!("--username={}", profile.user),
        format!("--dbname={}", options.database),
        format!("--format={}", options.format.flag()),
        // Sin esto no hay una sola línea de progreso.
        "--verbose".to_owned(),
        // Si no, `pg_restore` se queda esperando una contraseña por consola que nadie va a escribir.
        "--no-password".to_owned(),
    ];

    if options.create {
        args.push("--create".to_owned());
    }
    if options.clean {
        args.push("--clean".to_owned());
    }
    if options.if_exists {
        args.push("--if-exists".to_owned());
    }

    for schema in &options.schemas {
        args.push(format!("--schema={}", quote_pattern(schema)));
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
    if options.single_transaction {
        args.push("--single-transaction".to_owned());
    }
    if let Some(jobs) = options.jobs {
        args.push(format!("--jobs={jobs}"));
    }

    // El archivo va como argumento posicional, al final —así lo espera `pg_restore`, al revés que
    // `pg_dump` que lo toma con `--file`—. En el formato directorio es el directorio.
    args.push(options.source.display().to_string());

    Ok(args)
}

/// Lo que conviene saber antes de restaurar, o `None` si no hay nada que decir.
pub fn warning(options: &RestoreOptions) -> Option<&'static str> {
    if options.clean {
        return Some(
            "Con «limpiar» se elimina cada objeto de la base destino antes de recrearlo: lo que \
             haya ahí y coincida con el backup se pierde.",
        );
    }
    if options.data_only {
        return Some(
            "Un restore solo de datos da por hecho que las tablas ya existen; con claves foráneas \
             de por medio, el orden de carga puede hacerlo fallar.",
        );
    }
    if !options.single_transaction {
        return Some(
            "Sin «una sola transacción», un error a mitad de camino deja la base a medio restaurar \
             en vez de dejarla como estaba.",
        );
    }
    None
}

/// La línea de comando completa y su advertencia, con el binario ya ubicado.
///
/// Como en el backup, es acá donde se compara la versión del binario con la del servidor: es la
/// falla más común —`pg_restore` puede escribir en servidores más viejos que él, nunca más nuevos—
/// y conviene detectarla antes de empezar a tocar la base, no a mitad de la restauración.
pub async fn plan(handle: &ServerHandle, options: &RestoreOptions) -> Result<RestorePlan> {
    let binary = tools::require(Tool::PgRestore)?;
    let version = tools::version(&binary).await?;
    let server = handle.caps.version;

    if version.major() < server.major() {
        return Err(Error::Config(format!(
            "el pg_restore encontrado es {version} y el servidor es {server}: pg_restore puede \
             escribir en servidores más viejos que él, pero no más nuevos. Instalá las herramientas \
             cliente de PostgreSQL {} o indicá la ruta en PGFORGE_PG_RESTORE.",
            server.major()
        )));
    }

    let mut command = vec![binary.display().to_string()];
    command.extend(arguments(&handle.profile, options)?);

    Ok(RestorePlan {
        command,
        warning: warning(options).map(str::to_owned),
    })
}

/// Ejecuta el restore, transmitiendo el progreso por `progress` y abortando si llega algo por
/// `cancel`.
///
/// Sin límite de tiempo: un restore tarda lo que tarda, igual que el backup.
pub async fn run(
    handle: &ServerHandle,
    options: &RestoreOptions,
    progress: mpsc::Sender<String>,
    cancel: oneshot::Receiver<()>,
) -> Result<RestoreOutcome> {
    let plan = plan(handle, options).await?;
    let (binary, args) = plan.command.split_first().expect("el plan trae el binario");

    let mut command = super::base_command(binary, handle);
    command.args(args);

    let started = Instant::now();
    match super::spawn_streaming(command, binary, progress, cancel).await? {
        // A diferencia del backup no hay archivo que borrar: lo que se cargó quedó en la base. Que
        // no quede a medias es tarea de `--single-transaction`, no de esta función.
        Ended::Canceled => Err(Error::Canceled),
        Ended::Done(status, tail) => {
            if !status.success() {
                // `pg_restore` termina con código de error también cuando llegó hasta el final pero
                // ignoró errores por el camino: sin `--single-transaction` no aborta, los saltea y
                // los cuenta. Eso no es un fallo del restore —el grueso se cargó—, así que se reporta
                // el conteo en vez de tirar todo. Solo es un fallo de verdad cuando no dejó siquiera
                // ese resumen (no pudo conectarse, el archivo no era un backup, etc.).
                if let Some(ignored) = ignored_errors(tail.lines()) {
                    return Ok(RestoreOutcome {
                        database: options.database.clone(),
                        seconds: started.elapsed().as_secs_f64(),
                        ignored_errors: ignored,
                    });
                }
                return Err(Error::Config(format!(
                    "pg_restore terminó con error: {}",
                    tail.last().unwrap_or("sin detalle")
                )));
            }
            Ok(RestoreOutcome {
                database: options.database.clone(),
                seconds: started.elapsed().as_secs_f64(),
                ignored_errors: 0,
            })
        }
    }
}

/// Busca el resumen `errors ignored on restore: N` que `pg_restore` deja como última línea cuando
/// terminó salteando errores, y devuelve N. `None` si no está —es decir, si el proceso falló sin
/// llegar a ese punto—.
fn ignored_errors(lines: &[String]) -> Option<u64> {
    const MARKER: &str = "errors ignored on restore:";
    lines.iter().rev().find_map(|line| {
        let rest = line.split(MARKER).nth(1)?;
        rest.split_whitespace().next()?.parse().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> ConnectionProfile {
        ConnectionProfile::from_url("test", "postgres://ana@servidor:5433/ventas")
            .expect("URL válida")
            .0
    }

    fn options(format: Format) -> RestoreOptions {
        RestoreOptions {
            source: PathBuf::from("/tmp/ventas.dump"),
            format,
            database: "ventas".into(),
            schemas: vec![],
            tables: vec![],
            schema_only: false,
            data_only: false,
            clean: false,
            if_exists: false,
            create: false,
            no_owner: false,
            no_privileges: false,
            single_transaction: false,
            jobs: None,
        }
    }

    fn args(options: &RestoreOptions) -> Vec<String> {
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

    /// El archivo es un argumento posicional y va al final, no con `--file` como en el backup.
    #[test]
    fn el_archivo_va_al_final_como_posicional() {
        let args = args(&options(Format::Custom));
        assert_eq!(args.last().unwrap(), "/tmp/ventas.dump");
        assert!(!args.iter().any(|arg| arg.starts_with("--file")));
    }

    /// El formato plano es un script de `psql`: `pg_restore` no lo lee.
    #[test]
    fn el_formato_plano_no_se_restaura() {
        assert!(arguments(&profile(), &options(Format::Plain)).is_err());
    }

    #[test]
    fn la_ruta_vacia_no_se_acepta() {
        let mut opts = options(Format::Custom);
        opts.source = PathBuf::new();
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
    fn si_existe_necesita_limpiar() {
        let mut opts = options(Format::Custom);
        opts.if_exists = true;
        assert!(arguments(&profile(), &opts).is_err());

        opts.clean = true;
        let args = args(&opts);
        assert!(args.contains(&"--clean".to_owned()));
        assert!(args.contains(&"--if-exists".to_owned()));
    }

    #[test]
    fn el_paralelismo_no_va_con_una_sola_transaccion() {
        let mut opts = options(Format::Custom);
        opts.jobs = Some(4);
        opts.single_transaction = true;
        assert!(arguments(&profile(), &opts).is_err());

        // Cada uno por su lado sí vale.
        opts.single_transaction = false;
        assert!(args(&opts).contains(&"--jobs=4".to_owned()));

        opts.jobs = None;
        opts.single_transaction = true;
        assert!(args(&opts).contains(&"--single-transaction".to_owned()));
    }

    #[test]
    fn el_paralelismo_no_va_en_el_formato_tar() {
        let mut opts = options(Format::Tar);
        opts.jobs = Some(2);
        assert!(arguments(&profile(), &opts).is_err());

        // Un solo trabajo es el comportamiento por omisión y no contradice al formato.
        opts.jobs = Some(1);
        assert!(arguments(&profile(), &opts).is_ok());
    }

    #[test]
    fn un_argumento_por_esquema_y_por_tabla() {
        let mut opts = options(Format::Custom);
        opts.schemas = vec!["public".into()];
        opts.tables = vec!["public.clientes".into()];

        let args = args(&opts);
        assert!(args.contains(&"--schema=\"public\"".to_owned()));
        assert!(args.contains(&"--table=\"public\".\"clientes\"".to_owned()));
    }

    /// La contraseña viaja por el entorno: si apareciera acá, se filtraría en la vista previa.
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
    fn reconoce_el_resumen_de_errores_ignorados() {
        let lines = vec![
            "pg_restore: creating TABLE \"diag.clientes\"".to_owned(),
            "pg_restore: warning: errors ignored on restore: 3".to_owned(),
        ];
        assert_eq!(ignored_errors(&lines), Some(3));

        // Un fallo que ni llegó a ese resumen no cuenta como «restauró con errores ignorados».
        let fallo = vec!["pg_restore: error: connection to server failed".to_owned()];
        assert_eq!(ignored_errors(&fallo), None);
    }

    #[test]
    fn avisa_de_lo_que_conviene_saber_antes() {
        // Con «una sola transacción» y sin nada riesgoso, no hay nada que decir.
        let mut opts = options(Format::Custom);
        opts.single_transaction = true;
        assert!(warning(&opts).is_none());

        opts.clean = true;
        assert!(warning(&opts).is_some());

        // Sin «una sola transacción», avisa del estado a medias.
        let opts = options(Format::Custom);
        assert!(warning(&opts).is_some());
    }
}
