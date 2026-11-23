//! The HTTP client of the exporter (14.9, R6, R7): one connection for each export, plain HTTP or
//! HTTPS with `rustls` and the configured CA only. No proxy. No environment variable.

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use http::header::{CONNECTION, CONTENT_TYPE, HOST, USER_AGENT};
use http_body_util::Full;
use hyper_util::rt::TokioIo;
use rustls::pki_types::{CertificateDer, ServerName, pem::PemObject};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio_rustls::TlsConnector;

use super::TelemetryError;

/// The parts of the endpoint URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Endpoint {
    pub(crate) https: bool,
    /// The host without brackets.
    pub(crate) host: String,
    pub(crate) port: u16,
}

impl Endpoint {
    /// Parses `http://host[:port]` or `https://host[:port]`. Any path, query, fragment or user
    /// info is an error. The configuration checks the same rules (13.9).
    pub(crate) fn parse(text: &str) -> Result<Endpoint, TelemetryError> {
        let bad =
            || TelemetryError::new(format!("telemetry.otlp_endpoint: the value {text:?} is not a valid endpoint"));
        let (https, rest) = if let Some(r) = text.strip_prefix("https://") {
            (true, r)
        } else if let Some(r) = text.strip_prefix("http://") {
            (false, r)
        } else {
            return Err(bad());
        };
        if rest.is_empty() || rest.contains(['/', '?', '#', '@', '\\']) {
            return Err(bad());
        }
        let default_port = if https { 443 } else { 80 };
        let (host, port) = if let Some(inner) = rest.strip_prefix('[') {
            let close = inner.find(']').ok_or_else(bad)?;
            let host = &inner[..close];
            let after = &inner[close + 1..];
            let port = match after {
                "" => default_port,
                _ => after.strip_prefix(':').ok_or_else(bad)?.parse::<u16>().map_err(|_| bad())?,
            };
            (host.to_owned(), port)
        } else {
            match rest.split_once(':') {
                Some((h, p)) => (h.to_owned(), p.parse::<u16>().map_err(|_| bad())?),
                None => (rest.to_owned(), default_port),
            }
        };
        if host.is_empty() || port == 0 {
            return Err(bad());
        }
        Ok(Endpoint { https, host, port })
    }

    fn host_header(&self) -> String {
        let default_port = if self.https { 443 } else { 80 };
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        if self.port == default_port { host } else { format!("{host}:{}", self.port) }
    }
}

/// Builds the rustls client configuration: the `ring` provider, TLS 1.2 and 1.3, and the CA
/// certificates of the PEM bytes as the only roots (R6). It reads no file and no system store.
pub(crate) fn tls_client_config(ca_pem: &[u8]) -> Result<rustls::ClientConfig, TelemetryError> {
    let mut roots = rustls::RootCertStore::empty();
    let mut count = 0usize;
    for item in CertificateDer::pem_slice_iter(ca_pem) {
        let cert =
            item.map_err(|e| TelemetryError::new(format!("telemetry.ca_file: the PEM text is not valid: {e}")))?;
        roots
            .add(cert)
            .map_err(|e| TelemetryError::new(format!("telemetry.ca_file: a certificate is not valid: {e}")))?;
        count += 1;
    }
    if count == 0 {
        return Err(TelemetryError::new("telemetry.ca_file: the PEM text holds no CERTIFICATE block"));
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
        .map_err(|e| TelemetryError::new(format!("telemetry: the TLS versions are not supported: {e}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(config)
}

/// A plain or a TLS stream.
enum Io {
    Plain(TcpStream),
    Tls(Box<tokio_rustls::client::TlsStream<TcpStream>>),
}

impl AsyncRead for Io {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Io::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Io::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Io {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Io::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Io::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Io::Plain(s) => Pin::new(s).poll_flush(cx),
            Io::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Io::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Io::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
        }
    }
}

/// Stops the task that drives the connection when the export ends.
struct DriverGuard(JoinHandle<()>);

impl Drop for DriverGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The client of one endpoint.
pub(crate) struct Client {
    endpoint: Endpoint,
    host_header: String,
    tls: Option<(TlsConnector, ServerName<'static>)>,
    user_agent: String,
}

impl Client {
    /// Makes the client. An `https` endpoint needs the CA bytes.
    pub(crate) fn new(endpoint: Endpoint, ca_pem: Option<&[u8]>, user_agent: String) -> Result<Client, TelemetryError> {
        let tls = if endpoint.https {
            let pem =
                ca_pem.ok_or_else(|| TelemetryError::new("telemetry.ca_file: an https endpoint needs a CA file"))?;
            let name = ServerName::try_from(endpoint.host.clone()).map_err(|_| {
                TelemetryError::new("telemetry.otlp_endpoint: the host is not a DNS name or an IP address")
            })?;
            Some((TlsConnector::from(Arc::new(tls_client_config(pem)?)), name))
        } else {
            None
        };
        let host_header = endpoint.host_header();
        Ok(Client { endpoint, host_header, tls, user_agent })
    }

    /// Sends one `POST /v1/traces`. A status other than 2xx is an error. The caller sets the
    /// deadline.
    pub(crate) async fn post(&self, body: Vec<u8>) -> Result<(), String> {
        let tcp = TcpStream::connect((self.endpoint.host.as_str(), self.endpoint.port))
            .await
            .map_err(|e| format!("connect: {}", e.kind()))?;
        let _ = tcp.set_nodelay(true);
        let io = match &self.tls {
            None => Io::Plain(tcp),
            Some((connector, name)) => {
                let stream = connector.connect(name.clone(), tcp).await.map_err(|e| format!("tls: {e}"))?;
                Io::Tls(Box::new(stream))
            }
        };
        let (mut sender, connection) = hyper::client::conn::http1::handshake::<_, Full<Bytes>>(TokioIo::new(io))
            .await
            .map_err(|e| format!("http: {e}"))?;
        let _guard = DriverGuard(tokio::spawn(async move {
            let _ = connection.await;
        }));
        let request = http::Request::builder()
            .method(http::Method::POST)
            .uri("/v1/traces")
            .header(HOST, &self.host_header)
            .header(CONTENT_TYPE, "application/json")
            .header(USER_AGENT, &self.user_agent)
            .header(CONNECTION, "close")
            .body(Full::new(Bytes::from(body)))
            .map_err(|e| format!("http: {e}"))?;
        let response = sender.send_request(request).await.map_err(|e| format!("http: {e}"))?;
        let status = response.status();
        if status.is_success() { Ok(()) } else { Err(format!("status {}", status.as_u16())) }
    }
}
