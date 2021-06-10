mod client;
mod config;
mod metrics;
mod prometheus;

use reqwest::{Certificate, ClientBuilder};
use tracing::{debug, warn};
use tracing_subscriber::{prelude::*, EnvFilter};
use std::{env, error, fmt, fs, net::SocketAddr, pin::Pin, sync::Arc, time::Duration};
use tokio::{net::lookup_host, select};
use warp::{Filter, Future, http::StatusCode};

use client::Client;
use metrics::{Exporter, Publisher};
use crate::config::{Config, ConfigLoadError, MonerodConfig, ServerConfig};

enum Error {
    Config(ConfigLoadError),
    Publisher(Box<dyn error::Error>),
    Server(Box<dyn error::Error>),
}

impl From<ConfigLoadError> for Error {
    fn from(e: ConfigLoadError) -> Self {
        Error::Config(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Config(e) => {
                write!(f, "failed to create config: {}", e)
            },
            Error::Publisher(e) => {
                write!(f, "failed to create publisher: {}", e)
            },
            Error::Server(e) => {
                write!(f, "failed to create HTTP server: {}", e)
            },
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

type Server = dyn FnOnce(SocketAddr) -> Pin<Box<dyn Future<Output = ()>>>;

fn init_tracing() {
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();
    let fmt_layer = tracing_subscriber::fmt::layer();

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .init();
}

fn create_publisher(
    refresh_interval: Duration,
    block_spans: Vec<u32>,
    config: MonerodConfig,
) -> Result<Publisher, Box<dyn std::error::Error>> {
    let mut http_client = ClientBuilder::new().timeout(config.timeout);

    if let Some(tls_cert_path) = config.tls_cert_path {
        let cert_data = fs::read(tls_cert_path)?;
        let cert = Certificate::from_pem(&cert_data)?;
        http_client = http_client.add_root_certificate(cert);
    }

    if config.skip_tls_verification {
        warn!("TLS verification disabled for Monero RPC client");
        http_client = http_client
            .danger_accept_invalid_hostnames(true)
            .danger_accept_invalid_certs(true);
    }

    let http_client = http_client.build()?;
    let client = Client::new(http_client, config.base_url);
    let exporter = Exporter::new(client, block_spans);
    let publisher = Publisher::new(exporter, refresh_interval);

    Ok(publisher)
}

fn create_server(
    publisher: Arc<Publisher>,
    config: ServerConfig,
) -> Result<Box<Server>, Box<dyn error::Error>> {
    let filter = warp::any()
        .map(move || match publisher.get_metrics() {
            None => warp::reply::with_status(String::new(), StatusCode::SERVICE_UNAVAILABLE),
            Some(metrics) => warp::reply::with_status(metrics, StatusCode::OK),
        });

    Ok(Box::new(move |socket_addr| {
        if config.tls_key_path.is_some() {
            let mut server = warp::serve(filter).tls();
            if let Some(path) = config.tls_key_path {
                server = server.key_path(path);
            }
            if let Some(path) = config.tls_cert_path {
                server = server.cert_path(path);
            }
            Box::pin(server.run(socket_addr))
        } else {
            Box::pin(warp::serve(filter).run(socket_addr))
        }
    }))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    init_tracing();

    let args = env::args().collect::<Vec<_>>();
    let args = args.iter().map(|s| s.as_ref()).collect::<Vec<&str>>();

    let config_path = match args.as_slice() {
        [_, "-c", path] => Some(String::from(*path)),
        _ => dirs::config_dir().and_then(|mut p| {
            p.push("monerod-exporter.toml");
            p.to_str().map(String::from)
        }),
    };

    let config = Config::load(config_path.as_deref())
        .map_err(Error::Config)?;

    debug!("config: {:?}", config);

    let publisher = create_publisher(config.refresh_interval, config.block_spans, config.monerod)
        .map_err(Error::Publisher)?;
    let publisher = Arc::new(publisher);

    let socket_addr = lookup_host(&config.server.host)
        .await.map_err(|e| Error::Server(e.into()))?
        .next().ok_or(Error::Server("hostname lookup failed".into()))?;

    let server = create_server(publisher.clone(), config.server)
        .map_err(Error::Server)?;

    select! {
        _ = publisher.run() => {},
        _ = server(socket_addr) => {},
    }

    Ok(())
}
