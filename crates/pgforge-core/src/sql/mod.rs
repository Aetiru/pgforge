//! Ejecución de SQL escrito a mano.
//!
//! A diferencia de `introspect` y `monitor`, que hacen consultas que escribió pgforge, acá el texto
//! viene del usuario: puede ser cualquier cosa, puede tardar para siempre y puede fallar de formas
//! que hay que poder señalar en el editor. De ahí que cada pestaña de consulta trabaje sobre su
//! propia [`crate::conn::Session`], fuera del pool: es la única manera de poder cancelarla, de que
//! dos consultas no se estorben, y de que un `BEGIN` o una tabla temporal sobrevivan de una
//! ejecución a la siguiente.

pub mod completion;
pub mod exec;
pub mod explain;
pub mod history;
pub mod split;

pub use completion::{Relation, SchemaSnapshot};
pub use exec::{Limits, Outcome, QuerySession, TxStatus, DEFAULT_MAX_ROWS};
pub use explain::{ExplainOptions, Plan, PlanNode};
pub use history::{HistoryStore, NewEntry};
pub use split::{at_cursor, split, Statement};
