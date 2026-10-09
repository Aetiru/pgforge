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

use super::snapshot::{self, NamedDef, SchemaSnapshot};
use crate::conn::ServerHandle;
use crate::error::{Error, Result};
use crate::sql::lex::{lex, TokenKind};

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
    let file: SnapshotFile = serde_json::from_value(value)
        .map_err(|err| Error::Config(format!("la instantánea está dañada: {err}")))?;
    validate(&file.snapshot)?;
    Ok(file)
}

/// Verifica que cada texto de la instantánea que el script va a pegar tal cual sea lo que dice ser.
///
/// Un archivo es entrada no confiable: `render` interpola `definition`, tipos y valores por
/// omisión sin parametrizar, así que una instantánea manipulada con
/// `CREATE INDEX i ON t(a); DROP TABLE clientes` produciría un script que, corrido contra
/// producción, borra la tabla. Una instantánea tomada por pgforge nunca trae `;` ni comentarios
/// en esos textos (los escribe el servidor con `pg_get_*`), así que se rechaza todo el archivo en
/// vez de omitir objetos: una comparación con partes descartadas en silencio también miente.
pub fn validate(snapshot: &SchemaSnapshot) -> Result<()> {
    for table in &snapshot.tables {
        let owner = format!("la tabla «{}»", table.name);
        for column in &table.columns {
            let place = format!("la columna «{}» de {owner}", column.name);
            plain(&place, &column.type_name)?;
            for text in [&column.default, &column.generated, &column.collation]
                .into_iter()
                .flatten()
            {
                plain(&place, text)?;
            }
        }
        if let Some(partition) = &table.partition_by {
            plain(&owner, partition)?;
            starts_with(&owner, partition, &["RANGE", "LIST", "HASH"])?;
        }
        constraints(&owner, &table.constraints)?;
        indexes(&owner, &table.indexes)?;
    }
    for view in &snapshot.views {
        let owner = format!("la vista «{}»", view.name);
        // `pg_get_viewdef` cierra con `;` y `render` lo recorta: se admite uno solo, al final.
        let body = view.definition.trim_end();
        let body = body.strip_suffix(';').unwrap_or(body);
        plain(&owner, body)?;
        let first = lex(body)
            .into_iter()
            .find(|t| t.kind != TokenKind::Whitespace);
        let opens = first.is_some_and(|t| {
            t.text == "("
                || ["SELECT", "WITH", "VALUES", "TABLE"]
                    .iter()
                    .any(|w| t.text.eq_ignore_ascii_case(w))
        });
        if !opens {
            return Err(invalid(&owner, &view.definition, "no es una consulta"));
        }
        indexes(&owner, &view.indexes)?;
    }
    for sequence in &snapshot.sequences {
        plain(
            &format!("la secuencia «{}»", sequence.name),
            &sequence.type_name,
        )?;
    }
    for ty in &snapshot.types {
        let owner = format!("el tipo «{}»", ty.name);
        for field in &ty.fields {
            plain(&owner, &field.data_type)?;
            if let Some(collation) = &field.collation {
                plain(&owner, collation)?;
            }
        }
        for text in ty.base.iter().chain(ty.default.iter()) {
            plain(&owner, text)?;
        }
        constraints(&owner, &ty.checks)?;
    }
    Ok(())
}

fn invalid(owner: &str, text: &str, reason: &str) -> Error {
    Error::Config(format!(
        "la instantánea no es de fiar: en {owner} hay un texto que {reason} («{}»). Un archivo \
         manipulado podría hacer que el script ejecute otra cosa; hay que volver a tomarla",
        text.chars().take(80).collect::<String>()
    ))
}

/// Sin `;` ni comentarios fuera de literales y comillas: una sola expresión, que no puede esconder
/// una segunda sentencia ni comentar el resto de la línea del script.
fn plain(owner: &str, text: &str) -> Result<()> {
    for token in lex(text) {
        match token.kind {
            TokenKind::Punct if token.text == ";" => {
                return Err(invalid(owner, text, "trae más de una sentencia"));
            }
            TokenKind::LineComment | TokenKind::BlockComment => {
                return Err(invalid(owner, text, "trae comentarios"));
            }
            _ => {}
        }
    }
    Ok(())
}

/// La primera palabra del texto tiene que ser una de `allowed`.
fn starts_with(owner: &str, text: &str, allowed: &[&str]) -> Result<()> {
    let first = lex(text)
        .into_iter()
        .find(|t| t.kind != TokenKind::Whitespace);
    match first {
        Some(t) if allowed.iter().any(|w| t.text.eq_ignore_ascii_case(w)) => Ok(()),
        _ => Err(invalid(owner, text, "no empieza como se espera")),
    }
}

fn constraints(owner: &str, list: &[NamedDef]) -> Result<()> {
    for item in list {
        let place = format!("{owner}, restricción «{}»", item.name);
        plain(&place, &item.definition)?;
        starts_with(
            &place,
            &item.definition,
            &["CHECK", "FOREIGN", "PRIMARY", "UNIQUE", "EXCLUDE", "NOT"],
        )?;
    }
    Ok(())
}

fn indexes(owner: &str, list: &[NamedDef]) -> Result<()> {
    for item in list {
        let place = format!("{owner}, índice «{}»", item.name);
        plain(&place, &item.definition)?;
        let words: Vec<String> = lex(&item.definition)
            .into_iter()
            .filter(|t| t.kind == TokenKind::Word)
            .take(3)
            .map(|t| t.text.to_ascii_uppercase())
            .collect();
        let ok = match words.as_slice() {
            [create, index, ..] if create == "CREATE" && index == "INDEX" => true,
            [create, unique, index] => create == "CREATE" && unique == "UNIQUE" && index == "INDEX",
            _ => false,
        };
        if !ok {
            return Err(invalid(&place, &item.definition, "no es un CREATE INDEX"));
        }
    }
    Ok(())
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

    fn manipulada(tocar: impl FnOnce(&mut SnapshotFile)) -> String {
        let mut file = file();
        tocar(&mut file);
        to_json(&file).unwrap()
    }

    #[test]
    fn un_indice_con_una_segunda_sentencia_se_rechaza() {
        let texto = manipulada(|f| {
            f.snapshot.tables[0].indexes.push(NamedDef {
                name: "i".into(),
                definition: "CREATE INDEX i ON t(a); DROP TABLE clientes".into(),
            });
        });
        let error = parse(&texto).unwrap_err().to_string();
        assert!(error.contains("más de una sentencia"), "{error}");
    }

    #[test]
    fn un_punto_y_coma_dentro_de_un_literal_no_es_una_segunda_sentencia() {
        let texto = manipulada(|f| {
            f.snapshot.tables[0].indexes.push(NamedDef {
                name: "i".into(),
                definition: "CREATE UNIQUE INDEX i ON t USING btree (a) WHERE (b = 'x;y')".into(),
            });
        });
        assert!(parse(&texto).is_ok());
    }

    #[test]
    fn un_indice_o_una_restriccion_con_otro_prefijo_se_rechazan() {
        let indice = manipulada(|f| {
            f.snapshot.tables[0].indexes.push(NamedDef {
                name: "i".into(),
                definition: "DROP TABLE clientes".into(),
            });
        });
        assert!(parse(&indice)
            .unwrap_err()
            .to_string()
            .contains("CREATE INDEX"));
        let restriccion = manipulada(|f| {
            f.snapshot.tables[0].constraints[0].definition = "DROP TABLE clientes".into();
        });
        assert!(parse(&restriccion).is_err());
    }

    #[test]
    fn un_comentario_o_un_tipo_con_sentencias_se_rechazan() {
        let comentario = manipulada(|f| {
            f.snapshot.tables[0].constraints[0].definition = "PRIMARY KEY (id) -- x".into();
        });
        assert!(parse(&comentario).is_err());
        let tipo = manipulada(|f| {
            f.snapshot.tables[0].columns[0].type_name = "integer); DROP TABLE clientes; --".into();
        });
        assert!(parse(&tipo).is_err());
        let vista = manipulada(|f| {
            f.snapshot.views.push(crate::compare::snapshot::View {
                name: "v".into(),
                materialized: false,
                definition: " SELECT 1; DROP TABLE clientes;".into(),
                indexes: vec![],
            });
        });
        assert!(parse(&vista).is_err());
    }

    #[test]
    fn una_vista_con_su_punto_y_coma_final_es_valida() {
        let texto = manipulada(|f| {
            f.snapshot.views.push(crate::compare::snapshot::View {
                name: "v".into(),
                materialized: false,
                definition: " SELECT 1;".into(),
                indexes: vec![],
            });
        });
        assert!(parse(&texto).is_ok());
    }

    #[test]
    fn desde_un_archivo_nada_sale_como_seguro() {
        let origen = file();
        let mut destino = origen.snapshot.clone();
        destino.tables.clear();
        // Destino sin la tabla: crearla es `Safe` en vivo, y desde un archivo tiene que ser `Review`.
        let en_vivo = crate::compare::compare_snapshots(&origen.snapshot, "a", &destino, "b");
        assert!(en_vivo
            .plan
            .statements
            .iter()
            .any(|s| s.risk == crate::compare::Risk::Safe));
        let desde_archivo = crate::compare::compare_file_with_snapshot(&origen, &destino, "b");
        assert!(!desde_archivo.plan.statements.is_empty());
        assert!(desde_archivo
            .plan
            .statements
            .iter()
            .all(|s| s.risk != crate::compare::Risk::Safe));
    }

    #[test]
    fn el_rotulo_distingue_la_instantanea_del_servidor_vivo() {
        assert!(file().label().starts_with("producción (instantánea"));
    }
}
