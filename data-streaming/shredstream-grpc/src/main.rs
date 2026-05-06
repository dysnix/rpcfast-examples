use anyhow::{Context, Result};
use clap::Parser;
use serde::Deserialize;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashSet;

const VOTE_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("Vote111111111111111111111111111111111111111");

const DEFAULT_BUFFER_SIZE: usize = 4 * 1024 * 1024;

fn default_buffer_size() -> usize {
    DEFAULT_BUFFER_SIZE
}
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use tonic::{
    transport::{Channel, ClientTlsConfig},
    Request,
};
use tracing::{error, info, warn};

mod proto {
    pub mod shredstream {
        include!(concat!(env!("OUT_DIR"), "/shredstream.rs"));
    }
}

use proto::shredstream::{
    shredstream_proxy_client::ShredstreamProxyClient, SubscribeEntriesRequest,
};

#[derive(Parser, Debug)]
#[command(about = "Subscribe to a Jito Shredstream proxy and print filtered transactions")]
struct Cli {
    #[arg(long)]
    config: PathBuf,
}

#[derive(Debug, Deserialize)]
struct Config {
    /// gRPC endpoint of the shredstream proxy, e.g. http://localhost:10100
    endpoint: String,
    /// Optional auth token, sent as the `x-token` metadata header
    #[serde(default)]
    x_token: Option<String>,
    /// Only keep transactions that touch at least one of these accounts
    #[serde(default)]
    account_include: Vec<String>,
    /// Drop transactions that touch any of these accounts
    #[serde(default)]
    account_exclude: Vec<String>,
    /// `true` keeps only vote transactions, `false` drops them, unset keeps both
    #[serde(default)]
    vote: Option<bool>,
    /// tonic channel buffer size in bytes (default: 4 MiB)
    #[serde(default = "default_buffer_size")]
    buffer_size: usize,
}

struct Filters {
    include: Option<HashSet<Pubkey>>,
    exclude: HashSet<Pubkey>,
    vote: Option<bool>,
}

impl Filters {
    fn from_config(cfg: &Config) -> Result<Self> {
        let include = if cfg.account_include.is_empty() {
            None
        } else {
            Some(parse_pubkeys(&cfg.account_include).context("account_include")?)
        };
        let exclude = parse_pubkeys(&cfg.account_exclude).context("account_exclude")?;
        Ok(Self {
            include,
            exclude,
            vote: cfg.vote,
        })
    }

    fn matches(&self, keys: &[Pubkey]) -> bool {
        let is_vote = keys.iter().any(|k| k == &VOTE_PROGRAM_ID);
        match self.vote {
            Some(true) if !is_vote => return false,
            Some(false) if is_vote => return false,
            _ => {}
        }

        if !self.exclude.is_empty() && keys.iter().any(|k| self.exclude.contains(k)) {
            return false;
        }

        if let Some(include) = &self.include {
            if !keys.iter().any(|k| include.contains(k)) {
                return false;
            }
        }

        true
    }
}

fn parse_pubkeys(values: &[String]) -> Result<HashSet<Pubkey>> {
    values
        .iter()
        .map(|s| Pubkey::from_str(s).with_context(|| format!("invalid pubkey: {s}")))
        .collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let raw = std::fs::read_to_string(&cli.config)
        .with_context(|| format!("failed to read config file {}", cli.config.display()))?;
    let config: Config = serde_yaml::from_str(&raw).context("failed to parse config file")?;
    let filters = Filters::from_config(&config)?;

    run(
        &config.endpoint,
        config.x_token.as_deref(),
        config.buffer_size,
        &filters,
    )
    .await
}

async fn run(
    endpoint: &str,
    x_token: Option<&str>,
    buffer_size: usize,
    filters: &Filters,
) -> Result<()> {
    let (proto, rest) = endpoint
        .split_once("://")
        .context("endpoint must look like http(s)://host:port")?;

    let builder = Channel::builder(endpoint.parse()?)
        .tcp_keepalive(Some(Duration::from_secs(30)))
        .buffer_size(buffer_size);

    let channel = match proto {
        "http" => builder.connect().await?,
        "https" => {
            let host = rest.split(':').next().unwrap_or(rest).to_string();
            let tls = ClientTlsConfig::new().domain_name(host).with_enabled_roots();
            builder.tls_config(tls)?.connect().await?
        }
        other => anyhow::bail!("unsupported protocol `{other}`, expected http or https"),
    };

    let token = x_token.map(|t| t.to_string());
    let mut client =
        ShredstreamProxyClient::with_interceptor(channel, move |mut req: Request<()>| {
            if let Some(t) = &token {
                req.metadata_mut().insert(
                    "x-token",
                    t.parse().map_err(|_| {
                        tonic::Status::invalid_argument("x_token is not valid ASCII")
                    })?,
                );
            }
            Ok(req)
        });

    info!("subscribing to {endpoint}");
    let mut stream = client
        .subscribe_entries(SubscribeEntriesRequest {})
        .await?
        .into_inner();

    while let Some(msg) = stream.message().await? {
        let entries = match bincode::deserialize::<Vec<solana_entry::entry::Entry>>(&msg.entries) {
            Ok(e) => e,
            Err(e) => {
                warn!("failed to deserialize entries for slot {}: {e}", msg.slot);
                continue;
            }
        };

        for entry in &entries {
            for tx in &entry.transactions {
                let keys = tx.message.static_account_keys();
                if !filters.matches(keys) {
                    continue;
                }

                let signature = tx
                    .signatures
                    .first()
                    .map(ToString::to_string)
                    .unwrap_or_default();
                info!(slot = msg.slot, signature = %signature, "tx matched");
            }
        }
    }

    error!("stream ended");
    Ok(())
}
