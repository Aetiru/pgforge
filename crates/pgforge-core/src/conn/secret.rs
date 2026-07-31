//! Contraseñas en memoria.

/// Contraseña que nunca aparece en un log.
///
/// El `Debug` derivado de cualquier struct que contenga una contraseña la imprimiría entera en el
/// primer `tracing::debug!` que alguien agregue sin pensarlo. Este tipo hace que eso sea imposible
/// por construcción: el valor solo sale por [`Password::expose`], que es visible en una búsqueda.
#[derive(Clone, PartialEq, Eq)]
pub struct Password(String);

impl Password {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Único acceso al valor en claro. Usar solo al construir la configuración de conexión o al
    /// guardar en el almacén de credenciales del sistema.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Password(oculta)")
    }
}

impl std::fmt::Display for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("********")
    }
}

impl From<String> for Password {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nunca_se_imprime_el_valor() {
        let p = Password::new("clave-secreta");
        assert!(!format!("{p:?}").contains("clave-secreta"));
        assert!(!format!("{p}").contains("clave-secreta"));
        assert_eq!(p.expose(), "clave-secreta");
    }
}
