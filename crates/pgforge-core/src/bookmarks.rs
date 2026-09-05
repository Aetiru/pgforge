//! Marcadores: objetos del catálogo que el usuario decidió tener siempre a mano.
//!
//! Mismo molde que `sql::saved` —archivo propio en SQLite, mismo patrón de apertura y migración—,
//! pero con una diferencia central: acá el servidor **sí** ata. Una consulta guardada tiene sentido
//! correrla contra cualquier base; `public.clientes` de desarrollo y `public.clientes` de producción
//! son objetos distintos, y un marcador sin servidor no se puede abrir.
//!
//! Lo que hay detrás es una estrella que se aprieta, no un formulario: `add` es idempotente en vez
//! de devolver `Conflict` como `SavedStore::save`, y nunca se borra solo un marcador cuyo objeto ya
//! no está —eso lo nota quien lo abre, no quien lista— porque un servidor apagado no es un objeto
//! borrado.

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Versión del esquema del archivo. Subirla obliga a agregar el paso de migración de abajo.
const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS bookmarks (
        id         INTEGER PRIMARY KEY,
        profile_id TEXT    NOT NULL,
        database   TEXT    NOT NULL,
        schema     TEXT    NOT NULL,
        name       TEXT    NOT NULL,
        kind       TEXT    NOT NULL,
        label      TEXT,
        created_at INTEGER NOT NULL
    );
    -- Marcar dos veces el mismo objeto no duplica: `add` primero busca por acá.
    CREATE UNIQUE INDEX IF NOT EXISTS bookmarks_target
        ON bookmarks (profile_id, database, schema, name, kind);
";

/// Qué clase de objeto es. Vocabulario cerrado: son los que el árbol deja marcar, no texto libre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BookmarkKind {
    Table,
    View,
    MaterializedView,
    Function,
    Procedure,
}

impl BookmarkKind {
    fn as_str(self) -> &'static str {
        match self {
            BookmarkKind::Table => "table",
            BookmarkKind::View => "view",
            BookmarkKind::MaterializedView => "materializedView",
            BookmarkKind::Function => "function",
            BookmarkKind::Procedure => "procedure",
        }
    }

    fn from_str(text: &str) -> Option<Self> {
        Some(match text {
            "table" => BookmarkKind::Table,
            "view" => BookmarkKind::View,
            "materializedView" => BookmarkKind::MaterializedView,
            "function" => BookmarkKind::Function,
            "procedure" => BookmarkKind::Procedure,
            _ => return None,
        })
    }
}

/// A qué objeto apunta un marcador. Es la clave que lo identifica sin el `id`: lo que necesita la
/// estrella para saber si ya está prendida y para poder apagarse sin conocer el `id` de memoria.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub profile_id: String,
    pub database: String,
    pub schema: String,
    pub name: String,
    pub kind: BookmarkKind,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewBookmark {
    pub target: Target,
    /// Alias del usuario. Con `None` la lista muestra el nombre del objeto tal cual.
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub id: i64,
    pub profile_id: String,
    pub database: String,
    pub schema: String,
    pub name: String,
    pub kind: BookmarkKind,
    pub label: Option<String>,
    /// Segundos desde el epoch.
    pub created_at: i64,
}

pub struct BookmarkStore {
    connection: Connection,
}

const SELECT: &str =
    "SELECT id, profile_id, database, schema, name, kind, label, created_at FROM bookmarks";

impl BookmarkStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let connection = Connection::open(path)?;

        // Mismo criterio que `saved`/`history`: que el archivo siga abriendo después de un corte
        // importa más que la última escritura.
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;

        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let version: i64 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;

        if version < SCHEMA_VERSION {
            self.connection.execute_batch(SCHEMA)?;
            self.connection
                .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }

        Ok(())
    }

    /// Todos, agrupados por servidor en la interfaz y no acá: son decenas, y agrupar es una
    /// pregunta de cómo se muestran, no de cómo se guardan.
    pub fn list(&self) -> Result<Vec<Bookmark>> {
        self.query(
            &format!("{SELECT} ORDER BY profile_id, schema COLLATE NOCASE, name COLLATE NOCASE"),
            params![],
        )
    }

    pub fn list_for(&self, profile_id: &str) -> Result<Vec<Bookmark>> {
        self.query(
            &format!("{SELECT} WHERE profile_id = ?1 ORDER BY schema COLLATE NOCASE, name COLLATE NOCASE"),
            params![profile_id],
        )
    }

    /// Marca un objeto. Apretar la estrella sobre algo ya marcado no falla ni duplica: devuelve el
    /// que ya estaba, con su alias intacto —si hiciera falta cambiarlo está `set_label`—.
    pub fn add(&self, input: &NewBookmark, now: i64) -> Result<Bookmark> {
        let target = &input.target;
        self.connection.execute(
            "INSERT INTO bookmarks (profile_id, database, schema, name, kind, label, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (profile_id, database, schema, name, kind) DO NOTHING",
            params![
                target.profile_id,
                target.database,
                target.schema,
                target.name,
                target.kind.as_str(),
                input.label,
                now,
            ],
        )?;

        self.find(target)?.ok_or_else(|| {
            crate::error::Error::Config("no se pudo releer el marcador recién creado".to_owned())
        })
    }

    /// Borra por `id`. Devuelve `false` si no había nada con ese identificador.
    pub fn remove(&self, id: i64) -> Result<bool> {
        Ok(self
            .connection
            .execute("DELETE FROM bookmarks WHERE id = ?1", params![id])?
            > 0)
    }

    /// Borra por objeto, para cuando la estrella se apaga desde el árbol y ahí solo se tiene el
    /// objeto, no el `id` del marcador.
    pub fn remove_target(&self, target: &Target) -> Result<bool> {
        Ok(self
            .connection
            .execute(
                "DELETE FROM bookmarks
                  WHERE profile_id = ?1 AND database = ?2 AND schema = ?3 AND name = ?4 AND kind = ?5",
                params![
                    target.profile_id,
                    target.database,
                    target.schema,
                    target.name,
                    target.kind.as_str(),
                ],
            )?
            > 0)
    }

    pub fn set_label(&self, id: i64, label: Option<&str>) -> Result<bool> {
        Ok(self.connection.execute(
            "UPDATE bookmarks SET label = ?2 WHERE id = ?1",
            params![id, label],
        )? > 0)
    }

    fn find(&self, target: &Target) -> Result<Option<Bookmark>> {
        Ok(self
            .query(
                &format!(
                    "{SELECT} WHERE profile_id = ?1 AND database = ?2 AND schema = ?3 \
                     AND name = ?4 AND kind = ?5"
                ),
                params![
                    target.profile_id,
                    target.database,
                    target.schema,
                    target.name,
                    target.kind.as_str(),
                ],
            )?
            .pop())
    }

    fn query(&self, sql: &str, params: impl rusqlite::Params) -> Result<Vec<Bookmark>> {
        let mut statement = self.connection.prepare(sql)?;
        let rows = statement.query_map(params, |row| {
            let kind: String = row.get(5)?;
            Ok(Bookmark {
                id: row.get(0)?,
                profile_id: row.get(1)?,
                database: row.get(2)?,
                schema: row.get(3)?,
                name: row.get(4)?,
                // Vocabulario cerrado y grabado por esta misma versión: un `kind` que no matchea es
                // un archivo corrupto, no un caso de negocio, así que cae a `Table` en vez de
                // propagar un error que ensuciaría toda lectura de la lista por una fila.
                kind: BookmarkKind::from_str(&kind).unwrap_or(BookmarkKind::Table),
                label: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;

        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AHORA: i64 = 1_800_000_000;

    fn store() -> BookmarkStore {
        BookmarkStore::open(":memory:").unwrap()
    }

    fn objetivo(name: &str) -> Target {
        Target {
            profile_id: "servidor-1".into(),
            database: "app".into(),
            schema: "public".into(),
            name: name.into(),
            kind: BookmarkKind::Table,
        }
    }

    #[test]
    fn marca_y_lista_por_servidor() {
        let store = store();
        store
            .add(
                &NewBookmark {
                    target: objetivo("clientes"),
                    label: None,
                },
                AHORA,
            )
            .unwrap();
        store
            .add(
                &NewBookmark {
                    target: objetivo("pedidos"),
                    label: None,
                },
                AHORA,
            )
            .unwrap();

        let all = store.list().unwrap();
        let names: Vec<_> = all.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["clientes", "pedidos"]);
        assert_eq!(store.list_for("servidor-1").unwrap().len(), 2);
        assert!(store.list_for("otro-servidor").unwrap().is_empty());
    }

    #[test]
    fn marcar_dos_veces_lo_mismo_no_duplica() {
        let store = store();
        let primero = store
            .add(
                &NewBookmark {
                    target: objetivo("clientes"),
                    label: Some("importante".into()),
                },
                AHORA,
            )
            .unwrap();
        let segundo = store
            .add(
                &NewBookmark {
                    target: objetivo("clientes"),
                    label: None,
                },
                AHORA + 60,
            )
            .unwrap();

        assert_eq!(primero.id, segundo.id);
        assert_eq!(store.list().unwrap().len(), 1);
        // El segundo intento no pisó el alias del primero: apretar la estrella de nuevo no borra
        // lo que el usuario ya había escrito.
        assert_eq!(segundo.label.as_deref(), Some("importante"));
    }

    #[test]
    fn la_misma_tabla_en_dos_servidores_son_dos_marcadores() {
        let store = store();
        let mut otro = objetivo("clientes");
        otro.profile_id = "servidor-2".into();

        store
            .add(
                &NewBookmark {
                    target: objetivo("clientes"),
                    label: None,
                },
                AHORA,
            )
            .unwrap();
        store
            .add(
                &NewBookmark {
                    target: otro,
                    label: None,
                },
                AHORA,
            )
            .unwrap();

        assert_eq!(store.list().unwrap().len(), 2);
    }

    #[test]
    fn borrar_por_id_dice_si_habia_algo() {
        let store = store();
        let marcado = store
            .add(
                &NewBookmark {
                    target: objetivo("clientes"),
                    label: None,
                },
                AHORA,
            )
            .unwrap();

        assert!(store.remove(marcado.id).unwrap());
        assert!(
            !store.remove(marcado.id).unwrap(),
            "borrar dos veces no falla"
        );
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn borrar_por_objeto_es_lo_que_usa_la_estrella_al_apagarse() {
        let store = store();
        store
            .add(
                &NewBookmark {
                    target: objetivo("clientes"),
                    label: None,
                },
                AHORA,
            )
            .unwrap();

        assert!(store.remove_target(&objetivo("clientes")).unwrap());
        assert!(!store.remove_target(&objetivo("clientes")).unwrap());
    }

    #[test]
    fn cambiar_el_alias_no_toca_el_resto() {
        let store = store();
        let marcado = store
            .add(
                &NewBookmark {
                    target: objetivo("clientes"),
                    label: None,
                },
                AHORA,
            )
            .unwrap();

        assert!(store.set_label(marcado.id, Some("prod")).unwrap());
        assert_eq!(store.list().unwrap()[0].label.as_deref(), Some("prod"));

        assert!(store.set_label(marcado.id, None).unwrap());
        assert_eq!(store.list().unwrap()[0].label, None);
    }
}
