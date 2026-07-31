//! Núcleo de pgforge.
//!
//! Este crate no depende de Tauri ni de ninguna interfaz gráfica: todo lo que la aplicación de
//! escritorio puede hacer debe poder hacerse también desde `pgforge-cli`. Si algo solo funciona
//! desde la ventana, pertenece al lugar equivocado.

#![warn(clippy::disallowed_macros)]

pub mod caps;
pub mod error;

pub use caps::{ServerCaps, ServerVersion};
pub use error::{Error, Result};
