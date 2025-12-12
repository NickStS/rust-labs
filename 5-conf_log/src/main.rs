use std::error::Error;

use clap::Parser;
use config::{Config as ConfigBuilder, Environment, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Config {
    #[serde(default)]
    mode: Mode,
    #[serde(default)]
    server: Server,
    #[serde(default)]
    db: Db,
    #[serde(default)]
    log: Log,
    #[serde(default)]
    background: Background,
}

#[derive(Debug, Deserialize)]
struct Mode {
    #[serde(default = "Mode::default_debug")]
    debug: bool,
}

impl Mode {
    fn default_debug() -> bool {
        false
    }
}

impl Default for Mode {
    fn default() -> Self {
        Self {
            debug: Self::default_debug(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Server {
    #[serde(default = "Server::default_external_url")]
    external_url: String,
    #[serde(default = "Server::default_http_port")]
    http_port: u16,
    #[serde(default = "Server::default_grpc_port")]
    grpc_port: u16,
    #[serde(default = "Server::default_healthz_port")]
    healthz_port: u16,
    #[serde(default = "Server::default_metrics_port")]
    metrics_port: u16,
}

impl Server {
    fn default_external_url() -> String {
        "http://127.0.0.1".to_string()
    }
    fn default_http_port() -> u16 {
        8081
    }
    fn default_grpc_port() -> u16 {
        8082
    }
    fn default_healthz_port() -> u16 {
        10025
    }
    fn default_metrics_port() -> u16 {
        9199
    }
}

impl Default for Server {
    fn default() -> Self {
        Self {
            external_url: Self::default_external_url(),
            http_port: Self::default_http_port(),
            grpc_port: Self::default_grpc_port(),
            healthz_port: Self::default_healthz_port(),
            metrics_port: Self::default_metrics_port(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Db {
    #[serde(default)]
    mysql: Mysql,
}

impl Default for Db {
    fn default() -> Self {
        Self {
            mysql: Mysql::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Mysql {
    #[serde(default = "Mysql::default_host")]
    host: String,
    #[serde(default = "Mysql::default_port")]
    port: u16,
    #[serde(default = "Mysql::default_dating")]
    dating: String,
    #[serde(default = "Mysql::default_user")]
    user: String,
    #[serde(default = "Mysql::default_pass")]
    pass: String,
    #[serde(default)]
    connections: Connections,
}

impl Mysql {
    fn default_host() -> String {
        "127.0.0.1".to_string()
    }
    fn default_port() -> u16 {
        3306
    }
    fn default_dating() -> String {
        "default".to_string()
    }
    fn default_user() -> String {
        "root".to_string()
    }
    fn default_pass() -> String {
        "".to_string()
    }
}

impl Default for Mysql {
    fn default() -> Self {
        Self {
            host: Self::default_host(),
            port: Self::default_port(),
            dating: Self::default_dating(),
            user: Self::default_user(),
            pass: Self::default_pass(),
            connections: Connections::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Connections {
    #[serde(default = "Connections::default_max_idle")]
    max_idle: u32,
    #[serde(default = "Connections::default_max_open")]
    max_open: u32,
}

impl Connections {
    fn default_max_idle() -> u32 {
        30
    }
    fn default_max_open() -> u32 {
        30
    }
}

impl Default for Connections {
    fn default() -> Self {
        Self {
            max_idle: Self::default_max_idle(),
            max_open: Self::default_max_open(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Log {
    #[serde(default)]
    app: AppLog,
}

impl Default for Log {
    fn default() -> Self {
        Self {
            app: AppLog::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct AppLog {
    #[serde(default = "AppLog::default_level")]
    level: String,
}

impl AppLog {
    fn default_level() -> String {
        "info".to_string()
    }
}

impl Default for AppLog {
    fn default() -> Self {
        Self {
            level: Self::default_level(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Background {
    #[serde(default)]
    watchdog: Watchdog,
}

impl Default for Background {
    fn default() -> Self {
        Self {
            watchdog: Watchdog::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Watchdog {
    #[serde(default = "Watchdog::default_period")]
    period: String,
    #[serde(default = "Watchdog::default_limit")]
    limit: u32,
    #[serde(default = "Watchdog::default_lock_timeout")]
    lock_timeout: String,
}

impl Watchdog {
    fn default_period() -> String {
        "5s".to_string()
    }
    fn default_limit() -> u32 {
        10
    }
    fn default_lock_timeout() -> String {
        "4s".to_string()
    }
}

impl Default for Watchdog {
    fn default() -> Self {
        Self {
            period: Self::default_period(),
            limit: Self::default_limit(),
            lock_timeout: Self::default_lock_timeout(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::default(),
            server: Server::default(),
            db: Db::default(),
            log: Log::default(),
            background: Background::default(),
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "task_3_9")]
#[command(about = "Prints its configuration to STDOUT")]
struct Cli {
    #[arg(short = 'd', long = "debug")]
    debug: bool,
    #[arg(short = 'c', long = "conf", env = "CONF_FILE", default_value = "config.toml")]
    conf: String,
}

fn load_config(path: &str) -> Result<Config, Box<dyn Error>> {
    let builder = ConfigBuilder::builder()
        // Слой 1: Дефолтные значения
        .add_source(config::Config::try_from(&Config::default())?)
        // Слой 2: Файл конфигурации (если существует)
        .add_source(File::with_name(path).required(false))
        // Слой 3: Переменные окружения с префиксом CONF_
        .add_source(
            Environment::with_prefix("CONF")
                .separator("_")
                .try_parsing(true),
        )
        .build()?;

    let cfg: Config = builder.try_deserialize()?;
    Ok(cfg)
}

fn run() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let mut cfg = load_config(&cli.conf)?;

    // CLI флаг имеет наивысший приоритет
    if cli.debug {
        cfg.mode.debug = true;
    }

    println!("{:#?}", cfg);
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}