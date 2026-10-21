//! Funciones, disparadores, políticas y permisos: el informe y el script sobre las mismas
//! instantáneas.
//!
//! Van juntos porque la pregunta es una sola —¿lo que dice el informe es lo que hace el script?— y
//! porque los dos comparten fixtures. Todo es puro: ningún servidor.

use super::diff::{diff, DetailKind, ObjectKind, Status};
use super::snapshot::{
    AclKind, Column, Function, FunctionKind, Grant, ObjectAcl, Policy, PolicyCommand, RelationKind,
    RowSecurity, SchemaSnapshot, Table, Trigger, TriggerEnabled, View,
};
use super::sync::{plan, Action, Risk, SyncPlan, SyncStatement};
use crate::ServerVersion;

fn snapshot(schema: &str) -> SchemaSnapshot {
    snapshot_pg(schema, 160_004)
}

fn snapshot_pg(schema: &str, version: i32) -> SchemaSnapshot {
    SchemaSnapshot {
        database: "app".to_owned(),
        schema: schema.to_owned(),
        version: ServerVersion::from_num(version),
        schemas: vec![schema.to_owned()],
        tables: Vec::new(),
        views: Vec::new(),
        sequences: Vec::new(),
        types: Vec::new(),
        functions: Vec::new(),
        triggers: Vec::new(),
        policies: Vec::new(),
        row_security: Vec::new(),
        acls: Vec::new(),
    }
}

fn table(name: &str) -> Table {
    Table {
        name: name.to_owned(),
        kind: RelationKind::Ordinary,
        partition_by: None,
        columns: vec![Column {
            name: "id".to_owned(),
            type_name: "bigint".to_owned(),
            not_null: false,
            default: None,
            identity: None,
            generated: None,
            collation: None,
        }],
        constraints: Vec::new(),
        indexes: Vec::new(),
    }
}

fn with_table(schema: &str, names: &[&str]) -> SchemaSnapshot {
    let mut snapshot = snapshot(schema);
    snapshot.tables = names.iter().map(|name| table(name)).collect();
    snapshot
}

/// Una función tal como la escribe `pg_get_functiondef`.
fn function(schema: &str, name: &str, arguments: &str, body: &str) -> Function {
    Function {
        name: name.to_owned(),
        arguments: arguments.to_owned(),
        kind: FunctionKind::Function,
        definition: format!(
            "CREATE OR REPLACE FUNCTION {schema}.{name}({arguments})\n RETURNS integer\n \
             LANGUAGE plpgsql\nAS $function$\n{body}\n$function$\n"
        ),
    }
}

fn trigger(schema: &str, table: &str, name: &str, function: &str) -> Trigger {
    Trigger {
        table: table.to_owned(),
        name: name.to_owned(),
        definition: format!(
            "CREATE TRIGGER {name} BEFORE INSERT ON {schema}.{table} FOR EACH ROW EXECUTE \
             FUNCTION {schema}.{function}()"
        ),
        enabled: TriggerEnabled::Origin,
    }
}

fn policy(table: &str, name: &str) -> Policy {
    Policy {
        table: table.to_owned(),
        name: name.to_owned(),
        command: PolicyCommand::Select,
        permissive: true,
        roles: vec!["PUBLIC".to_owned()],
        using: Some("(tenant = 1)".to_owned()),
        with_check: None,
    }
}

fn grant(grantee: &str, privilege: &str) -> Grant {
    Grant {
        grantee: grantee.to_owned(),
        privilege: privilege.to_owned(),
        grantable: false,
    }
}

fn acl(kind: AclKind, name: &str, grants: Vec<Grant>) -> ObjectAcl {
    ObjectAcl {
        kind,
        name: name.to_owned(),
        arguments: None,
        grants,
    }
}

fn statements_of(plan: &SyncPlan, object: ObjectKind) -> Vec<&SyncStatement> {
    plan.statements
        .iter()
        .filter(|statement| statement.object == object)
        .collect()
}

fn position(plan: &SyncPlan, object: ObjectKind) -> usize {
    plan.statements
        .iter()
        .position(|statement| statement.object == object)
        .unwrap_or_else(|| panic!("no hay ninguna sentencia de {object:?}"))
}

// ---------------------------------------------------------------- funciones: el informe

#[test]
fn dos_funciones_iguales_no_son_una_diferencia() {
    let mut source = snapshot("public");
    let mut target = snapshot("public");
    source
        .functions
        .push(function("public", "doble", "integer", "RETURN x * 2;"));
    target
        .functions
        .push(function("public", "doble", "integer", "RETURN x * 2;"));

    let report = diff(&source, "a", &target, "b");
    assert!(report.is_empty());
    assert_eq!(report.equal, 1);
}

#[test]
fn el_formato_del_cuerpo_no_es_una_diferencia() {
    let mut source = snapshot("public");
    let mut target = snapshot("public");
    source.functions.push(function(
        "public",
        "doble",
        "integer",
        "BEGIN\n    RETURN x * 2;\nEND;",
    ));
    // Otra sangría, finales de línea de Windows, espacios de más y al final de cada línea.
    target.functions.push(function(
        "public",
        "doble",
        "integer",
        "BEGIN  \r\n\t\tRETURN   x * 2;   \r\nEND;\r\n\r\n",
    ));

    assert!(diff(&source, "a", &target, "b").is_empty());
    assert!(plan(&source, &target).statements.is_empty());
}

#[test]
fn los_espacios_de_un_literal_si_son_una_diferencia() {
    let mut source = snapshot("public");
    let mut target = snapshot("public");
    source.functions.push(function(
        "public",
        "aviso",
        "",
        "BEGIN RAISE NOTICE 'dos  espacios'; RETURN 1; END;",
    ));
    target.functions.push(function(
        "public",
        "aviso",
        "",
        "BEGIN RAISE NOTICE 'dos espacios'; RETURN 1; END;",
    ));

    let report = diff(&source, "a", &target, "b");
    assert_eq!(report.entries.len(), 1);
    assert_eq!(report.entries[0].status, Status::Different);
}

#[test]
fn un_cuerpo_distinto_sale_como_diferencia_de_definicion() {
    let mut source = snapshot("public");
    let mut target = snapshot("public");
    source
        .functions
        .push(function("public", "doble", "integer", "RETURN x * 2;"));
    target
        .functions
        .push(function("public", "doble", "integer", "RETURN x * 3;"));

    let report = diff(&source, "a", &target, "b");
    let entry = &report.entries[0];
    assert_eq!(entry.kind, ObjectKind::Function);
    assert_eq!(entry.name, "doble(integer)");
    assert_eq!(entry.details[0].kind, DetailKind::Property);
    assert_eq!(entry.details[0].name, "definición");
    assert!(entry.source_ddl.as_ref().unwrap().contains("x * 2"));
    assert!(entry.target_ddl.as_ref().unwrap().contains("x * 3"));
}

#[test]
fn la_sobrecarga_se_compara_por_firma_y_no_por_nombre() {
    let mut source = snapshot("public");
    let mut target = snapshot("public");
    source
        .functions
        .push(function("public", "f", "integer", "RETURN 1;"));
    source
        .functions
        .push(function("public", "f", "text", "RETURN 2;"));
    target
        .functions
        .push(function("public", "f", "integer", "RETURN 1;"));

    let report = diff(&source, "a", &target, "b");
    assert_eq!(report.entries.len(), 1);
    assert_eq!(report.entries[0].name, "f(text)");
    assert_eq!(report.entries[0].status, Status::OnlySource);
    assert_eq!(report.equal, 1);
}

#[test]
fn la_firma_no_depende_del_esquema_en_que_se_lee() {
    // Con el esquema con otro nombre en cada servidor, el tipo llega como `dev.estado` y como
    // `prod.estado`: la misma función.
    let mut source = snapshot("dev");
    let mut target = snapshot("prod");
    source
        .functions
        .push(function("dev", "f", "dev.estado", "RETURN 1;"));
    target
        .functions
        .push(function("prod", "f", "prod.estado", "RETURN 1;"));

    assert!(diff(&source, "a", &target, "b").is_empty());
}

#[test]
fn el_nombre_del_esquema_dentro_de_la_funcion_no_cuenta() {
    let mut source = snapshot("dev");
    let mut target = snapshot("prod");
    source.functions.push(function(
        "dev",
        "f",
        "",
        "RETURN (SELECT count(*) FROM dev.t);",
    ));
    target.functions.push(function(
        "prod",
        "f",
        "",
        "RETURN (SELECT count(*) FROM prod.t);",
    ));

    assert!(diff(&source, "a", &target, "b").is_empty());
}

#[test]
fn un_procedimiento_se_distingue_de_una_funcion() {
    let mut source = snapshot("public");
    let mut procedure = function("public", "limpiar", "", "BEGIN END;");
    procedure.kind = FunctionKind::Procedure;
    source.functions.push(procedure);

    let report = diff(&source, "a", &snapshot("public"), "b");
    assert_eq!(report.entries[0].kind, ObjectKind::Procedure);
}

// ---------------------------------------------------------------- funciones: el script

#[test]
fn una_funcion_nueva_se_crea_y_es_segura() {
    let mut source = snapshot("dev");
    source
        .functions
        .push(function("dev", "doble", "integer", "RETURN 2;"));

    let plan = plan(&source, &snapshot("prod"));
    let statement = &plan.statements[0];
    assert_eq!(statement.object, ObjectKind::Function);
    assert_eq!(statement.action, Action::Create);
    assert_eq!(statement.risk, Risk::Safe);
    // Reescrita contra el destino y terminada en `;`.
    assert!(statement
        .sql
        .starts_with("CREATE OR REPLACE FUNCTION prod.doble(integer)"));
    assert!(statement.sql.ends_with("$function$;"));
}

#[test]
fn una_funcion_cambiada_se_reemplaza_y_queda_para_revisar() {
    let mut source = snapshot("public");
    let mut target = snapshot("public");
    source
        .functions
        .push(function("public", "doble", "integer", "RETURN 2;"));
    target
        .functions
        .push(function("public", "doble", "integer", "RETURN 3;"));

    let plan = plan(&source, &target);
    assert_eq!(plan.statements.len(), 1);
    assert_eq!(plan.statements[0].action, Action::Alter);
    assert_eq!(plan.statements[0].risk, Risk::Review);
    assert!(plan.statements[0]
        .note
        .as_ref()
        .unwrap()
        .contains("CREATE OR REPLACE"));
}

#[test]
fn borrar_una_funcion_es_destructivo_y_nombra_los_argumentos() {
    let mut target = snapshot("public");
    target
        .functions
        .push(function("public", "f", "integer", "RETURN 1;"));
    target
        .functions
        .push(function("public", "f", "text", "RETURN 2;"));
    let mut source = snapshot("public");
    source
        .functions
        .push(function("public", "f", "integer", "RETURN 1;"));

    let plan = plan(&source, &target);
    assert_eq!(plan.statements.len(), 1);
    assert_eq!(plan.statements[0].risk, Risk::Destructive);
    assert_eq!(plan.statements[0].action, Action::Drop);
    // Por firma y sin CASCADE: se borra la sobrecarga que sobra, no la otra.
    assert_eq!(plan.statements[0].sql, "DROP FUNCTION public.f(text);");
}

#[test]
fn un_procedimiento_se_borra_como_procedimiento() {
    let mut target = snapshot("public");
    let mut procedure = function("public", "limpiar", "", "BEGIN END;");
    procedure.kind = FunctionKind::Procedure;
    target.functions.push(procedure);

    let plan = plan(&snapshot("public"), &target);
    assert_eq!(plan.statements[0].sql, "DROP PROCEDURE public.limpiar();");
}

#[test]
fn una_funcion_sql_nueva_avisa_del_orden() {
    let mut source = snapshot("public");
    let mut sql = function("public", "doble", "integer", "SELECT 2");
    sql.definition = sql.definition.replace("plpgsql", "sql");
    source.functions.push(sql);

    let plan = plan(&source, &snapshot("public"));
    assert!(plan.statements[0]
        .note
        .as_ref()
        .unwrap()
        .contains("check_function_bodies"));
}

// ---------------------------------------------------------------- disparadores

#[test]
fn un_disparador_que_falta_aparece_y_se_crea_despues_de_su_funcion() {
    let mut source = with_table("public", &["pedidos"]);
    let target = with_table("public", &["pedidos"]);
    source
        .functions
        .push(function("public", "auditar", "", "RETURN NULL;"));
    source
        .triggers
        .push(trigger("public", "pedidos", "pedidos_auditar", "auditar"));

    let report = diff(&source, "a", &target, "b");
    let entry = report
        .entries
        .iter()
        .find(|entry| entry.kind == ObjectKind::Trigger)
        .unwrap();
    assert_eq!(entry.status, Status::OnlySource);
    assert_eq!(entry.name, "pedidos.pedidos_auditar");

    let plan = plan(&source, &target);
    assert!(position(&plan, ObjectKind::Function) < position(&plan, ObjectKind::Trigger));
    let create = &statements_of(&plan, ObjectKind::Trigger)[0];
    assert_eq!(create.risk, Risk::Review);
    assert!(create.sql.starts_with("CREATE TRIGGER pedidos_auditar"));
    assert!(create.sql.ends_with(';'));
}

#[test]
fn el_disparador_de_una_tabla_nueva_viaja_con_ella() {
    let mut source = with_table("public", &["pedidos"]);
    source
        .triggers
        .push(trigger("public", "pedidos", "pedidos_auditar", "auditar"));
    let target = snapshot("public");

    let report = diff(&source, "a", &target, "b");
    assert!(report.entries.iter().all(|e| e.kind != ObjectKind::Trigger));
    assert_eq!(report.entries[0].kind, ObjectKind::Table);

    // Pero el script sí lo crea, y después de la tabla.
    let plan = plan(&source, &target);
    assert!(position(&plan, ObjectKind::Table) < position(&plan, ObjectKind::Trigger));
}

#[test]
fn un_disparador_cambiado_se_borra_y_se_vuelve_a_crear() {
    let mut source = with_table("public", &["pedidos"]);
    let mut target = with_table("public", &["pedidos"]);
    source
        .triggers
        .push(trigger("public", "pedidos", "t", "nueva"));
    target
        .triggers
        .push(trigger("public", "pedidos", "t", "vieja"));

    let plan = plan(&source, &target);
    let sentences = statements_of(&plan, ObjectKind::Trigger);
    assert_eq!(sentences.len(), 2);
    assert_eq!(sentences[0].sql, "DROP TRIGGER t ON public.pedidos;");
    assert_eq!(sentences[0].risk, Risk::Destructive);
    assert!(sentences[1].sql.contains("EXECUTE FUNCTION public.nueva()"));
    assert_eq!(sentences[1].risk, Risk::Review);
}

#[test]
fn el_esquema_del_disparador_no_es_una_diferencia() {
    let mut source = with_table("dev", &["pedidos"]);
    let mut target = with_table("prod", &["pedidos"]);
    source.triggers.push(trigger("dev", "pedidos", "t", "f"));
    target.triggers.push(trigger("prod", "pedidos", "t", "f"));

    assert!(diff(&source, "a", &target, "b").is_empty());
}

#[test]
fn un_disparador_deshabilitado_se_reporta_y_se_ajusta() {
    let mut source = with_table("public", &["pedidos"]);
    let mut target = with_table("public", &["pedidos"]);
    let mut apagado = trigger("public", "pedidos", "t", "f");
    apagado.enabled = TriggerEnabled::Disabled;
    source.triggers.push(apagado);
    target.triggers.push(trigger("public", "pedidos", "t", "f"));

    let report = diff(&source, "a", &target, "b");
    assert_eq!(report.entries[0].details[0].name, "estado");

    let plan = plan(&source, &target);
    assert_eq!(plan.statements.len(), 1);
    assert_eq!(
        plan.statements[0].sql,
        "ALTER TABLE public.pedidos DISABLE TRIGGER t;"
    );
}

#[test]
fn un_disparador_que_sobra_se_borra_pero_no_si_se_borra_su_tabla() {
    let source = with_table("public", &["pedidos"]);
    let mut target = with_table("public", &["pedidos", "viejas"]);
    target
        .triggers
        .push(trigger("public", "pedidos", "sobra", "f"));
    target
        .triggers
        .push(trigger("public", "viejas", "de_la_tabla", "f"));

    let plan = plan(&source, &target);
    let drops: Vec<&str> = plan
        .statements
        .iter()
        .filter(|s| s.object == ObjectKind::Trigger)
        .map(|s| s.sql.as_str())
        .collect();
    assert_eq!(drops, vec!["DROP TRIGGER sobra ON public.pedidos;"]);
}

// ---------------------------------------------------------------- políticas y RLS

#[test]
fn una_politica_con_otra_condicion_se_altera() {
    let mut source = with_table("public", &["pedidos"]);
    let mut target = with_table("public", &["pedidos"]);
    source.policies.push(policy("pedidos", "por_tenant"));
    let mut vieja = policy("pedidos", "por_tenant");
    vieja.using = Some("(tenant = 2)".to_owned());
    target.policies.push(vieja);

    let report = diff(&source, "a", &target, "b");
    assert_eq!(report.entries[0].kind, ObjectKind::Policy);
    assert_eq!(report.entries[0].details[0].name, "USING");

    let plan = plan(&source, &target);
    assert_eq!(plan.statements.len(), 1);
    assert_eq!(plan.statements[0].action, Action::Alter);
    assert_eq!(plan.statements[0].risk, Risk::Review);
    assert!(plan.statements[0]
        .sql
        .starts_with("ALTER POLICY por_tenant ON public.pedidos"));
    assert!(plan.statements[0].sql.contains("USING ((tenant = 1))"));
}

#[test]
fn una_politica_solo_cambia_los_roles_sin_tocar_las_condiciones() {
    let mut source = with_table("public", &["pedidos"]);
    let mut target = with_table("public", &["pedidos"]);
    let mut nueva = policy("pedidos", "p");
    nueva.roles = vec!["app".to_owned()];
    source.policies.push(nueva);
    target.policies.push(policy("pedidos", "p"));

    let plan = plan(&source, &target);
    assert!(plan.statements[0].sql.contains("TO app"));
    assert!(!plan.statements[0].sql.contains("USING"));
}

#[test]
fn cambiar_el_comando_de_una_politica_la_recrea() {
    let mut source = with_table("public", &["pedidos"]);
    let mut target = with_table("public", &["pedidos"]);
    let mut nueva = policy("pedidos", "p");
    nueva.command = PolicyCommand::Update;
    source.policies.push(nueva);
    target.policies.push(policy("pedidos", "p"));

    let plan = plan(&source, &target);
    assert_eq!(plan.statements.len(), 2);
    assert_eq!(plan.statements[0].sql, "DROP POLICY p ON public.pedidos;");
    assert_eq!(plan.statements[0].risk, Risk::Destructive);
    assert!(plan.statements[1].sql.contains("FOR UPDATE"));
}

#[test]
fn quitar_una_condicion_tambien_recrea_la_politica() {
    // ALTER POLICY no puede dejar una expresión en nada.
    let mut source = with_table("public", &["pedidos"]);
    let mut target = with_table("public", &["pedidos"]);
    let mut sin = policy("pedidos", "p");
    sin.using = None;
    source.policies.push(sin);
    target.policies.push(policy("pedidos", "p"));

    let plan = plan(&source, &target);
    assert_eq!(plan.statements[0].action, Action::Drop);
}

#[test]
fn una_politica_restrictiva_se_escribe_como_tal() {
    let mut source = with_table("public", &["pedidos"]);
    let mut restrictiva = policy("pedidos", "p");
    restrictiva.permissive = false;
    restrictiva.with_check = Some("(tenant = 1)".to_owned());
    source.policies.push(restrictiva);

    let plan = plan(&source, &with_table("public", &["pedidos"]));
    let sql = &plan.statements[0].sql;
    assert!(sql.contains("AS RESTRICTIVE"));
    assert!(sql.contains("TO PUBLIC"));
    assert!(sql.contains("WITH CHECK ((tenant = 1))"));
}

#[test]
fn la_seguridad_por_filas_se_enciende_despues_de_las_politicas() {
    let mut source = with_table("public", &["pedidos"]);
    source.row_security.push(RowSecurity {
        table: "pedidos".to_owned(),
        enabled: true,
        forced: true,
    });
    source.policies.push(policy("pedidos", "p"));
    let target = with_table("public", &["pedidos"]);

    let report = diff(&source, "a", &target, "b");
    let rls = report
        .entries
        .iter()
        .find(|e| e.kind == ObjectKind::RowSecurity)
        .unwrap();
    assert_eq!(rls.details.len(), 2);

    let plan = plan(&source, &target);
    assert!(position(&plan, ObjectKind::Policy) < position(&plan, ObjectKind::RowSecurity));
    let sql: Vec<&str> = statements_of(&plan, ObjectKind::RowSecurity)
        .iter()
        .map(|s| s.sql.as_str())
        .collect();
    assert_eq!(
        sql,
        vec![
            "ALTER TABLE public.pedidos ENABLE ROW LEVEL SECURITY;",
            "ALTER TABLE public.pedidos FORCE ROW LEVEL SECURITY;"
        ]
    );
}

#[test]
fn apagar_la_seguridad_por_filas_queda_para_revisar() {
    let source = with_table("public", &["pedidos"]);
    let mut target = with_table("public", &["pedidos"]);
    target.row_security.push(RowSecurity {
        table: "pedidos".to_owned(),
        enabled: true,
        forced: false,
    });

    let plan = plan(&source, &target);
    assert_eq!(
        plan.statements[0].sql,
        "ALTER TABLE public.pedidos DISABLE ROW LEVEL SECURITY;"
    );
    assert_eq!(plan.statements[0].risk, Risk::Review);
}

#[test]
fn la_seguridad_de_una_tabla_que_falta_viaja_con_ella() {
    let mut source = with_table("public", &["pedidos"]);
    source.row_security.push(RowSecurity {
        table: "pedidos".to_owned(),
        enabled: true,
        forced: false,
    });
    let report = diff(&source, "a", &snapshot("public"), "b");
    assert!(report
        .entries
        .iter()
        .all(|e| e.kind != ObjectKind::RowSecurity));
}

#[test]
fn una_politica_que_sobra_se_borra_al_final() {
    let source = with_table("public", &["pedidos"]);
    let mut target = with_table("public", &["pedidos"]);
    target.policies.push(policy("pedidos", "sobra"));

    let plan = plan(&source, &target);
    assert_eq!(
        plan.statements[0].sql,
        "DROP POLICY sobra ON public.pedidos;"
    );
    assert_eq!(plan.statements[0].risk, Risk::Destructive);
}

// ---------------------------------------------------------------- permisos

#[test]
fn un_permiso_que_falta_se_otorga_y_uno_que_sobra_se_revoca() {
    let mut source = with_table("public", &["pedidos"]);
    let mut target = with_table("public", &["pedidos"]);
    source.acls.push(acl(
        AclKind::Table,
        "pedidos",
        vec![grant("ana", "SELECT"), grant("ana", "INSERT")],
    ));
    target.acls.push(acl(
        AclKind::Table,
        "pedidos",
        vec![grant("ana", "SELECT"), grant("luis", "DELETE")],
    ));

    let report = diff(&source, "a", &target, "b");
    let entry = &report.entries[0];
    assert_eq!(entry.kind, ObjectKind::Grant);
    assert_eq!(entry.name, "pedidos");
    let details: Vec<(&str, Status)> = entry
        .details
        .iter()
        .map(|d| (d.name.as_str(), d.status))
        .collect();
    assert_eq!(
        details,
        vec![("ana", Status::Different), ("luis", Status::OnlyTarget)]
    );

    let plan = plan(&source, &target);
    let sql: Vec<&str> = plan.statements.iter().map(|s| s.sql.as_str()).collect();
    assert_eq!(
        sql,
        vec![
            "GRANT INSERT ON TABLE public.pedidos TO ana;",
            "REVOKE DELETE ON TABLE public.pedidos FROM luis;"
        ]
    );
    // Ninguno pierde datos, los dos cambian quién puede qué.
    assert!(plan.statements.iter().all(|s| s.risk == Risk::Review));
    assert!(plan.statements[0].note.as_ref().unwrap().contains("«ana»"));
}

#[test]
fn el_orden_de_los_permisos_no_es_una_diferencia() {
    let mut source = with_table("public", &["pedidos"]);
    let mut target = with_table("public", &["pedidos"]);
    source.acls.push(acl(
        AclKind::Table,
        "pedidos",
        vec![
            grant("ana", "SELECT"),
            grant("ana", "INSERT"),
            grant("luis", "SELECT"),
        ],
    ));
    target.acls.push(acl(
        AclKind::Table,
        "pedidos",
        vec![
            grant("luis", "SELECT"),
            grant("ana", "INSERT"),
            grant("ana", "SELECT"),
        ],
    ));

    assert!(diff(&source, "a", &target, "b").is_empty());
    assert!(plan(&source, &target).statements.is_empty());
}

#[test]
fn los_permisos_de_un_objeto_nuevo_viajan_con_el_y_no_inflan_los_iguales() {
    let mut source = with_table("public", &["pedidos"]);
    source
        .acls
        .push(acl(AclKind::Table, "pedidos", vec![grant("ana", "SELECT")]));
    let target = snapshot("public");

    let report = diff(&source, "a", &target, "b");
    assert!(report.entries.iter().all(|e| e.kind != ObjectKind::Grant));

    let plan = plan(&source, &target);
    assert!(position(&plan, ObjectKind::Table) < position(&plan, ObjectKind::Grant));
    assert_eq!(
        statements_of(&plan, ObjectKind::Grant)[0].sql,
        "GRANT SELECT ON TABLE public.pedidos TO ana;"
    );
}

#[test]
fn public_no_se_cita_y_la_opcion_de_otorgar_se_agrega_y_se_quita() {
    let mut source = with_table("public", &["pedidos"]);
    let mut target = with_table("public", &["pedidos"]);
    source.acls.push(acl(
        AclKind::Table,
        "pedidos",
        vec![
            grant("PUBLIC", "SELECT"),
            Grant {
                grantable: true,
                ..grant("Ana", "UPDATE")
            },
            grant("bea", "INSERT"),
        ],
    ));
    target.acls.push(acl(
        AclKind::Table,
        "pedidos",
        vec![
            Grant {
                grantable: true,
                ..grant("bea", "INSERT")
            },
            grant("Ana", "UPDATE"),
        ],
    ));

    let plan = plan(&source, &target);
    let sql: Vec<&str> = plan.statements.iter().map(|s| s.sql.as_str()).collect();
    assert_eq!(
        sql,
        // Por rol, en orden: «Ana», PUBLIC, bea.
        vec![
            "GRANT UPDATE ON TABLE public.pedidos TO \"Ana\" WITH GRANT OPTION;",
            "GRANT SELECT ON TABLE public.pedidos TO PUBLIC;",
            "REVOKE GRANT OPTION FOR INSERT ON TABLE public.pedidos FROM bea;",
        ]
    );
    assert!(plan.statements[1].note.is_none());
}

#[test]
fn los_permisos_de_una_funcion_van_por_firma() {
    let mut source = snapshot("public");
    let mut target = snapshot("public");
    let mut con = acl(AclKind::Function, "f", vec![grant("ana", "EXECUTE")]);
    con.arguments = Some("integer".to_owned());
    let mut sin = acl(AclKind::Function, "f", Vec::new());
    sin.arguments = Some("integer".to_owned());
    // Otra sobrecarga con el mismo nombre: no se mezcla.
    let mut otra = acl(AclKind::Function, "f", vec![grant("luis", "EXECUTE")]);
    otra.arguments = Some("text".to_owned());
    source.acls.extend([con.clone(), otra.clone()]);
    target.acls.extend([sin, otra]);

    let plan = plan(&source, &target);
    assert_eq!(plan.statements.len(), 1);
    assert_eq!(
        plan.statements[0].sql,
        "GRANT EXECUTE ON FUNCTION public.f(integer) TO ana;"
    );
}

#[test]
fn una_funcion_nueva_sin_execute_para_todos_lo_revoca() {
    let mut source = snapshot("public");
    source
        .functions
        .push(function("public", "secreta", "", "RETURN 1;"));
    let mut sin_public = acl(AclKind::Function, "secreta", vec![grant("ana", "EXECUTE")]);
    sin_public.arguments = Some(String::new());
    source.acls.push(sin_public);

    let plan = plan(&source, &snapshot("public"));
    let sql: Vec<&str> = statements_of(&plan, ObjectKind::Grant)
        .iter()
        .map(|s| s.sql.as_str())
        .collect();
    assert_eq!(
        sql,
        vec![
            "REVOKE EXECUTE ON FUNCTION public.secreta() FROM PUBLIC;",
            "GRANT EXECUTE ON FUNCTION public.secreta() TO ana;"
        ]
    );
}

#[test]
fn un_permiso_de_postgresql_17_no_se_otorga_a_un_destino_anterior() {
    let mut source = snapshot_pg("public", 170_000);
    source.tables.push(table("pedidos"));
    source.acls.push(acl(
        AclKind::Table,
        "pedidos",
        vec![grant("ana", "MAINTAIN"), grant("ana", "SELECT")],
    ));
    let mut target = snapshot_pg("public", 160_004);
    target.tables.push(table("pedidos"));
    target.acls.push(acl(AclKind::Table, "pedidos", Vec::new()));

    let plan = plan(&source, &target);
    assert_eq!(plan.statements.len(), 1);
    assert_eq!(
        plan.statements[0].sql,
        "GRANT SELECT ON TABLE public.pedidos TO ana;"
    );
    assert!(plan.warnings[0].contains("MAINTAIN"));
}

#[test]
fn una_vista_materializada_recreada_pierde_sus_permisos_y_se_vuelven_a_dar() {
    let view = |definition: &str| View {
        name: "resumen".to_owned(),
        materialized: true,
        definition: definition.to_owned(),
        indexes: Vec::new(),
    };
    let mut source = snapshot("public");
    let mut target = snapshot("public");
    source.views.push(view(" SELECT 1 AS a;"));
    target.views.push(view(" SELECT 2 AS a;"));
    // Iguales de los dos lados: sin la recreación no habría nada que otorgar.
    let permisos = || acl(AclKind::Table, "resumen", vec![grant("ana", "SELECT")]);
    source.acls.push(permisos());
    target.acls.push(permisos());

    let plan = plan(&source, &target);
    assert!(plan
        .statements
        .iter()
        .any(|s| s.sql == "GRANT SELECT ON TABLE public.resumen TO ana;"));
}

// ---------------------------------------------------------------- el orden del script

#[test]
fn el_script_respeta_las_dependencias() {
    let mut source = with_table("public", &["pedidos"]);
    source
        .functions
        .push(function("public", "auditar", "", "RETURN NULL;"));
    source
        .triggers
        .push(trigger("public", "pedidos", "t", "auditar"));
    source.policies.push(policy("pedidos", "p"));
    source.row_security.push(RowSecurity {
        table: "pedidos".to_owned(),
        enabled: true,
        forced: false,
    });
    source
        .acls
        .push(acl(AclKind::Table, "pedidos", vec![grant("ana", "SELECT")]));
    let mut target = with_table("public", &["viejas"]);
    target
        .functions
        .push(function("public", "vieja", "", "RETURN 1;"));

    let plan = plan(&source, &target);
    let order = [
        position(&plan, ObjectKind::Function),
        position(&plan, ObjectKind::Table),
        position(&plan, ObjectKind::Trigger),
        position(&plan, ObjectKind::Policy),
        position(&plan, ObjectKind::RowSecurity),
        position(&plan, ObjectKind::Grant),
    ];
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");

    // Y lo destructivo, al final de todo: se puede cortar el script ahí.
    let first_destructive = plan
        .statements
        .iter()
        .position(|s| s.risk == Risk::Destructive)
        .unwrap();
    assert!(plan.statements[first_destructive..]
        .iter()
        .all(|s| s.risk == Risk::Destructive));
    assert!(first_destructive > order[5]);
}

#[test]
fn dos_esquemas_sin_logica_no_producen_sentencias() {
    let plan = plan(&snapshot("a"), &snapshot("b"));
    assert!(plan.statements.is_empty());
    assert!(plan.warnings.is_empty());
}
