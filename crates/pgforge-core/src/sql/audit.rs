//! Registro de auditoría: el historial visto con filtros y exportable.
//!
//! No es una tabla aparte: es el mismo `history.db`, que ya anota lo que la aplicación ejecutó
//! (incluido lo que salió de un diálogo). Lo que agrega este módulo es la pregunta —qué se corrió,
//! contra qué servidor, con qué rol, cuándo— y la forma de llevarse la respuesta a un archivo.
//!
//! Dos límites que conviene tener presentes. Es el registro **de pgforge**, no del servidor: no ve
//! lo que otra herramienta corrió contra la misma base, y quien pueda borrar `history.db` puede
//! borrarlo. Y el «quién» es el rol con el que se conectó el perfil, no una persona: la aplicación
//! no tiene cuentas, así que dos personas con el mismo perfil aparecen como el mismo rol.

use std::path::Path;

use rusqlite::types::Value;
use serde::{Deserialize, Serialize};

use super::history::{like_pattern, Entry, HistoryStore, Source, SELECT};
use crate::error::{Error, Result};

/// Qué filas del historial interesan. Todo lo que viene vacío no filtra.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AuditFilter {
    pub profile_id: Option<String>,
    pub database: Option<String>,
    /// Segundos desde el epoch, ambos extremos incluidos.
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub source: Option<Source>,
    /// `dev` / `test` / `prod`, tal como estaba marcado el servidor al ejecutar.
    pub environment: Option<String>,
    pub only_failed: bool,
    /// Texto que tiene que aparecer en el SQL o en el error.
    pub text: Option<String>,
}

/// La cláusula `WHERE` de un filtro y sus parámetros, en el orden de los `?`.
///
/// Función pura a propósito: es lo que decide qué filas entran en la pantalla **y** en el archivo
/// exportado, y las dos tienen que coincidir.
fn where_clause(filter: &AuditFilter) -> (String, Vec<Value>) {
    let mut conditions: Vec<&str> = Vec::new();
    let mut values: Vec<Value> = Vec::new();
    fn text(value: &Option<String>) -> Option<&str> {
        value.as_deref().filter(|v| !v.is_empty())
    }

    if let Some(profile) = text(&filter.profile_id) {
        conditions.push("profile_id = ?");
        values.push(profile.to_owned().into());
    }
    if let Some(database) = text(&filter.database) {
        conditions.push("database = ?");
        values.push(database.to_owned().into());
    }
    if let Some(from) = filter.from {
        conditions.push("started_at >= ?");
        values.push(from.into());
    }
    if let Some(to) = filter.to {
        conditions.push("started_at <= ?");
        values.push(to.into());
    }
    if let Some(source) = filter.source {
        conditions.push("source = ?");
        values.push(source.as_str().to_owned().into());
    }
    if let Some(environment) = text(&filter.environment) {
        conditions.push("environment = ?");
        values.push(environment.to_owned().into());
    }
    if filter.only_failed {
        conditions.push("succeeded = 0");
    }
    if let Some(search) = text(&filter.text) {
        conditions.push("(sql LIKE ? ESCAPE '\\' OR error LIKE ? ESCAPE '\\')");
        let pattern = like_pattern(search);
        values.push(pattern.clone().into());
        values.push(pattern.into());
    }

    if conditions.is_empty() {
        (String::new(), values)
    } else {
        (format!(" WHERE {}", conditions.join(" AND ")), values)
    }
}

/// Un servidor que aparece en el historial, para el selector del filtro.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditServer {
    pub profile_id: String,
    /// El nombre más reciente anotado; `None` si solo hay filas anteriores a que se anotara.
    pub server_name: Option<String>,
}

/// Qué valores hay para elegir en los filtros. Salen del historial y no de los perfiles: un
/// servidor ya borrado sigue teniendo filas, y es justo el que se quiere encontrar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditFacets {
    pub servers: Vec<AuditServer>,
    pub databases: Vec<String>,
}

/// Una página de resultados con el total que cumple el filtro.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditPage {
    pub total: i64,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Csv,
    Json,
}

impl HistoryStore {
    /// Una página del registro filtrado, lo más nuevo primero.
    pub fn audit(&self, filter: &AuditFilter, limit: i64, offset: i64) -> Result<AuditPage> {
        let (clause, mut values) = where_clause(filter);
        let total: i64 = self.connection().query_row(
            &format!("SELECT COUNT(*) FROM history{clause}"),
            rusqlite::params_from_iter(values.iter()),
            |row| row.get(0),
        )?;

        values.push(limit.max(0).into());
        values.push(offset.max(0).into());
        let entries = self.query(
            &format!("{SELECT} {clause} ORDER BY started_at DESC, id DESC LIMIT ? OFFSET ?"),
            rusqlite::params_from_iter(values.iter()),
        )?;
        Ok(AuditPage { total, entries })
    }

    /// Todo lo que cumple el filtro, sin tope: es lo que se exporta.
    fn audit_all(&self, filter: &AuditFilter) -> Result<Vec<Entry>> {
        let (clause, values) = where_clause(filter);
        self.query(
            &format!("{SELECT} {clause} ORDER BY started_at DESC, id DESC"),
            rusqlite::params_from_iter(values.iter()),
        )
    }

    pub fn audit_facets(&self) -> Result<AuditFacets> {
        let connection = self.connection();
        let mut servers = connection.prepare(
            // El nombre sale de la fila más nueva de cada servidor: si lo renombraron, vale el actual.
            "SELECT profile_id,
                    (SELECT server_name FROM history h2
                      WHERE h2.profile_id = h1.profile_id AND h2.server_name IS NOT NULL
                      ORDER BY h2.id DESC LIMIT 1)
               FROM history h1 GROUP BY profile_id ORDER BY 2, 1",
        )?;
        let servers = servers
            .query_map([], |row| {
                Ok(AuditServer {
                    profile_id: row.get(0)?,
                    server_name: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut databases =
            connection.prepare("SELECT DISTINCT database FROM history ORDER BY database")?;
        let databases = databases
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;

        Ok(AuditFacets { servers, databases })
    }

    /// Escribe en `path` todo lo que cumple el filtro y devuelve cuántas filas fueron.
    ///
    /// La ruta ya viene autorizada por quien llama (la eligió el usuario en un diálogo de guardar);
    /// acá no se vuelve a decidir. Se escribe a un archivo vecino y se renombra, para que un disco
    /// lleno a mitad de camino no deje un CSV truncado que parezca completo.
    pub fn audit_export(
        &self,
        filter: &AuditFilter,
        format: ExportFormat,
        path: &Path,
    ) -> Result<usize> {
        let entries = self.audit_all(filter)?;
        let text = render(&entries, format)?;

        let mut partial = path.as_os_str().to_owned();
        partial.push(".parcial");
        let partial = std::path::PathBuf::from(partial);
        std::fs::write(&partial, text.as_bytes())?;
        if let Err(error) = std::fs::rename(&partial, path) {
            let _ = std::fs::remove_file(&partial);
            return Err(Error::Io(error));
        }
        Ok(entries.len())
    }
}

/// Instante en UTC con formato ISO 8601 (`2026-03-04T09:05:00Z`), a partir de segundos del epoch.
///
/// A mano y no con una caja de fechas: es el único lugar del núcleo que formatea una fecha, y el
/// algoritmo de días civiles son diez líneas que se prueban con fechas conocidas.
pub fn iso_utc(epoch: i64) -> String {
    let days = epoch.div_euclid(86_400);
    let secs = epoch.rem_euclid(86_400);
    // Howard Hinnant, «civil_from_days».
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs / 3_600,
        secs % 3_600 / 60,
        secs % 60
    )
}

const CSV_HEADER: [&str; 14] = [
    "id",
    "inicio",
    "inicio_epoch",
    "servidor",
    "servidor_id",
    "entorno",
    "base",
    "usuario",
    "origen",
    "resultado",
    "segundos",
    "filas",
    "sql",
    "error",
];

fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// El texto del archivo para estas filas.
///
/// CSV con BOM UTF-8 y fin de línea `\r\n`: es lo que hace que Excel lea bien las tildes y los
/// saltos de línea dentro de un SQL. No se antepone `'` a las celdas que empiezan con `=`, `+` o
/// `-` (la defensa habitual contra fórmulas): un SQL empieza con `--` a cada rato, y un registro
/// de auditoría que altera lo que registra deja de ser fiel. Para eso está el JSON.
pub fn render(entries: &[Entry], format: ExportFormat) -> Result<String> {
    match format {
        ExportFormat::Csv => {
            let mut out = String::from("\u{feff}");
            out.push_str(&CSV_HEADER.join(","));
            out.push_str("\r\n");
            for entry in entries {
                let cells = [
                    entry.id.to_string(),
                    iso_utc(entry.started_at),
                    entry.started_at.to_string(),
                    entry.actor.server_name.clone().unwrap_or_default(),
                    entry.profile_id.clone(),
                    entry.actor.environment.clone().unwrap_or_default(),
                    entry.database.clone(),
                    entry.actor.user.clone().unwrap_or_default(),
                    entry.source.as_str().to_owned(),
                    if entry.succeeded { "ok" } else { "error" }.to_owned(),
                    format!("{:.3}", entry.seconds),
                    entry.row_count.map(|n| n.to_string()).unwrap_or_default(),
                    entry.sql.clone(),
                    entry.error.clone().unwrap_or_default(),
                ];
                let row: Vec<String> = cells.iter().map(|cell| csv_cell(cell)).collect();
                out.push_str(&row.join(","));
                out.push_str("\r\n");
            }
            Ok(out)
        }
        ExportFormat::Json => {
            let rows: Vec<serde_json::Value> = entries
                .iter()
                .map(|entry| {
                    serde_json::json!({
                        "id": entry.id,
                        "startedAt": iso_utc(entry.started_at),
                        "startedAtEpoch": entry.started_at,
                        "server": entry.actor.server_name,
                        "serverId": entry.profile_id,
                        "environment": entry.actor.environment,
                        "database": entry.database,
                        "user": entry.actor.user,
                        "source": entry.source.as_str(),
                        "succeeded": entry.succeeded,
                        "seconds": entry.seconds,
                        "rows": entry.row_count,
                        "sql": entry.sql,
                        "error": entry.error,
                    })
                })
                .collect();
            serde_json::to_string_pretty(&rows)
                .map(|mut text| {
                    text.push('\n');
                    text
                })
                .map_err(|error| Error::Config(error.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::history::{Actor, NewEntry};
    use super::*;

    fn entry(sql: &str, at: i64) -> NewEntry {
        NewEntry {
            source: Source::Editor,
            actor: Actor {
                server_name: Some("Producción".into()),
                user: Some("ana".into()),
                environment: Some("prod".into()),
            },
            profile_id: "s1".into(),
            database: "app".into(),
            sql: sql.into(),
            started_at: at,
            seconds: 0.25,
            row_count: Some(2),
            error: None,
        }
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pgforge-audit-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Un historial con filas de dos servidores, dos bases, dos orígenes y una falla.
    fn populated(path: &Path) -> HistoryStore {
        let store = HistoryStore::open(path).unwrap();
        store.record(&entry("SELECT 1", 1_000)).unwrap();
        store
            .record(&NewEntry {
                source: Source::Dialog,
                ..entry("ALTER TABLE t ADD c int", 2_000)
            })
            .unwrap();
        store
            .record(&NewEntry {
                profile_id: "s2".into(),
                database: "otra".into(),
                actor: Actor {
                    server_name: Some("Desarrollo".into()),
                    user: Some("dev".into()),
                    environment: Some("dev".into()),
                },
                ..entry("SELECT 100 % 3", 3_000)
            })
            .unwrap();
        store
            .record(&NewEntry {
                error: Some("no existe la tabla \"x\"".into()),
                row_count: None,
                ..entry("SELECT * FROM x", 4_000)
            })
            .unwrap();
        store
    }

    #[test]
    fn filtra_por_cada_criterio_y_los_combina() {
        let dir = temp_dir("filtros");
        let store = populated(&dir.join("h.db"));
        let count = |filter: AuditFilter| store.audit(&filter, 100, 0).unwrap().total;

        assert_eq!(count(AuditFilter::default()), 4);
        assert_eq!(
            count(AuditFilter {
                profile_id: Some("s2".into()),
                ..Default::default()
            }),
            1
        );
        assert_eq!(
            count(AuditFilter {
                database: Some("app".into()),
                ..Default::default()
            }),
            3
        );
        assert_eq!(
            count(AuditFilter {
                source: Some(Source::Dialog),
                ..Default::default()
            }),
            1
        );
        assert_eq!(
            count(AuditFilter {
                environment: Some("dev".into()),
                ..Default::default()
            }),
            1
        );
        assert_eq!(
            count(AuditFilter {
                only_failed: true,
                ..Default::default()
            }),
            1
        );
        // Los dos extremos del rango entran.
        assert_eq!(
            count(AuditFilter {
                from: Some(2_000),
                to: Some(3_000),
                ..Default::default()
            }),
            2
        );
        assert_eq!(
            count(AuditFilter {
                profile_id: Some("s1".into()),
                from: Some(1_500),
                source: Some(Source::Editor),
                ..Default::default()
            }),
            1
        );
        // Una cadena vacía no filtra.
        assert_eq!(
            count(AuditFilter {
                database: Some(String::new()),
                text: Some(String::new()),
                ..Default::default()
            }),
            4
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn el_texto_busca_en_el_sql_y_en_el_error_y_los_comodines_son_texto() {
        let dir = temp_dir("texto");
        let store = populated(&dir.join("h.db"));
        let count = |text: &str| {
            store
                .audit(
                    &AuditFilter {
                        text: Some(text.into()),
                        ..Default::default()
                    },
                    100,
                    0,
                )
                .unwrap()
                .total
        };
        assert_eq!(count("ALTER"), 1);
        assert_eq!(count("no existe"), 1);
        assert_eq!(count("100 %"), 1);
        assert_eq!(count("%"), 1);
        assert_eq!(count("nada de esto"), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pagina_con_el_total_completo_y_lo_mas_nuevo_primero() {
        let dir = temp_dir("paginas");
        let store = populated(&dir.join("h.db"));
        let page = store.audit(&AuditFilter::default(), 2, 1).unwrap();
        assert_eq!(page.total, 4);
        let at: Vec<i64> = page.entries.iter().map(|e| e.started_at).collect();
        assert_eq!(at, [3_000, 2_000]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn las_facetas_salen_del_historial() {
        let dir = temp_dir("facetas");
        let store = populated(&dir.join("h.db"));
        let facets = store.audit_facets().unwrap();
        assert_eq!(facets.databases, ["app", "otra"]);
        let names: Vec<_> = facets
            .servers
            .iter()
            .map(|s| (s.profile_id.as_str(), s.server_name.as_deref()))
            .collect();
        assert_eq!(
            names,
            [("s2", Some("Desarrollo")), ("s1", Some("Producción"))]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn un_archivo_de_la_version_anterior_gana_las_columnas_sin_perder_filas() {
        let dir = temp_dir("migracion");
        let path = dir.join("h.db");
        {
            let viejo = rusqlite::Connection::open(&path).unwrap();
            viejo
                .execute_batch(
                    "CREATE TABLE history (
                         id INTEGER PRIMARY KEY, profile_id TEXT NOT NULL, database TEXT NOT NULL,
                         sql TEXT NOT NULL, started_at INTEGER NOT NULL, seconds REAL,
                         row_count INTEGER, succeeded INTEGER NOT NULL, error TEXT,
                         source TEXT NOT NULL DEFAULT 'editor'
                     );
                     INSERT INTO history (profile_id, database, sql, started_at, seconds, succeeded)
                     VALUES ('s1', 'app', 'SELECT 1', 1000, 0.1, 1);",
                )
                .unwrap();
            viejo.pragma_update(None, "user_version", 2).unwrap();
        }
        let store = HistoryStore::open(&path).unwrap();
        let page = store.audit(&AuditFilter::default(), 10, 0).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.entries[0].actor, Actor::default());
        store.record(&entry("SELECT 2", 2_000)).unwrap();
        let page = store.audit(&AuditFilter::default(), 10, 0).unwrap();
        assert_eq!(page.entries[0].actor.user.as_deref(), Some("ana"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn el_filtro_se_lee_tal_como_lo_manda_la_interfaz() {
        // `audit-filter.ts` manda `null` en lo que no filtra y camelCase en los nombres.
        let filter: AuditFilter = serde_json::from_str(
            r#"{"profileId":"s1","database":null,"from":10,"to":null,"source":"dialog",
                "environment":"prod","onlyFailed":true,"text":"alter"}"#,
        )
        .unwrap();
        assert_eq!(filter.profile_id.as_deref(), Some("s1"));
        assert_eq!(filter.source, Some(Source::Dialog));
        assert!(filter.only_failed && filter.to.is_none());
        let vacio: AuditFilter = serde_json::from_str("{}").unwrap();
        assert!(!vacio.only_failed);
    }

    #[test]
    fn la_entrada_viaja_con_el_actor_aplanado() {
        let dir = temp_dir("json");
        let store = populated(&dir.join("h.db"));
        let page = store.audit(&AuditFilter::default(), 1, 0).unwrap();
        let json = serde_json::to_value(&page.entries[0]).unwrap();
        assert_eq!(json["serverName"], "Producción");
        assert_eq!(json["dbUser"], "ana");
        assert_eq!(json["environment"], "prod");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn las_fechas_iso_son_las_de_siempre() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso_utc(1_709_556_300), "2024-03-04T12:45:00Z");
        assert_eq!(iso_utc(-1), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn el_csv_cita_comas_comillas_y_saltos_de_linea() {
        let dir = temp_dir("csv");
        let store = populated(&dir.join("h.db"));
        store
            .record(&entry("SELECT 'a,b', \"c\"\nFROM t", 5_000))
            .unwrap();
        let entries = store
            .audit(
                &AuditFilter {
                    text: Some("a,b".into()),
                    ..Default::default()
                },
                10,
                0,
            )
            .unwrap()
            .entries;
        let csv = render(&entries, ExportFormat::Csv).unwrap();
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("\"SELECT 'a,b', \"\"c\"\"\nFROM t\""));
        assert!(csv.contains("Producción,s1,prod,app,ana,editor,ok,0.250,2,"));
        assert!(csv.contains(",1970-01-01T01:23:20Z,5000,"));
        assert_eq!(csv.lines().next().unwrap().matches(',').count(), 13);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exporta_lo_filtrado_a_json_y_csv_en_un_archivo() {
        let dir = temp_dir("exportar");
        let store = populated(&dir.join("h.db"));
        let filter = AuditFilter {
            only_failed: true,
            ..Default::default()
        };

        let json_path = dir.join("auditoria.json");
        let n = store
            .audit_export(&filter, ExportFormat::Json, &json_path)
            .unwrap();
        assert_eq!(n, 1);
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap()).unwrap();
        assert_eq!(value.as_array().unwrap().len(), 1);
        assert_eq!(value[0]["server"], "Producción");
        assert_eq!(value[0]["user"], "ana");
        assert_eq!(value[0]["succeeded"], false);
        assert_eq!(value[0]["error"], "no existe la tabla \"x\"");
        assert_eq!(value[0]["startedAt"], "1970-01-01T01:06:40Z");

        let csv_path = dir.join("auditoria.csv");
        let n = store
            .audit_export(&AuditFilter::default(), ExportFormat::Csv, &csv_path)
            .unwrap();
        assert_eq!(n, 4);
        assert!(!dir.join("auditoria.csv.parcial").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
