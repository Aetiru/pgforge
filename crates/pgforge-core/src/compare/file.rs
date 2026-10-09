//! Instantáneas de un esquema guardadas en un archivo.
//!
//! Comparar en vivo responde «qué tiene este servidor que no tiene aquel». Lo que no responde es
//! «qué cambió desde el martes»: para eso hace falta el esquema de antes, y el de antes ya no está
//! en ningún servidor. Una instantánea guardada antes de un deploy es ese otro lado.
//!
//! El archivo es JSON con una marca y un número de formato adelante. Se verifican los dos antes de
//! leer el resto, por dos motivos: un JSON cualquiera tiene que fallar diciendo «esto no es una
//! instantánea» y no «missing field `tables`», y un archivo de un formato viejo no se puede leer
//! «más o menos»: una comparación contra una instantánea mal interpretada miente con toda seguridad.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::snapshot::{self, SchemaSnapshot};
use crate::conn::ServerHandle;
use crate::error::{Error, Result};

/// Qué es el archivo. Distingue una instantánea de cualquier otro JSON del disco.
const KIND: &str = "pgforge-schema-snapshot";

/// Versión del formato. Sube cuando cambia la forma de [`SchemaSnapshot`] de manera que un archivo
/// viejo ya no signifique lo mismo.
pub const FORMAT: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFile {
    pub kind: String,
    pub format: u32,
    /// Nombre del perfil del que se tomó. Es un dato para mostrar, no una atadura: la instantánea
    /// se compara contra cualquier servidor.
    pub server: String,
    /// Cuándo se tomó, con el reloj del servidor y como lo escribe PostgreSQL.
    pub taken_at: String,
    pub snapshot: SchemaSnapshot,
}

impl SnapshotFile {
    /// Cómo se nombra este lado en el informe: el perfil solo no alcanza, porque comparar el
    /// servidor contra su propia instantánea dejaría los dos lados con el mismo nombre.
    pub fn label(&self) -> String {
        format!("{} (instantánea del {})", self.server, self.taken_at)
    }
}

pub fn to_json(file: &SnapshotFile) -> Result<String> {
    serde_json::to_string_pretty(file)
        .map_err(|err| Error::Config(format!("no se pudo armar la instantánea: {err}")))
}

pub fn parse(text: &str) -> Result<SnapshotFile> {
    let not_a_snapshot =
        || Error::Config("el archivo no es una instantánea de esquema de pgforge".to_owned());

    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| not_a_snapshot())?;
    if value.get("kind").and_then(|kind| kind.as_str()) != Some(KIND) {
        return Err(not_a_snapshot());
    }
    let format = value.get("format").and_then(|format| format.as_u64());
    if format != Some(u64::from(FORMAT)) {
        return Err(Error::Config(format!(
            "la instantánea tiene el formato {}, y esta versión de pgforge lee el {FORMAT}: \
             hay que volver a tomarla",
            format.map_or_else(|| "desconocido".to_owned(), |format| format.to_string())
        )));
    }
    serde_json::from_value(value)
        .map_err(|err| Error::Config(format!("la instantánea está dañada: {err}")))
}

/// Lee el esquema y lo escribe en `path`.
pub async fn save(handle: &ServerHandle, database: &str, schema: &str, path: &Path) -> Result<()> {
    let snapshot = snapshot::read(handle, database, schema).await?;
    let taken_at: String = handle
        .client(database)
        .await?
        .query_one(
            "SELECT pg_catalog.to_char(now(), 'YYYY-MM-DD HH24:MI:SS TZ')",
            &[],
        )
        .await?
        .get(0);

    let file = SnapshotFile {
        kind: KIND.to_owned(),
        format: FORMAT,
        server: handle.profile.name.clone(),
        taken_at,
        snapshot,
    };
    std::fs::write(path, to_json(&file)?)?;
    Ok(())
}

pub fn load(path: &Path) -> Result<SnapshotFile> {
    parse(&std::fs::read_to_string(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::snapshot::{Column, NamedDef, RelationKind, Table};
    use crate::ServerVersion;

    fn file() -> SnapshotFile {
        SnapshotFile {
            kind: KIND.to_owned(),
            format: FORMAT,
            server: "producción".into(),
            taken_at: "2026-10-07 12:00:00 -05".into(),
            snapshot: SchemaSnapshot {
                database: "ventas".into(),
                schema: "public".into(),
                version: ServerVersion::from_num(160_004),
                schemas: vec!["public".into()],
                tables: vec![Table {
                    name: "clientes".into(),
                    kind: RelationKind::Ordinary,
                    partition_by: None,
                    columns: vec![Column {
                        name: "id".into(),
                        type_name: "integer".into(),
                        not_null: true,
                        default: None,
                        identity: None,
                        generated: None,
                        collation: None,
                    }],
                    constraints: vec![NamedDef {
                        name: "clientes_pkey".into(),
                        definition: "PRIMARY KEY (id)".into(),
                    }],
                    indexes: vec![],
                }],
                views: vec![],
                sequences: vec![],
                types: vec![],
            },
        }
    }

    #[test]
    fn una_instantanea_guardada_se_vuelve_a_leer_igual() {
        let leida = parse(&to_json(&file()).unwrap()).unwrap();
        assert_eq!(leida.server, "producción");
        assert_eq!(
            leida.snapshot.tables[0].columns[0],
            file().snapshot.tables[0].columns[0]
        );
        assert_eq!(leida.snapshot.version, file().snapshot.version);
    }

    #[test]
    fn un_json_cualquiera_no_pasa_por_instantanea() {
        let error = parse(r#"{"tables": []}"#).unwrap_err().to_string();
        assert!(error.contains("no es una instantánea"), "{error}");
        assert!(parse("esto no es json").is_err());
    }

    #[test]
    fn un_formato_distinto_se_rechaza_con_el_motivo() {
        let mut viejo = serde_json::to_value(file()).unwrap();
        viejo["format"] = 0.into();
        let error = parse(&viejo.to_string()).unwrap_err().to_string();
        assert!(error.contains("volver a tomarla"), "{error}");
    }

    #[test]
    fn el_rotulo_distingue_la_instantanea_del_servidor_vivo() {
        assert!(file().label().starts_with("producción (instantánea"));
    }
}
