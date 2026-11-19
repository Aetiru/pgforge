//! Revisión de la configuración del servidor.
//!
//! No aplica nada: cada hallazgo trae el `ALTER SYSTEM` para copiar, igual que la sugerencia de
//! índices. Cambiar un parámetro es una decisión de quien conoce la máquina, y la aplicación solo
//! puede conocer lo que el servidor le cuenta —no ve la RAM, por eso se pide aparte—.
//!
//! Pocas reglas y con evidencia donde se puede: `work_mem` solo se menciona si la base ya escribió
//! gigabytes de archivos temporales, no porque el valor de fábrica sea chico.

use std::collections::HashMap;

use serde::Serialize;
use tokio_postgres::Client;

use super::diagnose::{human_bytes, pg_size, setting_bytes, settings, Level, Setting};
use crate::error::Result;

/// Lo que se sabe de afuera del catálogo.
#[derive(Debug, Clone, Default)]
pub struct ReviewContext {
    /// RAM de la máquina, si el usuario la indicó. Sin ella no se sugieren tamaños de memoria.
    pub ram_bytes: Option<i64>,
    /// Bytes temporales que escribió la base desde el reinicio de sus estadísticas.
    pub temp_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub name: String,
    pub current: String,
    pub suggested: Option<String>,
    pub reason: String,
    pub level: Level,
    /// La sentencia para copiar. `None` si el cambio no se resuelve con un valor.
    pub alter_sql: Option<String>,
    /// Hay que reiniciar el servidor para que valga; si no, alcanza con `pg_reload_conf()`.
    pub restart: bool,
}

const NAMES: &[&str] = &[
    "shared_buffers",
    "effective_cache_size",
    "maintenance_work_mem",
    "work_mem",
    "checkpoint_completion_target",
    "random_page_cost",
    "autovacuum",
    "fsync",
    "full_page_writes",
    "max_connections",
    "idle_in_transaction_session_timeout",
    "log_min_duration_statement",
    "shared_preload_libraries",
];

fn is_default(setting: &Setting) -> bool {
    setting.source == "default"
}

fn alter(name: &str, value: &str) -> String {
    format!("ALTER SYSTEM SET {name} = '{}'", value.replace('\'', "''"))
}

fn number(settings: &HashMap<String, Setting>, name: &str) -> Option<f64> {
    settings.get(name)?.value.as_deref()?.parse().ok()
}

/// Las reglas, sobre los parámetros ya leídos. Ordenado de lo más grave a lo más suave.
pub fn review(settings: &HashMap<String, Setting>, context: &ReviewContext) -> Vec<Finding> {
    let mut out = Vec::new();
    let restart = |name: &str| {
        settings
            .get(name)
            .is_some_and(|s| s.context == "postmaster")
    };

    let mut push = |name: &str,
                    current: String,
                    suggested: Option<String>,
                    reason: String,
                    level: Level,
                    restart: bool| {
        let alter_sql = suggested.as_deref().map(|value| alter(name, value));
        out.push(Finding {
            name: name.to_owned(),
            current,
            suggested,
            reason,
            level,
            alter_sql,
            restart,
        });
    };

    // Los dos que pueden dañar datos o dejar de limpiar: no son una cuestión de ajuste fino.
    for name in ["fsync", "full_page_writes"] {
        if settings.get(name).and_then(|s| s.value.as_deref()) == Some("off") {
            push(
                name,
                "off".into(),
                Some("on".into()),
                format!(
                    "{name} apagado: un corte de luz o un fallo del sistema puede dejar la base \
                     corrupta, sin forma de recuperarla."
                ),
                Level::Attention,
                restart(name),
            );
        }
    }
    if settings.get("autovacuum").and_then(|s| s.value.as_deref()) == Some("off") {
        push(
            "autovacuum",
            "off".into(),
            Some("on".into()),
            "Sin autovacuum las tablas acumulan filas muertas y la edad de las transacciones \
             sigue subiendo hasta que el servidor deja de aceptar escrituras."
                .into(),
            Level::Attention,
            restart("autovacuum"),
        );
    }

    if let (Some(ram), Some(current)) = (
        context.ram_bytes,
        settings.get("shared_buffers").and_then(setting_bytes),
    ) {
        let target = ram / 4;
        if current < target / 2 {
            push(
                "shared_buffers",
                human_bytes(current),
                Some(pg_size(target / (1 << 20) * (1 << 20))),
                format!(
                    "Con {} de RAM, la regla de partida es un cuarto para la caché propia de \
                     PostgreSQL.",
                    human_bytes(ram)
                ),
                Level::Review,
                true,
            );
        }
    } else if let Some(setting) = settings.get("shared_buffers") {
        if is_default(setting) {
            push(
                "shared_buffers",
                setting_bytes(setting).map(human_bytes).unwrap_or_default(),
                None,
                "Está en el valor de fábrica, pensado para que arranque en cualquier máquina. \
                 Indica la RAM del servidor para que se sugiera un valor."
                    .into(),
                Level::Info,
                true,
            );
        }
    }

    if let (Some(ram), Some(current)) = (
        context.ram_bytes,
        settings.get("effective_cache_size").and_then(setting_bytes),
    ) {
        let target = ram / 4 * 3;
        if current < target / 2 {
            push(
                "effective_cache_size",
                human_bytes(current),
                Some(pg_size(target / (1 << 20) * (1 << 20))),
                "Es lo que el planificador cree que cabe en caché entre la de PostgreSQL y la del \
                 sistema. Muy bajo, descarta índices que convendría usar."
                    .into(),
                Level::Review,
                restart("effective_cache_size"),
            );
        }
    }

    if let (Some(ram), Some(current)) = (
        context.ram_bytes,
        settings.get("maintenance_work_mem").and_then(setting_bytes),
    ) {
        let target = (ram / 16).min(2 << 30);
        if ram >= 4 << 30 && current < target / 2 {
            push(
                "maintenance_work_mem",
                human_bytes(current),
                Some(pg_size(target / (1 << 20) * (1 << 20))),
                "Lo usan VACUUM y CREATE INDEX: con más memoria terminan antes.".into(),
                Level::Review,
                restart("maintenance_work_mem"),
            );
        }
    }

    if let (Some(temp), Some(current)) = (
        context.temp_bytes,
        settings.get("work_mem").and_then(setting_bytes),
    ) {
        if temp >= 1 << 30 && current <= 4 << 20 {
            push(
                "work_mem",
                human_bytes(current),
                Some("16MB".into()),
                format!(
                    "La base escribió {} de archivos temporales: ordenamientos y uniones que no \
                     entraron en memoria. Probalo primero con SET work_mem en la sesión que lo \
                     necesite; cada operación de cada conexión puede usar esa cantidad.",
                    human_bytes(temp)
                ),
                Level::Review,
                restart("work_mem"),
            );
        }
    }

    if number(settings, "checkpoint_completion_target").is_some_and(|v| v < 0.9) {
        push(
            "checkpoint_completion_target",
            number(settings, "checkpoint_completion_target")
                .map(|v| v.to_string().replace('.', ","))
                .unwrap_or_default(),
            Some("0.9".into()),
            "Reparte la escritura de cada checkpoint en el 90 % del intervalo en vez de \
             concentrarla."
                .into(),
            Level::Review,
            restart("checkpoint_completion_target"),
        );
    }

    if number(settings, "max_connections").is_some_and(|v| v >= 500.0) {
        push(
            "max_connections",
            number(settings, "max_connections")
                .map(|v| (v as i64).to_string())
                .unwrap_or_default(),
            None,
            "Cada conexión reserva memoria y compite por los mismos recursos. Con tantas, \
             conviene un pooler (PgBouncer) en vez de subir el límite."
                .into(),
            Level::Review,
            true,
        );
    }

    if let Some(setting) = settings.get("random_page_cost") {
        if is_default(setting) && number(settings, "random_page_cost") == Some(4.0) {
            push(
                "random_page_cost",
                "4".into(),
                Some("1.1".into()),
                "El 4 de fábrica supone discos giratorios. Si el almacenamiento es SSD, 1,1 hace \
                 que el planificador use índices que hoy descarta."
                    .into(),
                Level::Info,
                restart("random_page_cost"),
            );
        }
    }

    if number(settings, "idle_in_transaction_session_timeout") == Some(0.0) {
        push(
            "idle_in_transaction_session_timeout",
            "sin límite".into(),
            Some("10min".into()),
            "Una sesión que abrió una transacción y se olvidó retiene filas e impide que el \
             vacuum limpie. Elige un valor que tus transacciones legítimas no superen."
                .into(),
            Level::Info,
            restart("idle_in_transaction_session_timeout"),
        );
    }

    if number(settings, "log_min_duration_statement") == Some(-1.0) {
        push(
            "log_min_duration_statement",
            "apagado".into(),
            Some("1s".into()),
            "Deja en el registro del servidor lo que tarda más de un segundo, para encontrar \
             después las consultas lentas que nadie vio pasar."
                .into(),
            Level::Info,
            restart("log_min_duration_statement"),
        );
    }

    if let Some(libraries) = settings
        .get("shared_preload_libraries")
        .and_then(|s| s.value.as_deref())
    {
        if !libraries.contains("pg_stat_statements") {
            push(
                "shared_preload_libraries",
                if libraries.is_empty() {
                    "vacío".into()
                } else {
                    libraries.to_owned()
                },
                None,
                "Sin pg_stat_statements no hay «Consultas lentas». Agrégalo a la lista, reinicia \
                 el servidor y ejecuta CREATE EXTENSION pg_stat_statements."
                    .into(),
                Level::Info,
                true,
            );
        }
    }

    out.sort_by_key(|f| std::cmp::Reverse(f.level));
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigReview {
    pub findings: Vec<Finding>,
    /// Los parámetros que no se pudieron leer por falta de permiso.
    pub hidden: Vec<String>,
}

pub async fn config_review(client: &Client, ram_bytes: Option<i64>) -> Result<ConfigReview> {
    let values = settings(client, NAMES).await?;
    let temp_bytes: Option<i64> = client
        .query_opt(
            "SELECT temp_bytes FROM pg_catalog.pg_stat_database
              WHERE datname = pg_catalog.current_database()",
            &[],
        )
        .await?
        .and_then(|row| row.get(0));
    let hidden = values
        .iter()
        .filter(|(_, s)| s.value.is_none())
        .map(|(name, _)| name.clone())
        .collect();
    Ok(ConfigReview {
        findings: review(
            &values,
            &ReviewContext {
                ram_bytes,
                temp_bytes,
            },
        ),
        hidden,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setting(value: &str, unit: Option<&str>, source: &str, context: &str) -> Setting {
        Setting {
            value: Some(value.into()),
            unit: unit.map(str::to_owned),
            source: source.into(),
            boot_val: None,
            context: context.into(),
        }
    }

    fn factory() -> HashMap<String, Setting> {
        HashMap::from([
            (
                "shared_buffers".into(),
                setting("16384", Some("8kB"), "default", "postmaster"),
            ),
            (
                "effective_cache_size".into(),
                setting("524288", Some("8kB"), "default", "user"),
            ),
            (
                "maintenance_work_mem".into(),
                setting("65536", Some("kB"), "default", "user"),
            ),
            (
                "work_mem".into(),
                setting("4096", Some("kB"), "default", "user"),
            ),
            (
                "checkpoint_completion_target".into(),
                setting("0.9", None, "default", "sighup"),
            ),
            (
                "random_page_cost".into(),
                setting("4", None, "default", "user"),
            ),
            (
                "autovacuum".into(),
                setting("on", None, "default", "sighup"),
            ),
            ("fsync".into(), setting("on", None, "default", "sighup")),
            (
                "full_page_writes".into(),
                setting("on", None, "default", "sighup"),
            ),
            (
                "max_connections".into(),
                setting("100", None, "configuration file", "postmaster"),
            ),
            (
                "idle_in_transaction_session_timeout".into(),
                setting("0", Some("ms"), "default", "user"),
            ),
            (
                "log_min_duration_statement".into(),
                setting("-1", Some("ms"), "default", "superuser"),
            ),
            (
                "shared_preload_libraries".into(),
                setting(
                    "pg_stat_statements",
                    None,
                    "configuration file",
                    "postmaster",
                ),
            ),
        ])
    }

    fn names(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.name.as_str()).collect()
    }

    #[test]
    fn sin_ram_ni_temporales_no_se_sugieren_tamanos() {
        let findings = review(&factory(), &ReviewContext::default());
        assert!(!names(&findings).contains(&"effective_cache_size"));
        assert!(!names(&findings).contains(&"work_mem"));
        let shared = findings
            .iter()
            .find(|f| f.name == "shared_buffers")
            .unwrap();
        assert!(shared.suggested.is_none());
        assert_eq!(shared.level, Level::Info);
    }

    #[test]
    fn con_ram_sugiere_un_cuarto_para_shared_buffers() {
        let context = ReviewContext {
            ram_bytes: Some(16 << 30),
            temp_bytes: None,
        };
        let findings = review(&factory(), &context);
        let shared = findings
            .iter()
            .find(|f| f.name == "shared_buffers")
            .unwrap();
        assert_eq!(shared.suggested.as_deref(), Some("4GB"));
        assert_eq!(
            shared.alter_sql.as_deref(),
            Some("ALTER SYSTEM SET shared_buffers = '4GB'")
        );
        assert!(shared.restart);
        let cache = findings
            .iter()
            .find(|f| f.name == "effective_cache_size")
            .unwrap();
        assert_eq!(cache.suggested.as_deref(), Some("12GB"));
    }

    #[test]
    fn work_mem_solo_se_menciona_con_temporales_de_verdad() {
        let context = ReviewContext {
            ram_bytes: None,
            temp_bytes: Some(3 << 30),
        };
        let findings = review(&factory(), &context);
        assert!(names(&findings).contains(&"work_mem"));
    }

    #[test]
    fn lo_que_puede_danar_datos_va_primero() {
        let mut values = factory();
        values.insert(
            "fsync".into(),
            setting("off", None, "configuration file", "sighup"),
        );
        let findings = review(&values, &ReviewContext::default());
        assert_eq!(findings[0].name, "fsync");
        assert_eq!(findings[0].level, Level::Attention);
    }

    #[test]
    fn un_servidor_en_orden_solo_trae_informativos() {
        let findings = review(&factory(), &ReviewContext::default());
        assert!(findings.iter().all(|f| f.level <= Level::Info));
    }

    #[test]
    fn pg_stat_statements_ausente_se_avisa() {
        let mut values = factory();
        values.insert(
            "shared_preload_libraries".into(),
            setting("", None, "default", "postmaster"),
        );
        let findings = review(&values, &ReviewContext::default());
        let item = findings
            .iter()
            .find(|f| f.name == "shared_preload_libraries")
            .unwrap();
        assert!(item.restart && item.alter_sql.is_none());
    }

    #[test]
    fn el_valor_de_alter_system_escapa_las_comillas() {
        assert_eq!(alter("x", "a'b"), "ALTER SYSTEM SET x = 'a''b'");
    }

    #[test]
    fn un_parametro_oculto_no_genera_hallazgos() {
        let mut values = factory();
        values.insert(
            "shared_preload_libraries".into(),
            Setting {
                value: None,
                unit: None,
                source: "default".into(),
                boot_val: None,
                context: "postmaster".into(),
            },
        );
        let findings = review(&values, &ReviewContext::default());
        assert!(!names(&findings).contains(&"shared_preload_libraries"));
    }
}
