//! Cifrado del canal.
//!
//! Se implementa la misma escala que `libpq` en lugar de un simple interruptor de SSL: `require`
//! cifra pero no prueba nada sobre la identidad del servidor, y presentarlo como equivalente a
//! `verify-full` le daría al usuario una garantía que no tiene.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_postgres::tls::MakeTlsConnect;
use tokio_postgres_rustls::MakeRustlsConnect;

use super::profile::{ConnectionProfile, SslMode};
use crate::error::{Error, Result};

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// Conector TLS de una conexión.
///
/// Envuelve al de `rustls` con un solo agregado: poder reemplazar el nombre que `tokio-postgres` le
/// pasa —el host al que conecta el cliente— por el del servidor real. Los dos coinciden salvo por un
/// túnel SSH, donde el cliente conecta a `127.0.0.1` pero quien presenta el certificado es el
/// servidor del otro lado del bastión. Ese nombre es el que rustls valida contra el certificado y el
/// que viaja en el SNI, así que sin reemplazarlo `verify-full` no tendría nada que comparar y el
/// servidor elegiría su certificado por omisión en vez del que corresponde al nombre pedido.
#[derive(Clone)]
pub struct Connector {
    inner: MakeRustlsConnect,
    server_name: Option<String>,
}

impl<S> MakeTlsConnect<S> for Connector
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Stream = <MakeRustlsConnect as MakeTlsConnect<S>>::Stream;
    type TlsConnect = <MakeRustlsConnect as MakeTlsConnect<S>>::TlsConnect;
    type Error = <MakeRustlsConnect as MakeTlsConnect<S>>::Error;

    fn make_tls_connect(
        &mut self,
        domain: &str,
    ) -> std::result::Result<Self::TlsConnect, Self::Error> {
        let domain = self.server_name.as_deref().unwrap_or(domain);
        // Calificado entero porque `MakeRustlsConnect` implementa el trait para muchos flujos y el
        // método suelto no dice cuál es el de acá.
        <MakeRustlsConnect as MakeTlsConnect<S>>::make_tls_connect(&mut self.inner, domain)
    }
}

/// Construye el conector TLS que corresponde al modo del perfil.
///
/// Devuelve `None` cuando el modo es `disable`, en cuyo caso la conexión va en claro.
///
/// `server_name` es el nombre por el que se conoce al servidor cuando no es el mismo al que conecta
/// el cliente —o sea, el host del perfil cuando se llega por un túnel SSH—. Con `None` vale el del
/// destino, que es el caso directo.
pub fn connector(
    profile: &ConnectionProfile,
    server_name: Option<&str>,
) -> Result<Option<Connector>> {
    if profile.ssl_mode == SslMode::Disable {
        return Ok(None);
    }

    let config = match profile.ssl_mode {
        SslMode::Disable => unreachable!("descartado arriba"),
        SslMode::Prefer | SslMode::Require => client_config_without_verification(),
        SslMode::VerifyCa => client_config_verifying(profile, false)?,
        SslMode::VerifyFull => client_config_verifying(profile, true)?,
    };

    Ok(Some(Connector {
        inner: MakeRustlsConnect::new(config),
        server_name: server_name.map(str::to_owned),
    }))
}

fn client_config_without_verification() -> ClientConfig {
    let provider = provider();
    ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .expect("las versiones por omisión siempre son válidas")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyServer { provider }))
        .with_no_client_auth()
}

fn client_config_verifying(
    profile: &ConnectionProfile,
    check_hostname: bool,
) -> Result<ClientConfig> {
    let mut roots = RootCertStore::empty();

    let native = rustls_native_certs::load_native_certs();
    for cert in native.certs {
        // Un certificado ilegible del almacén del sistema no debería impedir usar los demás.
        let _ = roots.add(cert);
    }

    if let Some(path) = &profile.root_cert {
        let pem = std::fs::read(path)?;
        let mut cursor = std::io::Cursor::new(pem);
        let mut added = 0usize;
        for cert in rustls_pemfile::certs(&mut cursor) {
            let cert = cert.map_err(|e| {
                Error::Config(format!(
                    "el certificado raíz {} no es válido: {e}",
                    path.display()
                ))
            })?;
            roots.add(cert).map_err(|e| {
                Error::Config(format!(
                    "el certificado raíz {} no es usable: {e}",
                    path.display()
                ))
            })?;
            added += 1;
        }
        if added == 0 {
            return Err(Error::Config(format!(
                "el archivo {} no contiene ningún certificado",
                path.display()
            )));
        }
    }

    if roots.is_empty() {
        return Err(Error::Config(
            "no hay certificados raíz disponibles para validar el servidor".to_owned(),
        ));
    }

    let provider = provider();
    let roots = Arc::new(roots);

    let builder = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .expect("las versiones por omisión siempre son válidas");

    let config = if check_hostname {
        let verifier = WebPkiServerVerifier::builder_with_provider(roots, provider)
            .build()
            .map_err(|e| Error::Config(format!("no se pudo preparar la validación TLS: {e}")))?;
        builder
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth()
    } else {
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(ChainOnly { roots, provider }))
            .with_no_client_auth()
    };

    Ok(config)
}

/// Verificador de `prefer` y `require`: el tráfico va cifrado, pero no se comprueba con quién.
#[derive(Debug)]
struct AcceptAnyServer {
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for AcceptAnyServer {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Verificador de `verify-ca`: valida que el certificado esté firmado por una raíz de confianza,
/// pero no que el nombre coincida. Es lo que pide quien tiene su propia autoridad certificadora y
/// certificados emitidos para nombres que no son por los que él llega al servidor.
#[derive(Debug)]
struct ChainOnly {
    roots: Arc<RootCertStore>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for ChainOnly {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        rustls::client::verify_server_cert_signed_by_trust_anchor(
            &rustls::server::ParsedCertificate::try_from(end_entity)?,
            &self.roots,
            intermediates,
            now,
            self.provider.signature_verification_algorithms.all,
        )?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}
