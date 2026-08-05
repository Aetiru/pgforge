//! Exploración del esquema.
//!
//! Todas las consultas van contra `pg_catalog` y no contra `information_schema`: el catálogo es
//! más rápido, cubre objetos que la vista estándar no conoce (vistas materializadas, tipos
//! compuestos, particionamiento) y expone los OID que hacen falta para pedir el DDL después.
//!
//! El árbol se arma por niveles. Un servidor con miles de tablas no puede resolverse de una vez,
//! así que cada expansión es una consulta acotada al nodo que se abrió.

mod graph;
mod node;

pub use graph::{schema_graph, GraphColumn, GraphEdge, GraphTable, SchemaGraph};
pub use node::{Folder, NodeKind, NodeTag, TreeNode};

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

/// Preferencias de visualización del árbol.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeOptions {
    /// Mostrar `pg_catalog`, `information_schema` y los esquemas temporales.
    pub show_system_schemas: bool,
}

/// Devuelve los hijos de un nodo. Con `None` devuelve el primer nivel: las bases del servidor.
pub async fn children(
    handle: &ServerHandle,
    parent: Option<&TreeNode>,
    options: TreeOptions,
) -> Result<Vec<TreeNode>> {
    let Some(parent) = parent else {
        // "Roles" es del clúster entero, así que va como hermana de las bases en la raíz y no
        // colgando de ninguna de ellas.
        let mut nodes = vec![roles_folder(handle, options).await?];
        nodes.extend(databases(handle).await?);
        return Ok(nodes);
    };

    match parent.kind {
        NodeKind::Database => Ok(vec![
            schemas_folder(handle, parent, options).await?,
            extensions_folder(handle, parent).await?,
            fdws_folder(handle, parent).await?,
            foreign_servers_folder(handle, parent).await?,
        ]),
        NodeKind::Folder(Folder::Schemas) => schemas(handle, parent, options).await,
        NodeKind::Folder(Folder::Roles) => roles(handle, parent, options).await,
        NodeKind::Schema => schema_folders(handle, parent).await,
        NodeKind::Folder(folder) => objects(handle, parent, folder).await,
        NodeKind::Table
        | NodeKind::PartitionedTable
        | NodeKind::ForeignTable
        | NodeKind::View
        | NodeKind::MaterializedView => relation_folders(handle, parent).await,
        _ => Ok(Vec::new()),
    }
}

/// El dueño de un objeto, salvo que sea el usuario conectado.
///
/// Casi todo pertenece a quien está mirando: repetir su nombre en cada fila gasta el ancho del
/// árbol para no decir nada. Se muestra cuando es otro, que es justo cuando importa.
fn owner_detail(handle: &ServerHandle, owner: String) -> Option<String> {
    (owner != handle.caps.current_user).then_some(owner)
}

async fn databases(handle: &ServerHandle) -> Result<Vec<TreeNode>> {
    let databases = handle.databases().await?;
    Ok(databases
        .into_iter()
        .filter(|db| db.can_connect && !db.is_template)
        .map(|db| {
            let mut node = TreeNode::database(db.name);
            node.detail = owner_detail(handle, db.owner);
            node
        })
        .collect())
}

/// La carpeta "Roles", hermana de las bases en la raíz. Cualquier base sirve para leerla: los
/// roles son del clúster, no de una base en particular.
async fn roles_folder(handle: &ServerHandle, options: TreeOptions) -> Result<TreeNode> {
    let database = handle.default_database();
    let client = handle.client(database).await?;
    let row = client
        .query_one(
            "SELECT count(*)::int8 FROM pg_catalog.pg_roles r
              WHERE $1 OR r.rolname NOT LIKE 'pg\\_%'",
            &[&options.show_system_schemas],
        )
        .await?;
    Ok(TreeNode::root_folder(database, Folder::Roles, row.get(0)))
}

/// Los roles internos de Postgres (`pg_signal_backend` y el resto de los predefinidos) se ocultan
/// con el mismo interruptor que ya oculta `pg_catalog`/`information_schema`, en vez de agregar una
/// opción nueva.
async fn roles(
    handle: &ServerHandle,
    parent: &TreeNode,
    options: TreeOptions,
) -> Result<Vec<TreeNode>> {
    let client = handle.client(&parent.database).await?;
    let rows = client
        .query(
            "SELECT r.oid, r.rolname::text, r.rolcanlogin, r.rolsuper
               FROM pg_catalog.pg_roles r
              WHERE $1 OR r.rolname NOT LIKE 'pg\\_%'
              ORDER BY r.rolname",
            &[&options.show_system_schemas],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let can_login: bool = row.get(2);
            let is_super: bool = row.get(3);

            // Poder entrar o no es la distinción que separa a un usuario de un rol de agrupación, y
            // es lo primero que se busca al mirar la lista: va como etiqueta y no como texto.
            let mut tags = vec![if can_login {
                NodeTag::Login
            } else {
                NodeTag::Group
            }];
            if is_super {
                tags.push(NodeTag::Superuser);
            }

            TreeNode::object(parent, NodeKind::Role, row.get(0), row.get(1), None, false)
                .with_tags(tags)
        })
        .collect())
}

/// Nivel intermedio "Esquemas" bajo cada base. Existe para dejar lugar a las otras categorías de
/// objetos de nivel de base (extensiones, tablespaces, publicaciones) sin reordenar el árbol.
async fn schemas_folder(
    handle: &ServerHandle,
    parent: &TreeNode,
    options: TreeOptions,
) -> Result<TreeNode> {
    let client = handle.client(&parent.database).await?;
    let row = client
        .query_one(
            "SELECT count(*)::int8
               FROM pg_catalog.pg_namespace n
              WHERE $1 OR NOT (n.nspname IN ('pg_catalog', 'information_schema')
                               OR n.nspname LIKE 'pg\\_toast%'
                               OR n.nspname LIKE 'pg\\_temp%')",
            &[&options.show_system_schemas],
        )
        .await?;
    Ok(TreeNode::folder(parent, Folder::Schemas, row.get(0)))
}

/// La carpeta "Extensiones", hermana de "Esquemas" bajo cada base. Las extensiones son de la base
/// —cada base tiene las suyas—, a diferencia de los roles, que son del clúster.
async fn extensions_folder(handle: &ServerHandle, parent: &TreeNode) -> Result<TreeNode> {
    let client = handle.client(&parent.database).await?;
    let row = client
        .query_one("SELECT count(*)::int8 FROM pg_catalog.pg_extension", &[])
        .await?;
    Ok(TreeNode::folder(parent, Folder::Extensions, row.get(0)))
}

async fn extensions(handle: &ServerHandle, parent: &TreeNode) -> Result<Vec<TreeNode>> {
    let client = handle.client(&parent.database).await?;
    let rows = client
        .query(
            "SELECT e.oid,
                    e.extname::text,
                    e.extversion,
                    n.nspname::text,
                    pg_catalog.obj_description(e.oid, 'pg_extension')
               FROM pg_catalog.pg_extension e
               JOIN pg_catalog.pg_namespace n ON n.oid = e.extnamespace
              ORDER BY e.extname",
            &[],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let version: String = row.get(2);
            let schema: String = row.get(3);
            TreeNode::object(
                parent,
                NodeKind::Extension,
                row.get(0),
                row.get(1),
                Some(format!("v{version} · {schema}")),
                false,
            )
            .with_comment(row.get(4))
        })
        .collect())
}

/// La carpeta "Wrappers foráneos", hermana de "Esquemas" bajo cada base.
async fn fdws_folder(handle: &ServerHandle, parent: &TreeNode) -> Result<TreeNode> {
    let client = handle.client(&parent.database).await?;
    let row = client
        .query_one(
            "SELECT count(*)::int8 FROM pg_catalog.pg_foreign_data_wrapper",
            &[],
        )
        .await?;
    Ok(TreeNode::folder(
        parent,
        Folder::ForeignDataWrappers,
        row.get(0),
    ))
}

/// La carpeta "Servidores foráneos", hermana de "Esquemas" bajo cada base.
async fn foreign_servers_folder(handle: &ServerHandle, parent: &TreeNode) -> Result<TreeNode> {
    let client = handle.client(&parent.database).await?;
    let row = client
        .query_one(
            "SELECT count(*)::int8 FROM pg_catalog.pg_foreign_server",
            &[],
        )
        .await?;
    Ok(TreeNode::folder(parent, Folder::ForeignServers, row.get(0)))
}

async fn fdws(handle: &ServerHandle, parent: &TreeNode) -> Result<Vec<TreeNode>> {
    let client = handle.client(&parent.database).await?;
    let rows = client
        .query(
            "SELECT w.oid,
                    w.fdwname::text,
                    h.proname::text,
                    pg_catalog.pg_get_userbyid(w.fdwowner)::text
               FROM pg_catalog.pg_foreign_data_wrapper w
               LEFT JOIN pg_catalog.pg_proc h ON h.oid = w.fdwhandler
              ORDER BY w.fdwname",
            &[],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let handler: Option<String> = row.get(2);
            let owner: String = row.get(3);
            // Un wrapper sin handler no puede leer nada: es solo una cáscara. Decirlo evita el
            // «no anda» que si no aparece recién al crearle un servidor.
            let mut detail = handler.unwrap_or_else(|| "sin handler".to_owned());
            if let Some(owner) = owner_detail(handle, owner) {
                detail.push_str(&format!(" · {owner}"));
            }
            TreeNode::object(
                parent,
                NodeKind::ForeignDataWrapper,
                row.get(0),
                row.get(1),
                Some(detail),
                false,
            )
        })
        .collect())
}

async fn foreign_servers(handle: &ServerHandle, parent: &TreeNode) -> Result<Vec<TreeNode>> {
    let client = handle.client(&parent.database).await?;
    let rows = client
        .query(
            "SELECT s.oid,
                    s.srvname::text,
                    w.fdwname::text,
                    s.srvtype,
                    s.srvversion
               FROM pg_catalog.pg_foreign_server s
               JOIN pg_catalog.pg_foreign_data_wrapper w ON w.oid = s.srvfdw
              ORDER BY s.srvname",
            &[],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let fdw: String = row.get(2);
            let server_type: Option<String> = row.get(3);
            let version: Option<String> = row.get(4);

            let mut detail = fdw;
            if let Some(server_type) = server_type {
                detail.push_str(&format!(" · {server_type}"));
            }
            if let Some(version) = version {
                detail.push_str(&format!(" · v{version}"));
            }

            TreeNode::object(
                parent,
                NodeKind::ForeignServer,
                row.get(0),
                row.get(1),
                Some(detail),
                false,
            )
        })
        .collect())
}

async fn schemas(
    handle: &ServerHandle,
    parent: &TreeNode,
    options: TreeOptions,
) -> Result<Vec<TreeNode>> {
    let client = handle.client(&parent.database).await?;
    let rows = client
        .query(
            "SELECT n.oid,
                    n.nspname::text,
                    pg_catalog.pg_get_userbyid(n.nspowner)::text,
                    pg_catalog.obj_description(n.oid, 'pg_namespace')
               FROM pg_catalog.pg_namespace n
              WHERE $1 OR NOT (n.nspname IN ('pg_catalog', 'information_schema')
                               OR n.nspname LIKE 'pg\\_toast%'
                               OR n.nspname LIKE 'pg\\_temp%')
              ORDER BY n.nspname",
            &[&options.show_system_schemas],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            TreeNode::schema(
                parent,
                row.get(1),
                row.get(0),
                owner_detail(handle, row.get(2)),
            )
            .with_comment(row.get(3))
        })
        .collect())
}

/// Una sola consulta trae la cantidad de objetos de cada categoría del esquema. Contar por
/// separado serían ocho viajes al servidor cada vez que alguien abre un esquema.
const SCHEMA_COUNTS_SQL: &str = "
    SELECT 'tables'::text,    count(*)::int8 FROM pg_catalog.pg_class WHERE relnamespace = $1 AND relkind IN ('r', 'p')
    UNION ALL
    SELECT 'views',           count(*)       FROM pg_catalog.pg_class WHERE relnamespace = $1 AND relkind = 'v'
    UNION ALL
    SELECT 'matviews',        count(*)       FROM pg_catalog.pg_class WHERE relnamespace = $1 AND relkind = 'm'
    UNION ALL
    SELECT 'foreign',         count(*)       FROM pg_catalog.pg_class WHERE relnamespace = $1 AND relkind = 'f'
    UNION ALL
    SELECT 'sequences',       count(*)       FROM pg_catalog.pg_class WHERE relnamespace = $1 AND relkind = 'S'
    UNION ALL
    SELECT 'functions',       count(*)       FROM pg_catalog.pg_proc  WHERE pronamespace = $1 AND prokind IN ('f', 'w')
    UNION ALL
    SELECT 'procedures',      count(*)       FROM pg_catalog.pg_proc  WHERE pronamespace = $1 AND prokind = 'p'
    UNION ALL
    SELECT 'types',           count(*)       FROM pg_catalog.pg_type t
     WHERE t.typnamespace = $1
       AND (t.typrelid = 0
            OR (SELECT c.relkind FROM pg_catalog.pg_class c WHERE c.oid = t.typrelid) = 'c')
       AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_type el
                        WHERE el.oid = t.typelem AND el.typarray = t.oid)
";

async fn schema_folders(handle: &ServerHandle, parent: &TreeNode) -> Result<Vec<TreeNode>> {
    let oid = parent.oid.ok_or_else(|| missing_oid("esquema"))?;
    let client = handle.client(&parent.database).await?;
    let rows = client.query(SCHEMA_COUNTS_SQL, &[&oid]).await?;

    let count = |key: &str| -> i64 {
        rows.iter()
            .find(|row| row.get::<_, &str>(0) == key)
            .map(|row| row.get::<_, i64>(1))
            .unwrap_or(0)
    };

    Ok(vec![
        TreeNode::folder(parent, Folder::Tables, count("tables")),
        TreeNode::folder(parent, Folder::Views, count("views")),
        TreeNode::folder(parent, Folder::MaterializedViews, count("matviews")),
        TreeNode::folder(parent, Folder::ForeignTables, count("foreign")),
        TreeNode::folder(parent, Folder::Sequences, count("sequences")),
        TreeNode::folder(parent, Folder::Functions, count("functions")),
        TreeNode::folder(parent, Folder::Procedures, count("procedures")),
        TreeNode::folder(parent, Folder::Types, count("types")),
    ])
}

async fn objects(
    handle: &ServerHandle,
    parent: &TreeNode,
    folder: Folder,
) -> Result<Vec<TreeNode>> {
    match folder {
        Folder::Tables
        | Folder::Views
        | Folder::MaterializedViews
        | Folder::ForeignTables
        | Folder::Sequences => relations(handle, parent, folder).await,
        Folder::Functions | Folder::Procedures => routines(handle, parent, folder).await,
        Folder::Types => types(handle, parent).await,
        Folder::Columns => columns(handle, parent).await,
        Folder::Indexes => indexes(handle, parent).await,
        Folder::Constraints => constraints(handle, parent).await,
        Folder::Triggers => triggers(handle, parent).await,
        Folder::Policies => policies(handle, parent).await,
        Folder::Extensions => extensions(handle, parent).await,
        Folder::ForeignDataWrappers => fdws(handle, parent).await,
        Folder::ForeignServers => foreign_servers(handle, parent).await,
        // Se resuelven antes, en el `match` de `children()`: `Schemas` no tiene un padre de este
        // tipo, y `Roles` va por su propia función porque no cuelga de una base.
        Folder::Schemas | Folder::Roles => Ok(Vec::new()),
    }
}

async fn relations(
    handle: &ServerHandle,
    parent: &TreeNode,
    folder: Folder,
) -> Result<Vec<TreeNode>> {
    // El filtro es una constante elegida por la carpeta, no un valor de entrada.
    let relkinds = match folder {
        Folder::Tables => "'r', 'p'",
        Folder::Views => "'v'",
        Folder::MaterializedViews => "'m'",
        Folder::ForeignTables => "'f'",
        Folder::Sequences => "'S'",
        _ => return Ok(Vec::new()),
    };

    let sql = format!(
        "SELECT c.oid,
                c.relname::text,
                c.relkind::text,
                pg_catalog.pg_get_userbyid(c.relowner)::text,
                pg_catalog.obj_description(c.oid, 'pg_class'),
                c.relispartition,
                c.relrowsecurity
           FROM pg_catalog.pg_class c
          WHERE c.relnamespace = $1 AND c.relkind IN ({relkinds})
          ORDER BY c.relname"
    );

    let oid = parent.oid.ok_or_else(|| missing_oid("esquema"))?;
    let client = handle.client(&parent.database).await?;
    let rows = client.query(sql.as_str(), &[&oid]).await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let relkind: String = row.get(2);
            let owner: String = row.get(3);
            let is_partition: bool = row.get(5);
            let has_rls: bool = row.get(6);

            let kind = match relkind.as_str() {
                "r" => NodeKind::Table,
                "p" => NodeKind::PartitionedTable,
                "f" => NodeKind::ForeignTable,
                "v" => NodeKind::View,
                "m" => NodeKind::MaterializedView,
                _ => NodeKind::Sequence,
            };

            let detail = owner_detail(handle, owner);

            let mut tags = Vec::new();
            if is_partition {
                tags.push(NodeTag::Partition);
            }
            // Una tabla con RLS activa devuelve menos filas de las que tiene, y eso explica de
            // antemano el «no veo mis datos» que si no se descubre recién al consultarla.
            if has_rls {
                tags.push(NodeTag::RowSecurity);
            }

            let expandable = !matches!(kind, NodeKind::Sequence);
            TreeNode::object(parent, kind, row.get(0), row.get(1), detail, expandable)
                .with_comment(row.get(4))
                .with_tags(tags)
        })
        .collect())
}

async fn routines(
    handle: &ServerHandle,
    parent: &TreeNode,
    folder: Folder,
) -> Result<Vec<TreeNode>> {
    let prokinds = match folder {
        Folder::Functions => "'f', 'w'",
        Folder::Procedures => "'p'",
        _ => return Ok(Vec::new()),
    };

    let sql = format!(
        "SELECT p.oid,
                p.proname::text,
                pg_catalog.pg_get_function_identity_arguments(p.oid),
                pg_catalog.pg_get_function_result(p.oid),
                pg_catalog.obj_description(p.oid, 'pg_proc'),
                l.lanname::text
           FROM pg_catalog.pg_proc p
           JOIN pg_catalog.pg_language l ON l.oid = p.prolang
          WHERE p.pronamespace = $1 AND p.prokind IN ({prokinds})
          ORDER BY p.proname, p.oid"
    );

    let oid = parent.oid.ok_or_else(|| missing_oid("esquema"))?;
    let client = handle.client(&parent.database).await?;
    let rows = client.query(sql.as_str(), &[&oid]).await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let name: String = row.get(1);
            let args: String = row.get(2);
            let result: Option<String> = row.get(3);
            let language: String = row.get(5);

            // La firma va en la etiqueta porque el nombre solo no distingue las sobrecargas.
            let label = format!("{name}({args})");
            let detail = match result {
                Some(result) => Some(format!("{result} · {language}")),
                None => Some(language),
            };

            let kind = if folder == Folder::Procedures {
                NodeKind::Procedure
            } else {
                NodeKind::Function
            };

            TreeNode::object(parent, kind, row.get(0), label, detail, false)
                .with_comment(row.get(4))
        })
        .collect())
}

async fn types(handle: &ServerHandle, parent: &TreeNode) -> Result<Vec<TreeNode>> {
    let oid = parent.oid.ok_or_else(|| missing_oid("esquema"))?;
    let client = handle.client(&parent.database).await?;
    let rows = client
        .query(
            "SELECT t.oid,
                    t.typname::text,
                    t.typtype::text,
                    pg_catalog.obj_description(t.oid, 'pg_type')
               FROM pg_catalog.pg_type t
              WHERE t.typnamespace = $1
                AND (t.typrelid = 0
                     OR (SELECT c.relkind FROM pg_catalog.pg_class c WHERE c.oid = t.typrelid) = 'c')
                AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_type el
                                 WHERE el.oid = t.typelem AND el.typarray = t.oid)
              ORDER BY t.typname",
            &[&oid],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let typtype: String = row.get(2);
            let detail = match typtype.as_str() {
                "e" => "enumerado",
                "c" => "compuesto",
                "d" => "dominio",
                "r" => "rango",
                "m" => "multirango",
                "b" => "base",
                _ => "tipo",
            };
            TreeNode::object(
                parent,
                NodeKind::Type,
                row.get(0),
                row.get(1),
                Some(detail.to_owned()),
                false,
            )
            .with_comment(row.get(3))
        })
        .collect())
}

const RELATION_COUNTS_SQL: &str = "
    SELECT 'columns'::text, count(*)::int8 FROM pg_catalog.pg_attribute
     WHERE attrelid = $1 AND attnum > 0 AND NOT attisdropped
    UNION ALL
    SELECT 'indexes',     count(*) FROM pg_catalog.pg_index      WHERE indrelid = $1
    UNION ALL
    SELECT 'constraints', count(*) FROM pg_catalog.pg_constraint WHERE conrelid = $1
    UNION ALL
    SELECT 'triggers',    count(*) FROM pg_catalog.pg_trigger    WHERE tgrelid = $1 AND NOT tgisinternal
    UNION ALL
    SELECT 'policies',    count(*) FROM pg_catalog.pg_policy     WHERE polrelid = $1
";

async fn relation_folders(handle: &ServerHandle, parent: &TreeNode) -> Result<Vec<TreeNode>> {
    let oid = parent.oid.ok_or_else(|| missing_oid("relación"))?;
    let client = handle.client(&parent.database).await?;
    let rows = client.query(RELATION_COUNTS_SQL, &[&oid]).await?;

    let count = |key: &str| -> i64 {
        rows.iter()
            .find(|row| row.get::<_, &str>(0) == key)
            .map(|row| row.get::<_, i64>(1))
            .unwrap_or(0)
    };

    let mut folders = vec![TreeNode::folder(parent, Folder::Columns, count("columns"))];

    // Una vista no tiene índices ni disparadores propios; una materializada sí puede tener índices.
    match parent.kind {
        NodeKind::View => {}
        NodeKind::MaterializedView => {
            folders.push(TreeNode::folder(parent, Folder::Indexes, count("indexes")));
        }
        _ => {
            folders.push(TreeNode::folder(parent, Folder::Indexes, count("indexes")));
            folders.push(TreeNode::folder(
                parent,
                Folder::Constraints,
                count("constraints"),
            ));
            folders.push(TreeNode::folder(
                parent,
                Folder::Triggers,
                count("triggers"),
            ));
            folders.push(TreeNode::folder(
                parent,
                Folder::Policies,
                count("policies"),
            ));
        }
    }

    Ok(folders)
}

async fn columns(handle: &ServerHandle, parent: &TreeNode) -> Result<Vec<TreeNode>> {
    let oid = parent.oid.ok_or_else(|| missing_oid("relación"))?;
    let client = handle.client(&parent.database).await?;
    let rows = client
        .query(
            "SELECT a.attnum,
                    a.attname::text,
                    pg_catalog.format_type(a.atttypid, a.atttypmod),
                    a.attnotnull,
                    pg_catalog.pg_get_expr(d.adbin, d.adrelid),
                    pg_catalog.col_description(a.attrelid, a.attnum),
                    a.attidentity::text,
                    a.attgenerated::text
               FROM pg_catalog.pg_attribute a
               LEFT JOIN pg_catalog.pg_attrdef d
                      ON d.adrelid = a.attrelid AND d.adnum = a.attnum
              WHERE a.attrelid = $1 AND a.attnum > 0 AND NOT a.attisdropped
              ORDER BY a.attnum",
            &[&oid],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let attnum: i16 = row.get(0);
            let type_name: String = row.get(2);
            let not_null: bool = row.get(3);
            let default: Option<String> = row.get(4);
            let identity: String = row.get(6);
            let generated: String = row.get(7);

            let mut detail = type_name;
            if not_null {
                detail.push_str(" not null");
            }
            if !identity.is_empty() {
                detail.push_str(" identity");
            } else if !generated.is_empty() {
                detail.push_str(" generated");
            } else if let Some(default) = default {
                detail.push_str(&format!(" = {default}"));
            }

            TreeNode::leaf(parent, NodeKind::Column, attnum, row.get(1), Some(detail))
                .with_comment(row.get(5))
        })
        .collect())
}

async fn indexes(handle: &ServerHandle, parent: &TreeNode) -> Result<Vec<TreeNode>> {
    let oid = parent.oid.ok_or_else(|| missing_oid("relación"))?;
    let client = handle.client(&parent.database).await?;
    let rows = client
        .query(
            "SELECT c.oid,
                    c.relname::text,
                    i.indisprimary,
                    i.indisunique,
                    i.indisvalid,
                    am.amname::text
               FROM pg_catalog.pg_index i
               JOIN pg_catalog.pg_class c ON c.oid = i.indexrelid
               JOIN pg_catalog.pg_am am ON am.oid = c.relam
              WHERE i.indrelid = $1
              ORDER BY c.relname",
            &[&oid],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let is_primary: bool = row.get(2);
            let is_unique: bool = row.get(3);
            let is_valid: bool = row.get(4);
            let method: String = row.get(5);

            let mut detail = method;
            if is_primary {
                detail.push_str(" · primaria");
            } else if is_unique {
                detail.push_str(" · única");
            }
            // Un índice inválido quedó de un CREATE INDEX CONCURRENTLY que falló: no lo usa el
            // planificador y hay que rehacerlo. Es lo primero que hay que ver del índice.
            if !is_valid {
                detail.push_str(" · INVÁLIDO");
            }

            TreeNode::object(
                parent,
                NodeKind::Index,
                row.get(0),
                row.get(1),
                Some(detail),
                false,
            )
        })
        .collect())
}

async fn constraints(handle: &ServerHandle, parent: &TreeNode) -> Result<Vec<TreeNode>> {
    let oid = parent.oid.ok_or_else(|| missing_oid("relación"))?;
    let client = handle.client(&parent.database).await?;
    let rows = client
        .query(
            "SELECT con.oid,
                    con.conname::text,
                    con.contype::text,
                    pg_catalog.pg_get_constraintdef(con.oid, true)
               FROM pg_catalog.pg_constraint con
              WHERE con.conrelid = $1
              ORDER BY con.conname",
            &[&oid],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let contype: String = row.get(2);
            let kind = match contype.as_str() {
                "p" => "primaria",
                "f" => "foránea",
                "u" => "única",
                "c" => "verificación",
                "x" => "exclusión",
                "t" => "disparador",
                _ => "restricción",
            };
            TreeNode::object(
                parent,
                NodeKind::Constraint,
                row.get(0),
                row.get(1),
                Some(format!("{kind} · {}", row.get::<_, String>(3))),
                false,
            )
        })
        .collect())
}

async fn triggers(handle: &ServerHandle, parent: &TreeNode) -> Result<Vec<TreeNode>> {
    let oid = parent.oid.ok_or_else(|| missing_oid("relación"))?;
    let client = handle.client(&parent.database).await?;
    let rows = client
        .query(
            "SELECT t.oid,
                    t.tgname::text,
                    t.tgenabled::text
               FROM pg_catalog.pg_trigger t
              WHERE t.tgrelid = $1 AND NOT t.tgisinternal
              ORDER BY t.tgname",
            &[&oid],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let enabled: String = row.get(2);
            let detail = match enabled.as_str() {
                "D" => "deshabilitado",
                "R" => "replica",
                "A" => "siempre",
                _ => "habilitado",
            };
            TreeNode::object(
                parent,
                NodeKind::Trigger,
                row.get(0),
                row.get(1),
                Some(detail.to_owned()),
                false,
            )
        })
        .collect())
}

/// Las políticas de Row-Level Security de una tabla.
///
/// El detalle incluye si el filtro de la tabla está apagado, porque una política sobre una tabla
/// sin `ROW LEVEL SECURITY` no hace absolutamente nada y desde el árbol no hay otra forma de
/// enterarse.
async fn policies(handle: &ServerHandle, parent: &TreeNode) -> Result<Vec<TreeNode>> {
    let oid = parent.oid.ok_or_else(|| missing_oid("relación"))?;
    let client = handle.client(&parent.database).await?;
    let rows = client
        .query(
            "SELECT p.oid,
                    p.polname::text,
                    p.polcmd::text,
                    p.polpermissive,
                    c.relrowsecurity
               FROM pg_catalog.pg_policy p
               JOIN pg_catalog.pg_class c ON c.oid = p.polrelid
              WHERE p.polrelid = $1
              ORDER BY p.polname",
            &[&oid],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let polcmd: String = row.get(2);
            let command = match polcmd.as_str() {
                "r" => "SELECT",
                "a" => "INSERT",
                "w" => "UPDATE",
                "d" => "DELETE",
                _ => "ALL",
            };
            let permissive: bool = row.get(3);
            let enabled: bool = row.get(4);

            let mut detail = command.to_owned();
            if !permissive {
                detail.push_str(" · restrictiva");
            }
            if !enabled {
                detail.push_str(" · el filtro de la tabla está apagado");
            }

            TreeNode::object(
                parent,
                NodeKind::Policy,
                row.get(0),
                row.get(1),
                Some(detail),
                false,
            )
        })
        .collect())
}

fn missing_oid(what: &str) -> Error {
    Error::Config(format!("el nodo de {what} no tiene OID asociado"))
}
