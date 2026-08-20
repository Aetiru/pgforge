//! Comparación de esquemas entre dos servidores.
//!
//! Responde la pregunta que aparece siempre antes de una puesta en producción: qué tiene el
//! esquema de acá que no tiene el de allá. Se lee un esquema de cada lado ([`snapshot`]), se
//! comparan ([`diff`]) y se arma el SQL que llevaría al destino hasta el origen ([`sync`]).
//!
//! **Lo único que toca el servidor es la lectura**, y es a propósito: nada de lo que sale de acá se
//! ejecuta solo. La comparación termina en un informe y en un script que el usuario mira, copia o
//! abre en una pestaña de consulta. Es la misma regla de vista previa que el resto del proyecto,
//! llevada hasta el final: acá no hay un `compare_apply`.
//!
//! Lo que entra en la comparación es la **estructura**: tablas con sus columnas, restricciones e
//! índices; vistas y vistas materializadas; secuencias; y tipos —enumeraciones, compuestos,
//! dominios y rangos—. Quedan afuera funciones, disparadores, políticas y permisos: el cuerpo de
//! una función difiere por un espacio y el informe se llena de ruido que tapa lo que importa.
//! También quedan afuera los datos: esto compara formas, no filas.

pub mod diff;
pub mod render;
pub mod snapshot;
pub mod sync;

pub use diff::{Detail, DetailKind, DiffEntry, ObjectKind, SchemaDiff, SideInfo, Status};
pub use snapshot::SchemaSnapshot;
pub use sync::{Action, Risk, SyncPlan, SyncStatement};

use std::collections::BTreeMap;

use serde::Serialize;

use crate::conn::ServerHandle;
use crate::error::Result;

/// El resultado entero: qué difiere y qué habría que correr para que deje de diferir.
///
/// Los dos viajan juntos en vez de en dos comandos porque salen de la misma lectura: pedirlos por
/// separado haría el doble de consultas para responder lo mismo, y con dos lecturas en momentos
/// distintos el script podría no corresponderse con el informe que se está mirando.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub diff: SchemaDiff,
    pub plan: SyncPlan,
}

/// Compara un esquema contra otro, cada uno en su servidor.
///
/// El **origen** es el estado que se quiere; el **destino**, el que habría que llevar hasta ahí.
/// Los dos lados se leen a la vez: son servidores distintos y esperar uno para empezar el otro
/// duplica lo que tarda sin ganar nada.
pub async fn compare(
    source: &ServerHandle,
    source_database: &str,
    source_schema: &str,
    target: &ServerHandle,
    target_database: &str,
    target_schema: &str,
) -> Result<Comparison> {
    let (source_snapshot, target_snapshot) = tokio::try_join!(
        snapshot::read(source, source_database, source_schema),
        snapshot::read(target, target_database, target_schema),
    )?;

    Ok(Comparison {
        diff: diff::diff(
            &source_snapshot,
            &source.profile.name,
            &target_snapshot,
            &target.profile.name,
        ),
        plan: sync::plan(&source_snapshot, &target_snapshot),
    })
}

/// Empareja dos listas por nombre, en orden alfabético y sin repetir.
///
/// La comparan el informe y el generador de SQL, y tienen que emparejar igual: dos versiones de
/// esta función serían dos maneras de decidir qué objeto es «el mismo» del otro lado.
pub(crate) fn pair<'a, T>(
    source: &'a [T],
    target: &'a [T],
    name: impl Fn(&T) -> &str,
) -> Vec<(String, Option<&'a T>, Option<&'a T>)> {
    let mut map: BTreeMap<String, (Option<&T>, Option<&T>)> = BTreeMap::new();
    for item in source {
        map.entry(name(item).to_owned()).or_default().0 = Some(item);
    }
    for item in target {
        map.entry(name(item).to_owned()).or_default().1 = Some(item);
    }
    map.into_iter()
        .map(|(key, (source, target))| (key, source, target))
        .collect()
}
