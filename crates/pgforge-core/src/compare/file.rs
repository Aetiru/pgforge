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

use super::snapshot::{self, NamedDef, SchemaSnapshot, PRIVILEGES};
use crate::conn::ServerHandle;
use crate::error::{Error, Result};
use crate::sql::lex::{lex, TokenKind};

/// Qué es el archivo. Distingue una instantánea de cualquier otro JSON del disco.
const KIND: &str = "pgforge-schema-snapshot";

/// Versión del formato. Sube cuando cambia la forma de [`SchemaSnapshot`] de manera que un archivo
/// viejo ya no signifique lo mismo.
///
/// **El 2 agregó funciones, disparadores, políticas, seguridad por filas y permisos, y `parse`
/// rechaza el 1 en lugar de leerlo «sin esas secciones».** Leerlo sin ellas parecería inofensivo y
/// mentiría: una instantánea vieja no dice «no había funciones», dice «no se miraron», pero
/// comparada contra un servidor vivo sería lo primero —cada función, disparador y permiso del
/// servidor saldría como sobrante, y el script propondría borrarlos—. Volver a tomar la instantánea
/// cuesta una lectura; un informe que miente cuesta un `DROP FUNCTION` de más.
pub const FORMAT: u32 = 2;

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
            "la instantánea tiene el formato {}, y esta versión de pgforge lee el {FORMAT} \
             (desde el 2 incluye funciones, disparadores, políticas y permisos, que una anterior no \
             trae y compararía como si no existieran): hay que volver a tomarla",
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
    for function in &snapshot.functions {
        let owner = format!("la función «{}»", function.name);
        balanced(&owner, &function.arguments)?;
        routine_definition(&owner, &function.definition)?;
    }
    for trigger in &snapshot.triggers {
        let owner = format!("el disparador «{}» de «{}»", trigger.name, trigger.table);
        plain(&owner, &trigger.definition)?;
        let words: Vec<String> = lex(&trigger.definition)
            .into_iter()
            .filter(|t| t.kind == TokenKind::Word)
            .take(3)
            .map(|t| t.text.to_ascii_uppercase())
            .collect();
        let ok = match words.as_slice() {
            [create, keyword, ..] if create == "CREATE" && keyword == "TRIGGER" => true,
            [create, constraint, keyword] => {
                create == "CREATE" && constraint == "CONSTRAINT" && keyword == "TRIGGER"
            }
            _ => false,
        };
        if !ok {
            return Err(invalid(
                &owner,
                &trigger.definition,
                "no es un CREATE TRIGGER",
            ));
        }
    }
    for policy in &snapshot.policies {
        let owner = format!("la política «{}» de «{}»", policy.name, policy.table);
        for text in policy.using.iter().chain(policy.with_check.iter()) {
            balanced(&owner, text)?;
        }
    }
    for object in &snapshot.acls {
        let owner = format!("los permisos de «{}»", object.name);
        if let Some(text) = &object.arguments {
            balanced(&owner, text)?;
        }
        for grant in &object.grants {
            if !PRIVILEGES.contains(&grant.privilege.as_str()) {
                return Err(invalid(
                    &owner,
                    &grant.privilege,
                    "no es un privilegio conocido",
                ));
            }
        }
    }
    Ok(())
}

/// Un texto que se pega sin comillas dentro de unos paréntesis —los tipos de los argumentos de una
/// función, la condición de una política—:
/// una sola expresión y con los paréntesis balanceados, o un `)` de más cerraría la lista y dejaría
/// el resto como parte de la sentencia.
fn balanced(owner: &str, text: &str) -> Result<()> {
    plain(owner, text)?;
    let mut depth = 0usize;
    for token in lex(text) {
        if token.kind != TokenKind::Punct {
            continue;
        }
        match token.text.as_str() {
            "(" => depth += 1,
            ")" => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| invalid(owner, text, "cierra un paréntesis que no abrió"))?;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(invalid(owner, text, "deja un paréntesis sin cerrar"));
    }
    Ok(())
}

/// Un `CREATE [OR REPLACE] FUNCTION|PROCEDURE` y nada más.
///
/// No sirve `plain`: el cuerpo de una función es código y trae `;` —pero entre `$$ … $$`, que el
/// lexer cierra en un solo token, así que ahí no cuentan—. Lo único que escapa a eso es el cuerpo
/// de estándar SQL (`BEGIN ATOMIC … END`), que `pg_get_functiondef` escribe con `;` sin comillas
/// por dentro. Se admiten **solo** entre ese `BEGIN ATOMIC` y su `END` (contando los `CASE … END`
/// de adentro): un `;` fuera de ahí que no sea el último carácter sería una segunda sentencia, y
/// una instantánea manipulada podría colar un `DROP TABLE` justo después del cuerpo.
fn routine_definition(owner: &str, text: &str) -> Result<()> {
    let tokens: Vec<_> = lex(text)
        .into_iter()
        .filter(|t| t.kind != TokenKind::Whitespace)
        .collect();
    let word = |at: usize, expected: &str| {
        tokens
            .get(at)
            .is_some_and(|t| t.kind == TokenKind::Word && t.text.eq_ignore_ascii_case(expected))
    };

    let after_create = if word(0, "CREATE") && word(1, "OR") && word(2, "REPLACE") {
        3
    } else {
        1
    };
    if !(word(0, "CREATE") && (word(after_create, "FUNCTION") || word(after_create, "PROCEDURE"))) {
        return Err(invalid(
            owner,
            text,
            "no es un CREATE FUNCTION ni CREATE PROCEDURE",
        ));
    }

    let last = tokens.len() - 1;
    let mut depth = 0usize;
    let mut atomic = false;
    for (at, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::LineComment | TokenKind::BlockComment if !atomic => {
                return Err(invalid(owner, text, "trae comentarios"));
            }
            TokenKind::Punct if token.text == ";" && !atomic && at != last => {
                return Err(invalid(owner, text, "trae más de una sentencia"));
            }
            TokenKind::Word if atomic => {
                if token.text.eq_ignore_ascii_case("CASE") {
                    depth += 1;
                } else if token.text.eq_ignore_ascii_case("END") {
                    depth -= 1;
                    atomic = depth > 0;
                }
            }
            TokenKind::Word if word(at, "BEGIN") && word(at + 1, "ATOMIC") => {
                atomic = true;
                depth = 1;
            }
            _ => {}
        }
    }
    if atomic {
        return Err(invalid(owner, text, "abre un BEGIN ATOMIC que no cierra"));
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
    use crate::compare::snapshot::{
        AclKind, Column, Function, FunctionKind, Grant, NamedDef, ObjectAcl, Policy, PolicyCommand,
        RelationKind, RowSecurity, Table, Trigger, TriggerEnabled,
    };
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
                functions: vec![],
                triggers: vec![],
                policies: vec![],
                row_security: vec![],
                acls: vec![],
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
    fn el_formato_1_se_rechaza_y_explica_por_que() {
        // Leerlo «sin esas secciones» compararía cada función y permiso del servidor como sobrante.
        let mut viejo = serde_json::to_value(file()).unwrap();
        viejo["format"] = 1.into();
        let error = parse(&viejo.to_string()).unwrap_err().to_string();
        assert!(error.contains("formato 1"), "{error}");
        assert!(error.contains("funciones"), "{error}");
        assert!(error.contains("volver a tomarla"), "{error}");
    }

    fn funcion(definition: &str) -> Function {
        Function {
            name: "f".into(),
            arguments: "integer".into(),
            kind: FunctionKind::Function,
            definition: definition.into(),
        }
    }

    fn con_funcion(definition: &str) -> String {
        manipulada(|f| f.snapshot.functions.push(funcion(definition)))
    }

    const FUNCION: &str = "CREATE OR REPLACE FUNCTION public.f(integer)\n RETURNS integer\n \
                           LANGUAGE plpgsql\nAS $function$\nBEGIN\n  RETURN 1; -- ; ok\nEND;\n\
                           $function$\n";

    #[test]
    fn las_secciones_nuevas_se_guardan_y_se_leen_de_vuelta() {
        let texto = manipulada(|f| {
            f.snapshot.functions.push(funcion(FUNCION));
            f.snapshot.triggers.push(Trigger {
                table: "clientes".into(),
                name: "t".into(),
                definition: "CREATE TRIGGER t BEFORE INSERT ON public.clientes FOR EACH ROW \
                             EXECUTE FUNCTION public.f()"
                    .into(),
                enabled: TriggerEnabled::Disabled,
            });
            f.snapshot.policies.push(Policy {
                table: "clientes".into(),
                name: "p".into(),
                command: PolicyCommand::All,
                permissive: true,
                roles: vec!["PUBLIC".into()],
                using: Some("(id > 0)".into()),
                with_check: None,
            });
            f.snapshot.row_security.push(RowSecurity {
                table: "clientes".into(),
                enabled: true,
                forced: false,
            });
            f.snapshot.acls.push(ObjectAcl {
                kind: AclKind::Function,
                name: "f".into(),
                arguments: Some("integer".into()),
                grants: vec![Grant {
                    grantee: "PUBLIC".into(),
                    privilege: "EXECUTE".into(),
                    grantable: false,
                }],
            });
        });
        let leida = parse(&texto).unwrap();
        assert_eq!(leida.snapshot.functions.len(), 1);
        assert_eq!(leida.snapshot.triggers[0].enabled, TriggerEnabled::Disabled);
        assert_eq!(
            leida.snapshot.policies[0].using.as_deref(),
            Some("(id > 0)")
        );
        assert!(leida.snapshot.row_security[0].enabled);
        assert_eq!(leida.snapshot.acls[0].grants[0].privilege, "EXECUTE");
    }

    #[test]
    fn el_punto_y_coma_de_adentro_del_cuerpo_no_es_una_segunda_sentencia() {
        assert!(parse(&con_funcion(FUNCION)).is_ok());
        // Ni el que cierra la sentencia.
        assert!(parse(&con_funcion(&format!("{};", FUNCION.trim_end()))).is_ok());
    }

    #[test]
    fn una_funcion_con_una_sentencia_despues_del_cuerpo_se_rechaza() {
        let inyectada = format!("{}; DROP TABLE clientes", FUNCION.trim_end());
        let error = parse(&con_funcion(&inyectada)).unwrap_err().to_string();
        assert!(error.contains("más de una sentencia"), "{error}");
    }

    #[test]
    fn una_funcion_que_no_empieza_como_create_function_se_rechaza() {
        for texto in [
            "DROP TABLE clientes",
            "CREATE TABLE x (a int)",
            "SELECT 1",
            "",
        ] {
            assert!(parse(&con_funcion(texto)).is_err(), "{texto}");
        }
    }

    #[test]
    fn el_cuerpo_de_estandar_sql_admite_punto_y_coma_solo_hasta_su_end() {
        let valida = "CREATE OR REPLACE FUNCTION public.f(integer)\n RETURNS integer\n \
                      LANGUAGE sql\nBEGIN ATOMIC\n SELECT CASE WHEN true THEN 1 ELSE 2 END;\n \
                      SELECT 3;\nEND";
        assert!(parse(&con_funcion(valida)).is_ok());

        // El `END;` cierra el cuerpo: lo que sigue ya es otra sentencia.
        let inyectada =
            format!("{valida}; DROP TABLE clientes; CREATE FUNCTION g() BEGIN ATOMIC END");
        assert!(parse(&con_funcion(&inyectada)).is_err());

        // Un cuerpo que nunca cierra no deja pasar `;` sin control.
        let abierta = "CREATE FUNCTION public.f() RETURNS int LANGUAGE sql BEGIN ATOMIC SELECT 1; \
                       DROP TABLE clientes;";
        assert!(parse(&con_funcion(abierta)).is_err());
    }

    #[test]
    fn los_argumentos_de_una_funcion_no_pueden_cerrar_el_parentesis() {
        for argumentos in [
            "integer) TO publico; DROP TABLE x --",
            "integer))",
            "(integer",
        ] {
            let texto = manipulada(|f| {
                f.snapshot.acls.push(ObjectAcl {
                    kind: AclKind::Function,
                    name: "f".into(),
                    arguments: Some(argumentos.into()),
                    grants: vec![],
                });
            });
            assert!(parse(&texto).is_err(), "{argumentos}");
        }
        let texto = manipulada(|f| {
            f.snapshot.acls.push(ObjectAcl {
                kind: AclKind::Function,
                name: "f".into(),
                arguments: Some("integer, numeric(10,2)".into()),
                grants: vec![],
            });
        });
        assert!(parse(&texto).is_ok());
    }

    #[test]
    fn un_privilegio_desconocido_se_rechaza() {
        let texto = manipulada(|f| {
            f.snapshot.acls.push(ObjectAcl {
                kind: AclKind::Table,
                name: "clientes".into(),
                arguments: None,
                grants: vec![Grant {
                    grantee: "ana".into(),
                    privilege: "SELECT ON ALL TABLES IN SCHEMA public TO x; --".into(),
                    grantable: false,
                }],
            });
        });
        let error = parse(&texto).unwrap_err().to_string();
        assert!(error.contains("privilegio"), "{error}");
    }

    #[test]
    fn un_disparador_o_una_politica_con_otra_sentencia_se_rechazan() {
        let disparador = manipulada(|f| {
            f.snapshot.triggers.push(Trigger {
                table: "clientes".into(),
                name: "t".into(),
                definition: "DROP TABLE clientes".into(),
                enabled: TriggerEnabled::Origin,
            });
        });
        assert!(parse(&disparador)
            .unwrap_err()
            .to_string()
            .contains("CREATE TRIGGER"));

        let politica = manipulada(|f| {
            f.snapshot.policies.push(Policy {
                table: "clientes".into(),
                name: "p".into(),
                command: PolicyCommand::All,
                permissive: true,
                roles: vec![],
                using: Some("true); DROP TABLE clientes; --".into()),
                with_check: None,
            });
        });
        assert!(parse(&politica).is_err());
    }

    #[test]
    fn el_rotulo_distingue_la_instantanea_del_servidor_vivo() {
        assert!(file().label().starts_with("producción (instantánea"));
    }
}
