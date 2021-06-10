use config::{Environment, File, FileFormat};
use humantime::parse_duration;
use serde::Deserialize;
use std::{convert::TryInto, fmt, path::PathBuf, time::Duration};

fn parse_path<E: Clone>(
    default: Option<PathBuf>,
    error: E,
    path: Option<String>,
) -> Result<Option<PathBuf>, E> {
    Ok(match path.as_deref() {
        None | Some("") => default,
        Some(path) => {
            let path = path.parse::<PathBuf>().map_err(|_| error.clone())?;
            if !path.is_file() {
                return Err(error);
            };
            Some(path)
        },
    })
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub tls_key_path: Option<PathBuf>,
    pub tls_cert_path: Option<PathBuf>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            host: "[::]:8080".into(),
            tls_key_path: None,
            tls_cert_path: None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ServerSettings {
    pub host: Option<String>,
    pub tls_key_path: Option<String>,
    pub tls_cert_path: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ServerSettingsError {
    InvalidTlsKeyPath,
    InvalidTlsCertPath,
}

impl TryInto<ServerConfig> for ServerSettings {
    type Error = ServerSettingsError;

    fn try_into(self) -> Result<ServerConfig, Self::Error> {
        let default = ServerConfig::default();

        let host = self.host.unwrap_or(default.host);

        let tls_key_path = parse_path(
            default.tls_key_path,
            ServerSettingsError::InvalidTlsKeyPath,
            self.tls_key_path,
        )?;

        let tls_cert_path = parse_path(
            default.tls_cert_path,
            ServerSettingsError::InvalidTlsCertPath,
            self.tls_cert_path,
        )?;

        Ok(ServerConfig {
            host,
            tls_key_path,
            tls_cert_path,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct MonerodConfig {
    pub base_url: String,
    pub tls_cert_path: Option<PathBuf>,
    pub skip_tls_verification: bool,
    pub timeout: Duration,
}

impl Default for MonerodConfig {
    fn default() -> Self {
        MonerodConfig {
            base_url: "http://localhost:18081".into(),
            tls_cert_path: None,
            skip_tls_verification: false,
            timeout: Duration::from_secs(1),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct MonerodSettings {
    pub base_url: Option<String>,
    pub tls_cert_path: Option<String>,
    pub skip_tls_verification: Option<bool>,
    pub timeout: Option<String>,
}

#[derive(Clone, Debug)]
pub enum MonerodSettingsError {
    InvalidTlsCertPath,
    InvalidTimeout,
}

impl TryInto<MonerodConfig> for MonerodSettings {
    type Error = MonerodSettingsError;

    fn try_into(self) -> Result<MonerodConfig, Self::Error> {
        let default = MonerodConfig::default();

        let base_url = self.base_url.unwrap_or(default.base_url);

        let tls_cert_path = parse_path(
            default.tls_cert_path,
            MonerodSettingsError::InvalidTlsCertPath,
            self.tls_cert_path,
        )?;

        let skip_tls_verification = self.skip_tls_verification
            .unwrap_or(default.skip_tls_verification);

        let timeout = match self.timeout {
            None => default.timeout,
            Some(timeout) => parse_duration(&timeout)
                .map_err(|_| MonerodSettingsError::InvalidTimeout)?,
        };

        Ok(MonerodConfig {
            base_url,
            tls_cert_path,
            skip_tls_verification,
            timeout,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub refresh_interval: Duration,
    pub block_spans: Vec<u32>,
    pub server: ServerConfig,
    pub monerod: MonerodConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            refresh_interval: Duration::from_secs(15),
            block_spans: vec![30, 180, 720],
            server: ServerConfig::default(),
            monerod: MonerodConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub refresh_interval: Option<String>,
    pub block_spans: Option<String>,
    pub server: Option<ServerSettings>,
    pub monerod: Option<MonerodSettings>,
}

#[derive(Debug)]
pub enum SettingsError {
    InvalidRefreshInterval,
    InvalidBlockSpans,
    ServerSettings(ServerSettingsError),
    MonerodSettings(MonerodSettingsError),
}

impl TryInto<Config> for Settings {
    type Error = SettingsError;

    fn try_into(self) -> Result<Config, Self::Error> {
        let default = Config::default();

        let refresh_interval = match self.refresh_interval {
            None => default.refresh_interval,
            Some(interval) => parse_duration(&interval)
                .map_err(|_| SettingsError::InvalidRefreshInterval)?,
        };

        let block_spans = match self.block_spans {
            None => default.block_spans,
            Some(spans) => spans
                .split_terminator(',')
                .map(str::parse)
                .collect::<Result<Vec<u32>, _>>()
                .map_err(|_| SettingsError::InvalidBlockSpans)?,
        };

        let server = match self.server {
            None => ServerConfig::default(),
            Some(server) => server.try_into().map_err(SettingsError::ServerSettings)?,
        };

        let monerod = match self.monerod {
            None => MonerodConfig::default(),
            Some(monerod) => monerod.try_into().map_err(SettingsError::MonerodSettings)?,
        };

        Ok(Config {
            refresh_interval,
            block_spans,
            server,
            monerod,
        })
    }
}

impl Settings {
    pub fn load(config_path: Option<&str>) -> Result<Settings, config::ConfigError> {
        let mut cfg = config::Config::default();
        if let Some(config_path) = config_path {
            cfg.merge(File::new(config_path, FileFormat::Toml).required(false))?;
        }
        cfg.merge(Environment::with_prefix("MONEROD_EXPORTER").separator("__"))?;
        cfg.try_into()
    }
}

#[derive(Debug)]
pub enum ConfigLoadError {
    Load(config::ConfigError),
    Validation(SettingsError),
}

impl fmt::Display for ConfigLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigLoadError::Load(e) => {
                write!(f, "load failed: {}", e)
            },
            ConfigLoadError::Validation(e) => {
                write!(f, "invalid config: {:?}", e)
            }
        }
    }
}

impl Config {
    pub fn load(config_path: Option<&str>) -> Result<Config, ConfigLoadError> {
        Settings::load(config_path)
            .map_err(ConfigLoadError::Load)?
            .try_into()
            .map_err(ConfigLoadError::Validation)
    }
}
