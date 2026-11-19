//! Nodos del árbol de objetos.

use serde::{Deserialize, Serialize};

/// Tipo de nodo. Determina qué consulta hay que hacer para traer sus hijos y qué ícono le
/// corresponde en la interfaz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeKind {
    Database,
    Schema,
    Folder(Folder),
    Table,
    PartitionedTable,
    ForeignTable,
    View,
    MaterializedView,
    Sequence,
    Function,
    Procedure,
    Type,
    Column,
    Index,
    Constraint,
    Trigger,
    /// Una política de Row-Level Security. Cuelga de la tabla que filtra.
    Policy,
    /// Del clúster entero, no de una base: vive como hermano de las bases en la raíz del árbol.
    Role,
    /// Una extensión instalada en la base. Cuelga de la carpeta "Extensiones" de cada base.
    Extension,
    /// Un wrapper de datos externos (`FOREIGN DATA WRAPPER`). Cuelga de "Wrappers foráneos".
    ForeignDataWrapper,
    /// Un servidor foráneo (`SERVER`). Cuelga de "Servidores foráneos". Sus mapeos de usuario se
    /// gestionan en su panel de detalle, no como nodos aparte.
    ForeignServer,
}

/// Rasgo de un objeto que conviene ver sin abrirlo.
///
/// Es un vocabulario cerrado y no texto libre porque la interfaz les da color y forma: `detail`
/// sirve para lo que solo se puede leer (el dueño, la firma de una función), y esto para lo que se
/// reconoce de un vistazo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeTag {
    /// El rol puede iniciar sesión.
    Login,
    /// El rol no puede iniciar sesión: existe para agrupar privilegios.
    Group,
    Superuser,
    /// La tabla es una partición de otra.
    Partition,
    /// La tabla tiene activada la seguridad por fila.
    RowSecurity,
    /// Una tabla sin clave primaria: la grilla la abre en solo lectura y la replicación lógica no
    /// sabe cómo identificar sus filas.
    NoPrimaryKey,
    /// Un índice que quedó inválido (un `CREATE INDEX CONCURRENTLY` que falló): no lo usa el
    /// planificador y hay que rehacerlo.
    Invalid,
    /// Una vista materializada sin datos: se creó con `WITH NO DATA` y consultarla falla.
    NotPopulated,
}

/// Agrupaciones que se muestran como carpetas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Folder {
    Schemas,
    Tables,
    Views,
    MaterializedViews,
    ForeignTables,
    Sequences,
    Functions,
    Procedures,
    Types,
    Columns,
    Indexes,
    Constraints,
    Triggers,
    Policies,
    /// Del clúster entero: junto con `Databases`, las dos únicas carpetas de la raíz.
    Roles,
    /// Las bases del servidor. Antes colgaban sueltas de la raíz, al lado de «Roles», mientras que
    /// adentro de cada base todo iba en carpetas: la raíz era lo único que no estaba agrupado.
    Databases,
    /// De nivel base: hermana de "Esquemas" bajo cada base.
    Extensions,
    /// De nivel base: los wrappers de datos externos.
    ForeignDataWrappers,
    /// De nivel base: los servidores foráneos.
    ForeignServers,
}

impl Folder {
    pub fn label(self) -> &'static str {
        match self {
            Folder::Schemas => "Esquemas",
            Folder::Tables => "Tablas",
            Folder::Views => "Vistas",
            Folder::MaterializedViews => "Vistas materializadas",
            Folder::ForeignTables => "Tablas externas",
            Folder::Sequences => "Secuencias",
            Folder::Functions => "Funciones",
            Folder::Procedures => "Procedimientos",
            Folder::Types => "Tipos",
            Folder::Columns => "Columnas",
            Folder::Indexes => "Índices",
            Folder::Constraints => "Restricciones",
            Folder::Triggers => "Disparadores",
            Folder::Policies => "Políticas",
            Folder::Roles => "Roles",
            Folder::Databases => "Bases de datos",
            Folder::Extensions => "Extensiones",
            Folder::ForeignDataWrappers => "Wrappers foráneos",
            Folder::ForeignServers => "Servidores foráneos",
        }
    }

    /// Segmento con el que se identifica la carpeta dentro del id del nodo.
    fn slug(self) -> &'static str {
        match self {
            Folder::Schemas => "schemas",
            Folder::Tables => "tables",
            Folder::Views => "views",
            Folder::MaterializedViews => "matviews",
            Folder::ForeignTables => "foreign",
            Folder::Sequences => "sequences",
            Folder::Functions => "functions",
            Folder::Procedures => "procedures",
            Folder::Types => "types",
            Folder::Columns => "columns",
            Folder::Indexes => "indexes",
            Folder::Constraints => "constraints",
            Folder::Triggers => "triggers",
            Folder::Policies => "policies",
            Folder::Roles => "roles",
            Folder::Databases => "databases",
            Folder::Extensions => "extensions",
            Folder::ForeignDataWrappers => "fdws",
            Folder::ForeignServers => "fservers",
        }
    }
}

/// Un nodo del árbol.
///
/// Lleva encima todo el contexto que hace falta para pedir sus hijos, de modo que expandir un nodo
/// no obligue a la interfaz a recordar el camino recorrido ni al núcleo a mantener estado del
/// árbol entre llamadas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    /// Identificador estable dentro de un servidor. La interfaz lo usa como clave.
    pub id: String,
    pub label: String,
    /// Texto secundario: firma de la función, tipo de la columna, definición del índice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub kind: NodeKind,
    /// Si el nodo se puede expandir. Las hojas no muestran flecha.
    pub has_children: bool,
    pub database: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// OID del objeto, o del objeto contenedor en el caso de las carpetas de una tabla.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Rasgos del objeto, para mostrar como etiquetas junto al nombre.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<NodeTag>,
    /// Filas estimadas (`reltuples`) de una relación, para ordenar el árbol por tamaño sin tener que
    /// leer el texto de `detail`. `None` donde no se sabe o no aplica.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<i64>,
    /// Tamaño en disco de una relación, si se calculó (ver `SIZE_LIMIT`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
}

impl TreeNode {
    pub fn database(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            id: format!("db:{name}"),
            label: name.clone(),
            detail: None,
            kind: NodeKind::Database,
            has_children: true,
            database: name,
            schema: None,
            oid: None,
            comment: None,
            tags: Vec::new(),
            rows: None,
            size_bytes: None,
        }
    }

    /// Una carpeta sin padre: hace falta para "Roles" y "Bases de datos", que no cuelgan de ninguna
    /// base sino de la raíz del servidor. `database` es la que se usa para conectar y leer el
    /// catálogo — cualquiera sirve, `pg_roles` se ve igual desde todas.
    pub(crate) fn root_folder(database: impl Into<String>, folder: Folder, count: i64) -> Self {
        Self {
            id: format!("f:{}", folder.slug()),
            label: folder.label().to_owned(),
            detail: Some(count.to_string()),
            kind: NodeKind::Folder(folder),
            has_children: count > 0,
            database: database.into(),
            schema: None,
            oid: None,
            comment: None,
            tags: Vec::new(),
            rows: None,
            size_bytes: None,
        }
    }

    pub(crate) fn folder(parent: &TreeNode, folder: Folder, count: i64) -> Self {
        Self {
            id: format!("{}/f:{}", parent.id, folder.slug()),
            label: folder.label().to_owned(),
            detail: Some(count.to_string()),
            kind: NodeKind::Folder(folder),
            // Una carpeta vacía sin flecha comunica "acá no hay nada" sin obligar a expandirla.
            has_children: count > 0,
            database: parent.database.clone(),
            schema: parent.schema.clone(),
            // Las carpetas de una tabla necesitan recordar de qué tabla cuelgan.
            oid: parent.oid,
            comment: None,
            tags: Vec::new(),
            rows: None,
            size_bytes: None,
        }
    }

    pub(crate) fn schema(parent: &TreeNode, name: String, oid: u32, owner: Option<String>) -> Self {
        Self {
            id: format!("{}/sch:{name}", parent.id),
            label: name.clone(),
            detail: owner,
            kind: NodeKind::Schema,
            has_children: true,
            database: parent.database.clone(),
            schema: Some(name),
            oid: Some(oid),
            comment: None,
            tags: Vec::new(),
            rows: None,
            size_bytes: None,
        }
    }

    pub(crate) fn object(
        parent: &TreeNode,
        kind: NodeKind,
        oid: u32,
        label: String,
        detail: Option<String>,
        has_children: bool,
    ) -> Self {
        Self {
            id: format!("{}/o:{oid}", parent.id),
            label,
            detail,
            kind,
            has_children,
            database: parent.database.clone(),
            schema: parent.schema.clone(),
            oid: Some(oid),
            comment: None,
            tags: Vec::new(),
            rows: None,
            size_bytes: None,
        }
    }

    /// Hijo de una tabla que no tiene OID propio, como una columna.
    pub(crate) fn leaf(
        parent: &TreeNode,
        kind: NodeKind,
        key: impl std::fmt::Display,
        label: String,
        detail: Option<String>,
    ) -> Self {
        Self {
            id: format!("{}/c:{key}", parent.id),
            label,
            detail,
            kind,
            has_children: false,
            database: parent.database.clone(),
            schema: parent.schema.clone(),
            oid: parent.oid,
            comment: None,
            tags: Vec::new(),
            rows: None,
            size_bytes: None,
        }
    }

    pub(crate) fn with_comment(mut self, comment: Option<String>) -> Self {
        self.comment = comment;
        self
    }

    pub(crate) fn with_measures(mut self, rows: Option<i64>, size_bytes: Option<i64>) -> Self {
        self.rows = rows;
        self.size_bytes = size_bytes;
        self
    }

    pub(crate) fn with_tags(mut self, tags: Vec<NodeTag>) -> Self {
        self.tags = tags;
        self
    }
}
