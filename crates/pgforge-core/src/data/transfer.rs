//! Copiar una tabla de un servidor a otro.
//!
//! Hasta ahora pasar una tabla de producción a desarrollo era exportar a un archivo, crear la tabla
//! a mano e importar. Acá el `COPY ... TO STDOUT` de un servidor alimenta directamente al
//! `COPY ... FROM STDIN` del otro, sin archivo intermedio y sin que las filas pasen enteras por
//! memoria: se reenvían los trozos tal como llegan.
//!
//! Se parte como el resto de las mutaciones: [`plan`] arma las sentencias y es **pura** —lo único que
//! se puede verificar sin servidor, y lo que la vista previa muestra tal cual—; [`prepare`] lee el
//! catálogo de origen y [`run`] ejecuta.
//!
//! **Qué se copia y qué no**, dicho con claridad porque copiar «casi todo» en silencio es peor que
//! avisar: columnas con su tipo, `NOT NULL`, valores por omisión, claves primarias y únicas, `CHECK`
//! y, si se pide, los índices. **No** se copian las claves foráneas (apuntan a tablas que quizá no
//! están del otro lado), ni los disparadores, ni los permisos, ni las secuencias: un `serial` o una
//! columna de identidad queda como una columna común con sus valores, y la secuencia de destino hay
//! que ajustarla a mano. Cada una de esas omisiones sale en `warnings`.

use bytes::Bytes;
use futures_util::{SinkExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use tokio_postgres::CopyInSink;

use crate::conn::ServerHandle;
use crate::data::io::Outcome;
use crate::data::shape::oid_of;
use crate::ddl::{qualified, quote_ident};
use crate::error::{Error, Result};

/// Cada cuántos bytes se avisa el avance: el mismo paso que exportar e importar.
const REPORT_EVERY: u64 = 256 * 1024;

/// Qué hacer con la tabla de destino.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransferMode {
    /// La tabla no existe: se crea. Si ya existe, es un error y no se toca nada.
    Create,
    /// La tabla existe: se le agregan las filas.
    Append,
    /// La tabla existe: se vacía y se cargan las filas, todo en la misma transacción.
    Truncate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSpec {
    /// La tabla de origen.
    pub schema: String,
    pub table: String,
    /// Cómo se llama del otro lado: puede ser distinto.
    pub target_schema: String,
    pub target_table: String,
    pub mode: TransferMode,
    /// Con `false` solo se crea la estructura. Solo con `Create`.
    pub with_data: bool,
    /// Recrear los índices que no sostienen una restricción. Solo con `Create`.
    pub with_indexes: bool,
    /// Condición `WHERE` para copiar solo parte de las filas. Va cruda: misma frontera de confianza
    /// que el editor, la escribe el usuario y la ejecuta con sus propios privilegios.
    #[serde(default)]
    pub filter: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceColumn {
    pub name: String,
    pub type_name: String,
    pub not_null: bool,
    pub default: Option<String>,
    pub identity: bool,
    /// Columna generada almacenada: la calcula el servidor, no se copia su valor.
    pub generated: bool,
}

/// Un índice del origen, listo para rearmarse apuntando a otra tabla.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceIndex {
    /// La definición completa tal como la escribe `pg_get_indexdef`.
    pub definition: String,
    /// Lo que `pg_get_indexdef` escribe antes del método: `CREATE [UNIQUE] INDEX nombre ON tabla `.
    pub prefix: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTable {
    pub columns: Vec<SourceColumn>,
    /// `PRIMARY KEY (…)`, `UNIQUE (…)`, `CHECK (…)`, sin nombre: PostgreSQL les pone el suyo, porque
    /// el nombre de una restricción única es también el de su índice y chocaría al copiar dentro de
    /// un mismo esquema.
    pub constraints: Vec<String>,
    pub indexes: Vec<SourceIndex>,
    /// Cuántas claves foráneas tiene, para avisar que no se copian.
    pub foreign_keys: usize,
    /// `true` si es una tabla particionada: se copia como una tabla común.
    pub partitioned: bool,
}

/// Lo que se ejecutaría, para mostrarlo antes de hacerlo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferPlan {
    /// Sentencias de destino antes de cargar: el `CREATE TABLE` o el `TRUNCATE`.
    pub before: Vec<String>,
    /// El `COPY … TO STDOUT` que se corre en el origen. `None` si no se copian datos.
    pub copy_out: Option<String>,
    /// El `COPY … FROM STDIN` que se corre en el destino.
    pub copy_in: Option<String>,
    /// Sentencias de destino después de cargar: los índices (más rápido que cargar con ellos).
    pub after: Vec<String>,
    pub warnings: Vec<String>,
}

impl TransferPlan {
    /// Todo lo que se ejecuta, en orden, como un solo texto para la vista previa y el historial.
    pub fn script(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.extend(self.before.iter().map(|sql| format!("{sql};")));
        if let (Some(out), Some(into)) = (&self.copy_out, &self.copy_in) {
            lines.push(format!("-- origen:  {out}"));
            lines.push(format!("-- destino: {into}"));
        }
        lines.extend(self.after.iter().map(|sql| format!("{sql};")));
        lines.join("\n")
    }
}

/// Arma las sentencias. Pura.
pub fn plan(spec: &TransferSpec, source: &SourceTable) -> Result<TransferPlan> {
    if spec.target_table.trim().is_empty() || spec.target_schema.trim().is_empty() {
        return Err(Error::Config(
            "falta el esquema o el nombre de la tabla de destino".into(),
        ));
    }
    if spec.mode != TransferMode::Create && (!spec.with_data || spec.with_indexes) {
        // Sin crear nada, «solo estructura» no significa nada y los índices ya están o no.
        return Err(Error::Config(
            "agregar o reemplazar filas en una tabla que ya existe no admite «solo estructura» ni recrear índices".into(),
        ));
    }
    let filter = spec
        .filter
        .as_deref()
        .map(str::trim)
        .filter(|f| !f.is_empty());
    let filter = filter.map(|f| f.trim_end_matches(';').trim());
    if let Some(filter) = filter {
        // Un `;` o un comentario de línea abriría la puerta a otra sentencia dentro del `COPY (…)`.
        if filter.contains(';') || filter.contains("--") {
            return Err(Error::Config(
                "el filtro no puede llevar «;» ni comentarios «--»".into(),
            ));
        }
    }

    let target = qualified(&spec.target_schema, &spec.target_table);
    let from = qualified(&spec.schema, &spec.table);
    let mut warnings = Vec::new();
    let mut before = Vec::new();

    match spec.mode {
        TransferMode::Create => {
            let mut parts: Vec<String> = Vec::new();
            let mut sequences: Vec<&str> = Vec::new();
            for column in &source.columns {
                let mut line = format!("    {} {}", quote_ident(&column.name), column.type_name);
                if column.generated {
                    if let Some(expression) = &column.default {
                        line.push_str(&format!(" GENERATED ALWAYS AS ({expression}) STORED"));
                    }
                } else if let Some(default) = &column.default {
                    if default.contains("nextval(") {
                        // Apunta a una secuencia del origen que del otro lado no existe.
                        sequences.push(&column.name);
                    } else {
                        line.push_str(&format!(" DEFAULT {default}"));
                    }
                }
                if column.identity {
                    sequences.push(&column.name);
                }
                if column.not_null {
                    line.push_str(" NOT NULL");
                }
                parts.push(line);
            }
            parts.extend(source.constraints.iter().map(|c| format!("    {c}")));
            before.push(format!("CREATE TABLE {target} (\n{}\n)", parts.join(",\n")));

            if !sequences.is_empty() {
                sequences.dedup();
                warnings.push(format!(
                    "Las columnas {} usan una secuencia o identidad del origen: quedan como columnas comunes con sus valores, y hay que crear la secuencia del destino y ajustarla.",
                    sequences.iter().map(|n| format!("«{n}»")).collect::<Vec<_>>().join(", ")
                ));
            }
        }
        TransferMode::Truncate => before.push(format!("TRUNCATE TABLE {target}")),
        TransferMode::Append => {}
    }

    if source.foreign_keys > 0 {
        warnings.push(format!(
            "{} clave(s) foránea(s) del origen no se copian: apuntan a otras tablas.",
            source.foreign_keys
        ));
    }
    if source.partitioned {
        warnings
            .push("Es una tabla particionada: en el destino queda como una tabla común.".into());
    }
    warnings.push("Los disparadores, los permisos y los comentarios no se copian.".into());

    let mut after = Vec::new();
    if spec.with_indexes && spec.mode == TransferMode::Create {
        for index in &source.indexes {
            match retarget_index(index, &target) {
                Some(sql) => after.push(sql),
                None => warnings.push(format!(
                    "No se pudo reescribir un índice, se omite: {}",
                    index.definition
                )),
            }
        }
    }

    let copyable: Vec<&SourceColumn> = source.columns.iter().filter(|c| !c.generated).collect();
    let (copy_out, copy_in) = if spec.with_data {
        if copyable.is_empty() {
            return Err(Error::Config(
                "la tabla no tiene columnas que copiar".into(),
            ));
        }
        let columns = copyable
            .iter()
            .map(|c| quote_ident(&c.name))
            .collect::<Vec<_>>()
            .join(", ");
        let predicate = filter.map(|f| format!(" WHERE {f}")).unwrap_or_default();
        (
            Some(format!(
                "COPY (SELECT {columns} FROM {from}{predicate}) TO STDOUT"
            )),
            Some(format!("COPY {target} ({columns}) FROM STDIN")),
        )
    } else {
        (None, None)
    };

    Ok(TransferPlan {
        before,
        copy_out,
        copy_in,
        after,
        warnings,
    })
}

/// Reescribe `CREATE [UNIQUE] INDEX nombre ON origen USING …` para que apunte al destino y **sin
/// nombre**, así PostgreSQL le pone uno libre. `None` si la definición no tiene la forma esperada.
fn retarget_index(index: &SourceIndex, target: &str) -> Option<String> {
    let rest = index.definition.strip_prefix(&index.prefix)?;
    let unique = if index.prefix.starts_with("CREATE UNIQUE") {
        "UNIQUE "
    } else {
        ""
    };
    // `prefix` termina en `ON origen `: lo que queda es `USING …`.
    Some(format!("CREATE {unique}INDEX ON {target} {rest}"))
}

const COLUMNS_SQL: &str = "
    SELECT a.attname::text,
           pg_catalog.format_type(a.atttypid, a.atttypmod),
           a.attnotnull,
           pg_catalog.pg_get_expr(d.adbin, d.adrelid),
           a.attidentity <> '',
           a.attgenerated <> ''
      FROM pg_catalog.pg_attribute a
      LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
     WHERE a.attrelid = $1 AND a.attnum > 0 AND NOT a.attisdropped
     ORDER BY a.attnum
";

/// Los índices que no sostienen una restricción —esos viajan con su `PRIMARY KEY`/`UNIQUE`— y que
/// sirven: uno inválido es el resto de un `CREATE INDEX CONCURRENTLY` fallido.
const INDEXES_SQL: &str = "
    SELECT pg_catalog.pg_get_indexdef(i.indexrelid),
           i.indisunique,
           pg_catalog.quote_ident(ic.relname::text),
           pg_catalog.quote_ident(n.nspname::text) || '.' || pg_catalog.quote_ident(c.relname::text)
      FROM pg_catalog.pg_index i
      JOIN pg_catalog.pg_class ic ON ic.oid = i.indexrelid
      JOIN pg_catalog.pg_class c ON c.oid = i.indrelid
      JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
     WHERE i.indrelid = $1
       AND i.indisvalid
       AND NOT EXISTS (
             SELECT 1 FROM pg_catalog.pg_constraint k
              WHERE k.conindid = i.indexrelid AND k.conrelid = i.indrelid
                AND k.contype IN ('p', 'u', 'x'))
     ORDER BY ic.relname
";

/// Lee del catálogo de origen lo que hace falta para armar el plan.
pub async fn read_source(
    handle: &ServerHandle,
    database: &str,
    schema: &str,
    table: &str,
) -> Result<SourceTable> {
    let oid = oid_of(handle, database, schema, table).await?;
    let client = handle.client(database).await?;

    let relkind: String = client
        .query_one(
            "SELECT relkind::text FROM pg_catalog.pg_class WHERE oid = $1",
            &[&oid],
        )
        .await?
        .get(0);
    if relkind != "r" && relkind != "p" {
        return Err(Error::Config(format!(
            "{schema}.{table} no es una tabla: solo se copian tablas"
        )));
    }

    let columns: Vec<SourceColumn> = client
        .query(COLUMNS_SQL, &[&oid])
        .await?
        .into_iter()
        .map(|row| SourceColumn {
            name: row.get(0),
            type_name: row.get(1),
            not_null: row.get(2),
            default: row.get(3),
            identity: row.get(4),
            generated: row.get(5),
        })
        .collect();

    let constraints = client
        .query(
            "SELECT pg_catalog.pg_get_constraintdef(oid)
               FROM pg_catalog.pg_constraint
              WHERE conrelid = $1 AND contype IN ('p', 'u', 'c', 'x')
              ORDER BY contype <> 'p', conname",
            &[&oid],
        )
        .await?
        .into_iter()
        .map(|row| row.get(0))
        .collect();

    let foreign_keys: i64 = client
        .query_one(
            "SELECT count(*) FROM pg_catalog.pg_constraint WHERE conrelid = $1 AND contype = 'f'",
            &[&oid],
        )
        .await?
        .get(0);

    let indexes = client
        .query(INDEXES_SQL, &[&oid])
        .await?
        .into_iter()
        .map(|row| {
            let unique: bool = row.get(1);
            let name: String = row.get(2);
            let table: String = row.get(3);
            SourceIndex {
                definition: row.get(0),
                prefix: format!(
                    "CREATE {}INDEX {name} ON {table} ",
                    if unique { "UNIQUE " } else { "" }
                ),
            }
        })
        .collect();

    Ok(SourceTable {
        columns,
        constraints,
        indexes,
        foreign_keys: foreign_keys as usize,
        partitioned: relkind == "p",
    })
}

/// Lee el origen y arma el plan: lo que muestra la vista previa.
pub async fn prepare(
    handle: &ServerHandle,
    database: &str,
    spec: &TransferSpec,
) -> Result<TransferPlan> {
    let source = read_source(handle, database, &spec.schema, &spec.table).await?;
    plan(spec, &source)
}

/// De dónde a dónde se copia: los dos servidores y la base de cada uno.
pub struct Route<'a> {
    pub source: &'a ServerHandle,
    pub source_database: &'a str,
    pub target: &'a ServerHandle,
    pub target_database: &'a str,
}

/// Ejecuta el plan: una transacción en el destino, y el `COPY` de un lado alimenta al del otro.
///
/// Todo lo del destino —crear, vaciar, cargar, los índices— va adentro de la misma transacción: si
/// algo falla, o se cancela, no queda una tabla a medio crear ni vaciada sin recargar. El origen
/// usa su propia sesión sin `statement_timeout`: una tabla grande tarda lo que tarda.
pub async fn run(
    route: Route<'_>,
    spec: &TransferSpec,
    plan: &TransferPlan,
    progress: mpsc::Sender<u64>,
    cancel: oneshot::Receiver<()>,
) -> Result<Outcome> {
    let Route {
        source,
        source_database,
        target,
        target_database,
    } = route;
    let destination = target.open_session(target_database, None).await?;
    let client = destination.client();
    let target_name = qualified(&spec.target_schema, &spec.target_table);

    // Comprobar antes de empezar: el error de «ya existe» o «no existe» es el que se espera ver, y no
    // lo que sale de un `CREATE TABLE` fallido o de un `COPY` contra la nada.
    let exists: bool = client
        .query_one("SELECT to_regclass($1) IS NOT NULL", &[&target_name])
        .await?
        .get(0);
    match (spec.mode, exists) {
        (TransferMode::Create, true) => {
            return Err(Error::Conflict(format!(
                "{target_name} ya existe en el destino; elegí agregar o reemplazar las filas"
            )))
        }
        (TransferMode::Append | TransferMode::Truncate, false) => {
            return Err(Error::Config(format!(
                "{target_name} no existe en el destino"
            )))
        }
        _ => {}
    }

    client.batch_execute("BEGIN").await?;
    let outcome = move_rows(
        source,
        source_database,
        &destination,
        plan,
        progress,
        cancel,
    )
    .await;

    match outcome {
        Ok(outcome) => {
            client.batch_execute("COMMIT").await?;
            Ok(outcome)
        }
        Err(error) => {
            let _ = client.batch_execute("ROLLBACK").await;
            Err(error)
        }
    }
}

async fn move_rows(
    source: &ServerHandle,
    source_database: &str,
    destination: &crate::conn::Session,
    plan: &TransferPlan,
    progress: mpsc::Sender<u64>,
    cancel: oneshot::Receiver<()>,
) -> Result<Outcome> {
    let client = destination.client();
    for sql in &plan.before {
        client.batch_execute(sql).await?;
    }

    let mut bytes: u64 = 0;
    let mut rows: Option<u64> = None;

    if let (Some(out), Some(into)) = (&plan.copy_out, &plan.copy_in) {
        let origin = source.open_session(source_database, None).await?;
        let token = origin.cancel_token();
        let stream = origin.client().copy_out(out.as_str()).await?;
        tokio::pin!(stream);

        let sink: CopyInSink<Bytes> = client.copy_in(into.as_str()).await?;
        tokio::pin!(sink);

        let mut cancel = cancel;
        let mut since_report: u64 = 0;

        let result: Result<()> = loop {
            tokio::select! {
                biased;
                _ = &mut cancel => break Err(Error::Canceled),
                chunk = stream.try_next() => match chunk {
                    Ok(Some(data)) => {
                        let len = data.len() as u64;
                        if let Err(e) = sink.send(data).await {
                            break Err(Error::from(e));
                        }
                        bytes += len;
                        since_report += len;
                        if since_report >= REPORT_EVERY {
                            since_report = 0;
                            let _ = progress.send(bytes).await;
                        }
                    }
                    Ok(None) => break Ok(()),
                    Err(e) => break Err(Error::from(e)),
                }
            }
        };

        match result {
            Ok(()) => rows = Some(sink.finish().await?),
            Err(error) => {
                // El `COPY TO` del origen sigue vivo hasta que se le avisa.
                if matches!(error, Error::Canceled) {
                    let _ = source.cancel(&token).await;
                }
                return Err(error);
            }
        }
        let _ = progress.send(bytes).await;
    }

    for sql in &plan.after {
        client.batch_execute(sql).await?;
    }

    Ok(Outcome { bytes, rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str, type_name: &str) -> SourceColumn {
        SourceColumn {
            name: name.into(),
            type_name: type_name.into(),
            not_null: false,
            default: None,
            identity: false,
            generated: false,
        }
    }

    fn source() -> SourceTable {
        SourceTable {
            columns: vec![
                SourceColumn {
                    not_null: true,
                    ..column("id", "bigint")
                },
                column("Nombre", "text"),
                SourceColumn {
                    default: Some("now()".into()),
                    ..column("creado", "timestamp with time zone")
                },
            ],
            constraints: vec!["PRIMARY KEY (id)".into(), "CHECK ((id > 0))".into()],
            indexes: vec![SourceIndex {
                definition:
                    "CREATE INDEX clientes_nombre_idx ON public.clientes USING btree (\"Nombre\")"
                        .into(),
                prefix: "CREATE INDEX clientes_nombre_idx ON public.clientes ".into(),
            }],
            foreign_keys: 0,
            partitioned: false,
        }
    }

    fn spec(mode: TransferMode) -> TransferSpec {
        TransferSpec {
            schema: "public".into(),
            table: "clientes".into(),
            target_schema: "copia".into(),
            target_table: "clientes".into(),
            mode,
            with_data: true,
            with_indexes: false,
            filter: None,
        }
    }

    #[test]
    fn crea_la_tabla_con_tipos_claves_y_el_copy_en_las_dos_puntas() {
        let plan = plan(&spec(TransferMode::Create), &source()).unwrap();
        assert_eq!(plan.before.len(), 1);
        let create = &plan.before[0];
        assert!(create.starts_with("CREATE TABLE copia.clientes ("));
        assert!(create.contains("id bigint NOT NULL"));
        assert!(create.contains("\"Nombre\" text"));
        assert!(create.contains("creado timestamp with time zone DEFAULT now()"));
        assert!(create.contains("PRIMARY KEY (id)"));
        assert_eq!(
            plan.copy_out.as_deref(),
            Some("COPY (SELECT id, \"Nombre\", creado FROM public.clientes) TO STDOUT")
        );
        assert_eq!(
            plan.copy_in.as_deref(),
            Some("COPY copia.clientes (id, \"Nombre\", creado) FROM STDIN")
        );
    }

    #[test]
    fn una_secuencia_del_origen_no_se_copia_y_se_avisa() {
        let mut table = source();
        table.columns[0].default = Some("nextval('clientes_id_seq'::regclass)".into());
        let plan = plan(&spec(TransferMode::Create), &table).unwrap();
        assert!(!plan.before[0].contains("nextval"));
        assert!(plan.warnings.iter().any(|w| w.contains("«id»")));
    }

    #[test]
    fn una_columna_generada_se_recrea_pero_no_se_copia() {
        let mut table = source();
        table.columns.push(SourceColumn {
            default: Some("(id * 2)".into()),
            generated: true,
            ..column("doble", "bigint")
        });
        let plan = plan(&spec(TransferMode::Create), &table).unwrap();
        assert!(plan.before[0].contains("doble bigint GENERATED ALWAYS AS ((id * 2)) STORED"));
        assert!(!plan.copy_in.unwrap().contains("doble"));
    }

    #[test]
    fn los_indices_se_reescriben_sin_nombre_y_hacia_el_destino() {
        let mut s = spec(TransferMode::Create);
        s.with_indexes = true;
        let plan = plan(&s, &source()).unwrap();
        assert_eq!(
            plan.after,
            vec!["CREATE INDEX ON copia.clientes USING btree (\"Nombre\")".to_owned()]
        );
    }

    #[test]
    fn un_indice_unico_conserva_el_unique() {
        let index = SourceIndex {
            definition: "CREATE UNIQUE INDEX u ON public.clientes USING btree (email)".into(),
            prefix: "CREATE UNIQUE INDEX u ON public.clientes ".into(),
        };
        assert_eq!(
            retarget_index(&index, "copia.clientes").unwrap(),
            "CREATE UNIQUE INDEX ON copia.clientes USING btree (email)"
        );
    }

    #[test]
    fn solo_estructura_no_trae_copy() {
        let mut s = spec(TransferMode::Create);
        s.with_data = false;
        let plan = plan(&s, &source()).unwrap();
        assert!(plan.copy_out.is_none() && plan.copy_in.is_none());
    }

    #[test]
    fn el_filtro_va_en_el_select_y_no_puede_encadenar_sentencias() {
        let mut s = spec(TransferMode::Append);
        s.filter = Some("estado = 'activo'".into());
        let plan = plan(&s, &source()).unwrap();
        assert!(plan
            .copy_out
            .unwrap()
            .ends_with("FROM public.clientes WHERE estado = 'activo') TO STDOUT"));

        s.filter = Some("true) TO STDOUT; DROP TABLE x; --".into());
        assert!(super::plan(&s, &source()).is_err());
    }

    #[test]
    fn reemplazar_vacia_en_la_misma_transaccion_y_agregar_no_toca_nada() {
        let truncate = plan(&spec(TransferMode::Truncate), &source()).unwrap();
        assert_eq!(
            truncate.before,
            vec!["TRUNCATE TABLE copia.clientes".to_owned()]
        );
        let append = plan(&spec(TransferMode::Append), &source()).unwrap();
        assert!(append.before.is_empty());
    }

    #[test]
    fn las_claves_foraneas_se_avisan() {
        let mut table = source();
        table.foreign_keys = 2;
        let plan = plan(&spec(TransferMode::Create), &table).unwrap();
        assert!(plan
            .warnings
            .iter()
            .any(|w| w.contains("2 clave(s) foránea(s)")));
    }

    #[test]
    fn sin_destino_no_se_arma() {
        let mut s = spec(TransferMode::Create);
        s.target_table = " ".into();
        assert!(plan(&s, &source()).is_err());
    }
}
