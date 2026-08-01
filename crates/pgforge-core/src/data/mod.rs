//! Datos de una tabla: leerlos de a páginas y cambiarlos.
//!
//! Es el reverso de [`crate::sql`]. Allá el texto lo escribe el usuario y pgforge solo lo ejecuta;
//! acá las consultas las arma pgforge sobre una tabla cuya forma conoce, y por eso puede permitir
//! que se editen los valores: sabe de qué tabla sale cada columna, cuál es la clave y qué tipo
//! tiene cada celda.

pub mod edit;
pub mod page;
pub mod shape;

pub use edit::{apply, statements, Applied, Change, Statement, Values};
pub use page::{page, select, Cursor, Page, DEFAULT_PAGE_SIZE};
pub use shape::{shape, Column, Key, KeyKind, TableShape};
