use chrono::Utc;
use futures::executor::block_on;
use local_ip_address::local_ip;
use log::{Level, Log, Metadata, Record};
use rustls_pki_types::ServerName;
use std::env;
use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::sync::OnceCell;
use tokio::time;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;
use url::Url;

pub static LOGGER: OnceCell<SilatusLogger> = OnceCell::const_new();
pub static IP_ADDR: OnceCell<IpAddr> = OnceCell::const_new();

struct IpAddrWrapper {}
impl IpAddrWrapper {
    pub async fn init() -> IpAddr {
        IpAddrWrapper::fetch_ip_address().await
    }

    async fn fetch_ip_address() -> IpAddr {
        if let Ok(response) = reqwest::get("https://api.ipify.org").await {
            if let Ok(text) = response.text().await {
                if let Ok(ip_addr) = IpAddr::from_str(&text) {
                    return ip_addr;
                }
            }
        }

        local_ip().unwrap_or_else(|_| IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)))
    }
}

pub async fn get_ip_addr() -> &'static IpAddr {
    IP_ADDR
        .get_or_init(|| async { IpAddrWrapper::init().await })
        .await
}

pub struct SilatusLogger {
    stream: Option<Arc<Mutex<BufWriter<tokio_rustls::client::TlsStream<TcpStream>>>>>,
    log_level: Level,
}

pub async fn get_logger_instance() -> &'static SilatusLogger {
    LOGGER
        .get_or_init(|| async { SilatusLogger::new().await.expect("Failed to start logger") })
        .await
}

impl SilatusLogger {
    pub async fn new() -> Result<SilatusLogger, Box<dyn std::error::Error>> {
        println!("Setting up logger...");
        IpAddrWrapper::init().await;

        let log_level = env::var("LOG_LEVEL").unwrap_or("info".to_string());
        let log_level = match log_level.to_lowercase().as_str() {
            "error" => Level::Error,
            "warn" => Level::Warn,
            "info" => Level::Info,
            "debug" => Level::Debug,
            "trace" => Level::Trace,
            _ => Level::Info,
        };

        if env::var("PAPERTRAIL_URL").is_err() {
            println!("No Papertrail URL...");

            return Ok(SilatusLogger {
                stream: None,
                log_level,
            });
        }

        let papertrail_url = env::var("PAPERTRAIL_URL")?;

        let pem_data = reqwest::get("https://papertrailapp.com/tools/papertrail-bundle.pem")
            .await?
            .bytes()
            .await?;

        let mut root_cert_store = RootCertStore::empty();

        let buf_reader = &mut std::io::BufReader::new(pem_data.as_ref());
        // Load certificates from the PEM
        let additional_certs = rustls_pemfile::certs(buf_reader);
        for cert in additional_certs {
            root_cert_store.add(cert?)?;
        }

        let config = ClientConfig::builder()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth();

        let url = Url::parse(&papertrail_url)?;
        let host = url.host_str().ok_or(url::ParseError::IdnaError)?.to_owned();
        let host_with_port = match url.port() {
            Some(port) => format!("{}:{}", host, port),
            None => host.to_string(),
        };

        let connector = TlsConnector::from(Arc::new(config));
        let dnsname = ServerName::try_from(host).unwrap();

        let stream = TcpStream::connect(&host_with_port).await?;
        let tls_stream = connector.connect(dnsname, stream).await?;

        let buffered = Arc::new(Mutex::new(BufWriter::new(tls_stream)));

        Ok(SilatusLogger {
            stream: Some(buffered),
            log_level,
        })
    }

    fn buffer_size(&self) -> usize {
        if let Some(ref stream_mutex) = self.stream {
            if let Ok(stream) = stream_mutex.try_lock() {
                return stream.buffer().len();
            }
        }
        0
    }

    pub async fn periodic_flush_and_check(&self) {
        // Buffer check every second
        let mut buffer_check_interval = time::interval(Duration::from_secs(1));
        // Forced flush every 30 seconds
        let mut flush_interval = time::interval(Duration::from_secs(30));
        // 5Kb buffer
        let buffer_size_threshold = 1024 * 5;

        loop {
            tokio::select! {
                _ = buffer_check_interval.tick() => {
                    if self.buffer_size() >= buffer_size_threshold {
                        self.flush();
                    }
                },
                _ = flush_interval.tick() => {
                    if self.buffer_size() > 0 {
                        self.flush();
                    }
                },
            }
        }
    }

    async fn format_syslog_message<'a>(&self, record: &Record<'a>) -> String {
        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S%.fZ").to_string();
        let ip_address = get_ip_addr().await;
        let app_name = "datum"; // Replace with your application name

        let severity = Self::map_log_level_to_syslog_severity(record.level());
        let facility = 1; // Set your facility code here

        format!(
            "<{}>1 {} {} {} - - - {} {}\n",
            facility * 8 + severity,
            timestamp,
            ip_address,
            app_name,
            record.level().as_str(),
            record.args()
        )
    }

    fn map_log_level_to_syslog_severity(level: Level) -> i32 {
        match level {
            Level::Error => 3, // Error
            Level::Warn => 4,  // Warning
            Level::Info => 6,  // Informational
            Level::Debug => 7, // Debug
            Level::Trace => 7, // Trace
        }
    }
}

impl Log for SilatusLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.log_level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        // Local logging
        match record.level() {
            Level::Error | Level::Warn => eprintln!("{}", record.args()),
            _ => println!("{}", record.args()),
        }

        // Papertrail logging for Warning and above
        if record.level() >= Level::Warn {
            if let Some(arc_stream_mutex) = &self.stream {
                let arc_stream_mutex = Arc::clone(arc_stream_mutex);
                let message = block_on(self.format_syslog_message(record));
                tokio::spawn(async move {
                    let mut stream = match tokio::time::timeout(
                        Duration::from_millis(500),
                        arc_stream_mutex.lock(),
                    )
                    .await
                    {
                        Ok(stream) => stream,
                        Err(_) => {
                            eprintln!(
                                "Failed to lock stream for Papertrail logging within timeout"
                            );
                            return;
                        }
                    };

                    if let Err(e) = stream.write(message.as_bytes()).await {
                        eprintln!("Failed to send log to Papertrail: {}", e);
                    }
                });
            }
        }
    }

    fn flush(&self) {
        if let Some(ref stream_mutex) = self.stream {
            let stream_mutex = Arc::clone(stream_mutex);
            tokio::spawn(async move {
                let mut stream =
                    match tokio::time::timeout(Duration::from_millis(500), stream_mutex.lock())
                        .await
                    {
                        Ok(stream) => stream,
                        Err(_) => {
                            eprintln!("Failed to lock stream for flushing within timeout");
                            return;
                        }
                    };
                if let Err(e) = stream.flush().await {
                    eprintln!("Flushing error occurred: {}", e);
                }
            });
        }
    }
}
