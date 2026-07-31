//! Capacidades del servidor conectado.
//!
//! El catálogo de PostgreSQL cambia entre versiones: una consulta escrita contra PG 17 falla
//! contra PG 13. En vez de escribir consultas atadas a una versión, cada módulo pregunta acá qué
//! ofrece el servidor con el que está hablando y arma la consulta en consecuencia.

use serde::{Deserialize, Serialize};

/// Versión mínima soportada, en el formato de `server_version_num` (PG 13.0).
pub const MIN_SUPPORTED_VERSION_NUM: i32 = 130_000;

/// Versión del servidor tal como la reporta `SHOW server_version_num`.
///
/// Desde PG 10 el número se compone como `major * 10000 + minor`, así que 130_004 es 13.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ServerVersion(i32);

impl ServerVersion {
    pub fn from_num(num: i32) -> Self {
        Self(num)
    }

    pub fn num(self) -> i32 {
        self.0
    }

    pub fn major(self) -> i32 {
        self.0 / 10_000
    }

    pub fn minor(self) -> i32 {
        self.0 % 10_000
    }

    /// `true` si el servidor es al menos de la versión mayor indicada.
    pub fn at_least(self, major: i32) -> bool {
        self.major() >= major
    }

    pub fn is_supported(self) -> bool {
        self.0 >= MIN_SUPPORTED_VERSION_NUM
    }
}

impl std::fmt::Display for ServerVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major(), self.minor())
    }
}

/// Contexto del servidor y del rol conectado. Se calcula una vez al conectar y se cachea: nada de
/// esto cambia dentro de una sesión, y volver a consultarlo en cada operación es desperdicio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCaps {
    pub version: ServerVersion,
    pub current_user: String,
    pub current_database: String,
    pub is_superuser: bool,
    /// Superusuario o miembro de `pg_signal_backend`. Sin esto no se pueden cancelar ni terminar
    /// sesiones de otros usuarios, y conviene avisarlo antes de intentarlo.
    pub can_signal_backends: bool,
    /// Superusuario o miembro de `pg_read_all_stats`. Sin esto `pg_stat_activity` oculta el texto
    /// de las consultas de otros roles y el monitoreo se ve vacío sin explicación.
    pub can_read_all_stats: bool,
}

impl ServerCaps {
    /// `pg_stat_activity.query_id`, para agrupar ejecuciones de la misma consulta.
    pub fn has_query_id(&self) -> bool {
        self.version.at_least(14)
    }

    /// `pg_stat_activity.leader_pid`, para atribuir los workers paralelos a su sesión líder.
    pub fn has_leader_pid(&self) -> bool {
        self.version.at_least(13)
    }

    /// Vista `pg_stat_io`.
    pub fn has_pg_stat_io(&self) -> bool {
        self.version.at_least(16)
    }

    /// `REINDEX` con la opción `CONCURRENTLY`.
    pub fn has_reindex_concurrently(&self) -> bool {
        self.version.at_least(12)
    }

    /// `VACUUM (PROCESS_TOAST ...)` y demás opciones agregadas en PG 14.
    pub fn has_vacuum_process_toast(&self) -> bool {
        self.version.at_least(14)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descompone_el_numero_de_version() {
        let v = ServerVersion::from_num(130_004);
        assert_eq!(v.major(), 13);
        assert_eq!(v.minor(), 4);
        assert_eq!(v.to_string(), "13.4");
    }

    #[test]
    fn rechaza_versiones_anteriores_a_la_minima() {
        assert!(!ServerVersion::from_num(120_018).is_supported());
        assert!(ServerVersion::from_num(130_000).is_supported());
        assert!(ServerVersion::from_num(170_002).is_supported());
    }

    #[test]
    fn las_capacidades_dependen_de_la_version_mayor() {
        let caps = |num| ServerCaps {
            version: ServerVersion::from_num(num),
            current_user: "postgres".into(),
            current_database: "postgres".into(),
            is_superuser: true,
            can_signal_backends: true,
            can_read_all_stats: true,
        };

        assert!(!caps(130_000).has_query_id());
        assert!(caps(140_000).has_query_id());
        assert!(!caps(150_000).has_pg_stat_io());
        assert!(caps(160_000).has_pg_stat_io());
    }
}
