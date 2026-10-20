//! Lo que tienen en común el informe y el script a la hora de comparar permisos.
//!
//! Todo puro. Los permisos se comparan **por rol y por privilegio**, no por la ACL entera: dos ACL
//! escritas en distinto orden son la misma, y lo que hay que emitir para igualarlas es un `GRANT`
//! o un `REVOKE` por lo que de verdad cambió, no la ACL de un lado pisando la del otro.
//!
//! También vive acá la identidad de una función o procedimiento ([`signature`]): nombre y tipos de
//! los argumentos, sin el esquema propio. Dos servidores pueden tener el esquema con otro nombre, y
//! `oidvectortypes` califica un tipo del esquema solo cuando no está en el `search_path`, así que el
//! mismo `f(estado)` puede venir como `f(dev.estado)` o como `f(estado)` según desde dónde se lea.

use std::collections::BTreeMap;

use crate::ddl::quote_ident;

use super::snapshot::{AclKind, Grant};

/// Privilegios de cada rol: `privilegio → ¿con GRANT OPTION?`, ordenados para que el texto salga
/// siempre igual.
pub(crate) type ByGrantee = BTreeMap<String, BTreeMap<String, bool>>;

pub(crate) fn by_grantee(grants: &[Grant]) -> ByGrantee {
    let mut map = ByGrantee::new();
    for grant in grants {
        let privilege = map
            .entry(grant.grantee.clone())
            .or_default()
            .entry(grant.privilege.clone())
            .or_insert(false);
        // Un privilegio repetido con y sin opción vale con opción: es lo que quedó otorgado.
        *privilege |= grant.grantable;
    }
    map
}

/// Lo que un objeto recién creado tiene sin que nadie lo otorgue, sin contar a su dueño.
///
/// Las funciones nacen con `EXECUTE` para todos (`acldefault`); el resto, para nadie. Es la base
/// contra la que se compara un objeto que el script va a crear: si el origen le sacó ese `EXECUTE`
/// a `PUBLIC`, el script tiene que revocarlo después de crearla, y sin esta base no lo vería.
pub(crate) fn factory(kind: AclKind) -> Vec<Grant> {
    match kind {
        AclKind::Function | AclKind::Procedure => vec![Grant {
            grantee: "PUBLIC".to_owned(),
            privilege: "EXECUTE".to_owned(),
            grantable: false,
        }],
        AclKind::Table | AclKind::Sequence => Vec::new(),
    }
}

/// Lo que hay que hacer para un rol, ya separado en las cuatro sentencias posibles.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct GranteeChange {
    pub grantee: String,
    /// `GRANT` simple.
    pub grant: Vec<String>,
    /// `GRANT … WITH GRANT OPTION`.
    pub grant_option: Vec<String>,
    /// `REVOKE`: sobra el privilegio entero.
    pub revoke: Vec<String>,
    /// `REVOKE GRANT OPTION FOR`: queda el privilegio pero no la opción de pasarlo.
    pub revoke_option: Vec<String>,
}

impl GranteeChange {
    fn is_empty(&self) -> bool {
        self.grant.is_empty()
            && self.grant_option.is_empty()
            && self.revoke.is_empty()
            && self.revoke_option.is_empty()
    }
}

/// Qué cambiar en los permisos de `target` para que queden como los de `source`.
pub(crate) fn changes(source: &[Grant], target: &[Grant]) -> Vec<GranteeChange> {
    let source = by_grantee(source);
    let target = by_grantee(target);
    let empty = BTreeMap::new();

    let mut grantees: Vec<&String> = source.keys().chain(target.keys()).collect();
    grantees.sort();
    grantees.dedup();

    let mut out = Vec::new();
    for grantee in grantees {
        let wanted = source.get(grantee).unwrap_or(&empty);
        let current = target.get(grantee).unwrap_or(&empty);

        let mut change = GranteeChange {
            grantee: grantee.clone(),
            ..GranteeChange::default()
        };
        let mut privileges: Vec<&String> = wanted.keys().chain(current.keys()).collect();
        privileges.sort();
        privileges.dedup();

        for privilege in privileges {
            match (wanted.get(privilege), current.get(privilege)) {
                (Some(true), None) | (Some(true), Some(false)) => {
                    change.grant_option.push(privilege.clone())
                }
                (Some(false), None) => change.grant.push(privilege.clone()),
                (Some(false), Some(true)) => change.revoke_option.push(privilege.clone()),
                (None, Some(_)) => change.revoke.push(privilege.clone()),
                _ => {}
            }
        }

        if !change.is_empty() {
            out.push(change);
        }
    }
    out
}

/// El rol como se escribe en un `GRANT`: `PUBLIC` es una palabra clave y no se cita.
pub(crate) fn grantee_sql(grantee: &str) -> String {
    if grantee == "PUBLIC" {
        grantee.to_owned()
    } else {
        quote_ident(grantee)
    }
}

/// Los privilegios de un rol como una línea de texto, para el informe.
pub(crate) fn privileges_text(privileges: &BTreeMap<String, bool>) -> String {
    privileges
        .iter()
        .map(|(privilege, grantable)| {
            if *grantable {
                format!("{privilege} (GRANT OPTION)")
            } else {
                privilege.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Saca de `sql` los prefijos `esquema.` del esquema indicado.
///
/// Solo donde empieza un nombre: `mi_dev.tipo` no es una referencia al esquema `dev`.
pub(crate) fn strip_schema(sql: &str, schema: &str) -> String {
    let quoted = quote_ident(schema);
    let mut out = remove_prefix(sql, &format!("{quoted}."));
    if quoted != schema {
        out = remove_prefix(&out, &format!("{schema}."));
    }
    out
}

fn remove_prefix(sql: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut rest = 0;
    for (at, _) in sql.match_indices(prefix) {
        let before = sql[..at].chars().next_back();
        let inside_name = before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '"');
        if inside_name || at < rest {
            continue;
        }
        out.push_str(&sql[rest..at]);
        rest = at + prefix.len();
    }
    out.push_str(&sql[rest..]);
    out
}

/// `nombre(tipos)` de una función o procedimiento, independiente del nombre del esquema.
pub(crate) fn signature(name: &str, arguments: &str, schema: &str) -> String {
    format!("{}({})", quote_ident(name), strip_schema(arguments, schema))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(grantee: &str, privilege: &str, grantable: bool) -> Grant {
        Grant {
            grantee: grantee.to_owned(),
            privilege: privilege.to_owned(),
            grantable,
        }
    }

    #[test]
    fn dos_acl_en_otro_orden_son_la_misma() {
        let a = [grant("ana", "SELECT", false), grant("ana", "UPDATE", false)];
        let b = [grant("ana", "UPDATE", false), grant("ana", "SELECT", false)];
        assert!(changes(&a, &b).is_empty());
    }

    #[test]
    fn otorga_lo_que_falta_y_revoca_lo_que_sobra() {
        let origen = [grant("ana", "SELECT", false), grant("ana", "INSERT", false)];
        let destino = [grant("ana", "SELECT", false), grant("ana", "DELETE", false)];
        let cambios = changes(&origen, &destino);
        assert_eq!(cambios.len(), 1);
        assert_eq!(cambios[0].grant, vec!["INSERT"]);
        assert_eq!(cambios[0].revoke, vec!["DELETE"]);
    }

    #[test]
    fn la_opcion_de_otorgar_se_agrega_y_se_quita_sin_tocar_el_privilegio() {
        let con = [grant("ana", "SELECT", true)];
        let sin = [grant("ana", "SELECT", false)];

        let agrega = changes(&con, &sin);
        assert_eq!(agrega[0].grant_option, vec!["SELECT"]);
        assert!(agrega[0].revoke.is_empty());

        let quita = changes(&sin, &con);
        assert_eq!(quita[0].revoke_option, vec!["SELECT"]);
        assert!(quita[0].revoke.is_empty());
    }

    #[test]
    fn un_rol_que_solo_esta_en_el_destino_pierde_todo() {
        let cambios = changes(&[], &[grant("ana", "SELECT", false)]);
        assert_eq!(cambios[0].revoke, vec!["SELECT"]);
    }

    #[test]
    fn una_funcion_nueva_arranca_con_execute_para_todos() {
        assert_eq!(factory(AclKind::Function)[0].privilege, "EXECUTE");
        assert!(factory(AclKind::Table).is_empty());
        // El origen le sacó el EXECUTE a PUBLIC: el script tiene que revocarlo.
        let cambios = changes(&[], &factory(AclKind::Function));
        assert_eq!(cambios[0].grantee, "PUBLIC");
        assert_eq!(cambios[0].revoke, vec!["EXECUTE"]);
    }

    #[test]
    fn public_no_se_cita() {
        assert_eq!(grantee_sql("PUBLIC"), "PUBLIC");
        assert_eq!(grantee_sql("Ana"), "\"Ana\"");
    }

    #[test]
    fn la_firma_no_depende_del_nombre_del_esquema() {
        assert_eq!(
            signature("f", "dev.estado, integer", "dev"),
            signature("f", "estado, integer", "prod")
        );
        // `mi_dev.` no es el esquema `dev`.
        assert_eq!(signature("f", "mi_dev.t", "dev"), "f(mi_dev.t)");
    }

    #[test]
    fn la_sobrecarga_da_firmas_distintas() {
        assert_ne!(
            signature("f", "integer", "public"),
            signature("f", "text", "public")
        );
    }
}
