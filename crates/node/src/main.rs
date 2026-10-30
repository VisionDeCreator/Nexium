use anyhow::{ensure, Context, Result};
use axum::{
    extract::{DefaultBodyLimit, Path, State as HttpState},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use clap::{Parser, Subcommand};
use ed25519_dalek::SigningKey;
use nexium_ledger::{Ledger, Status, SyncRequest, SyncResponse, SYNC_PAGE};
use nexium_primitives::*;
use rand::RngCore;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    net::SocketAddr,
    path::{Path as FsPath, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Semaphore;

#[derive(Parser)]
#[command(
    name = "nexium",
    about = "Nexium local development PoW node — no production funds"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write an isolated dev genesis. Its funding key is publicly known (07 repeated).
    Init {
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value_t = 10_000)]
        block_time_ms: u64,
    },
    /// Generate a random Ed25519 key file with owner-only permissions on Unix.
    Keygen {
        #[arg(long)]
        out: PathBuf,
    },
    /// Run a loopback-only node. Static peers are polled and all blocks revalidated.
    Run {
        #[arg(long)]
        genesis: PathBuf,
        #[arg(long)]
        db: PathBuf,
        #[arg(long, default_value = "127.0.0.1:9840")]
        listen: SocketAddr,
        #[arg(long)]
        peer: Vec<String>,
        #[arg(long)]
        mine_to: Option<String>,
    },
    /// Query a node's canonical head and mempool occupancy.
    Status {
        #[arg(long, default_value = "http://127.0.0.1:9840")]
        rpc: String,
    },
    /// Sign locally and submit a native development transfer.
    Transfer {
        #[arg(long, default_value = "http://127.0.0.1:9840")]
        rpc: String,
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        to: String,
        #[arg(long)]
        amount: u64,
        #[arg(long, default_value_t = 1)]
        fee: u64,
        #[arg(long, default_value_t = 100)]
        valid_for: u64,
    },
}

#[derive(Clone)]
struct App {
    ledger: Arc<Mutex<Ledger>>,
    requests: Arc<Semaphore>,
    operations: Arc<Semaphore>,
    stopping: Arc<AtomicBool>,
}

struct ApiError(anyhow::Error);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": self.0.to_string()})),
        )
            .into_response()
    }
}
type ApiResult<T> = std::result::Result<T, ApiError>;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_millis() as u64
}

fn locked<T>(app: &App, f: impl FnOnce(&mut Ledger) -> Result<T>) -> Result<T> {
    let mut ledger = app
        .ledger
        .lock()
        .map_err(|_| anyhow::anyhow!("ledger lock poisoned"))?;
    f(&mut ledger)
}

async fn access<T: Send + 'static>(
    app: App,
    f: impl FnOnce(&mut Ledger) -> Result<T> + Send + 'static,
) -> ApiResult<T> {
    // Retain this permit inside the blocking job even if its HTTP waiter times out.
    // Otherwise canceled requests could accumulate an unbounded queue at the DB mutex.
    let permit = app
        .operations
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError(anyhow::anyhow!("ledger busy; retry later")))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        locked(&app, f)
    })
    .await
    .map_err(|error| ApiError(error.into()))?
    .map_err(ApiError)
}

async fn status(HttpState(app): HttpState<App>) -> ApiResult<Json<Status>> {
    access(app, |ledger| ledger.status()).await.map(Json)
}
async fn account(
    HttpState(app): HttpState<App>,
    Path(address): Path<String>,
) -> ApiResult<Json<Account>> {
    let address = parse_hash(&address).map_err(ApiError)?;
    access(app, move |ledger| ledger.account(address))
        .await
        .map(Json)
}
async fn pending(HttpState(app): HttpState<App>) -> ApiResult<Json<Vec<SignedTransfer>>> {
    access(app, |ledger| Ok(ledger.pending())).await.map(Json)
}
async fn submit(
    HttpState(app): HttpState<App>,
    Json(transaction): Json<SignedTransfer>,
) -> ApiResult<Json<serde_json::Value>> {
    let id = access(app, move |ledger| ledger.submit(transaction)).await?;
    Ok(Json(
        json!({"transaction": hex::encode(id), "admission": "mempool only; inclusion is not guaranteed"}),
    ))
}
async fn block(
    HttpState(app): HttpState<App>,
    Json(block): Json<Block>,
) -> ApiResult<Json<serde_json::Value>> {
    let changed = access(app, move |ledger| ledger.import(block, now_ms())).await?;
    Ok(Json(json!({"tip_changed": changed})))
}
async fn sync(
    HttpState(app): HttpState<App>,
    Json(request): Json<SyncRequest>,
) -> ApiResult<Json<SyncResponse>> {
    access(app, move |ledger| ledger.sync_page(request))
        .await
        .map(Json)
}
async fn limit(
    HttpState(app): HttpState<App>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let Ok(_permit) = app.requests.clone().try_acquire_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tokio::time::timeout(Duration::from_secs(10), next.run(request)).await {
        Ok(response) => response,
        Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
    }
}

fn endpoint(value: &str) -> Result<String> {
    let url = reqwest::Url::parse(value)?;
    ensure!(
        url.scheme() == "http" && url.username().is_empty() && url.password().is_none(),
        "local dev peers require plain HTTP without credentials"
    );
    let host: std::net::IpAddr = url
        .host_str()
        .context("missing host")?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse()
        .context("use a numeric loopback address")?;
    ensure!(
        host.is_loopback(),
        "prototype endpoints must be loopback addresses"
    );
    ensure!(
        url.path() == "/" && url.query().is_none() && url.fragment().is_none(),
        "endpoint must be an origin"
    );
    Ok(value.trim_end_matches('/').into())
}

async fn decode<T: DeserializeOwned>(response: reqwest::Response, max: usize) -> Result<T> {
    let mut response = response.error_for_status()?;
    ensure!(
        response.content_length().is_none_or(|n| n <= max as u64),
        "peer response too large"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes
                .len()
                .checked_add(chunk.len())
                .is_some_and(|n| n <= max),
            "peer response too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(4))
        .connect_timeout(Duration::from_secs(1))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()?)
}

async fn follow_peer(app: App, client: reqwest::Client, peer: String) {
    let mut cursor = None;
    let mut failures = 0u32;
    while !app.stopping.load(Ordering::Relaxed) {
        let result: Result<()> = async {
            let request = locked(&app, |ledger| {
                Ok(SyncRequest {
                    network: ledger.network_id(),
                    locator: ledger.locator(cursor)?,
                })
            })?;
            let response: SyncResponse = decode(
                client
                    .post(format!("{peer}/sync"))
                    .json(&request)
                    .send()
                    .await?,
                16 * 1024 * 1024,
            )
            .await?;
            ensure!(
                response.network == request.network && response.blocks.len() <= SYNC_PAGE,
                "invalid sync response"
            );
            for block in response.blocks {
                let id = block.id();
                let app = app.clone();
                // Execute against validated parent state; never trust a peer's state snapshot.
                tokio::task::spawn_blocking(move || {
                    locked(&app, |ledger| ledger.import(block, now_ms()))
                })
                .await??;
                cursor = Some(id);
            }
            let pending: Vec<SignedTransfer> = decode(
                client.get(format!("{peer}/transactions")).send().await?,
                2 * 1024 * 1024,
            )
            .await?;
            ensure!(
                pending.len() <= nexium_ledger::MAX_POOL_COUNT,
                "peer mempool too large"
            );
            let app = app.clone();
            tokio::task::spawn_blocking(move || {
                locked(&app, |ledger| {
                    for transaction in pending {
                        let _ = ledger.submit(transaction);
                    }
                    Ok(())
                })
            })
            .await??;
            Ok(())
        }
        .await;
        match result {
            Ok(()) => failures = 0,
            Err(error) => {
                failures = failures.saturating_add(1);
                if failures == 1 || failures.is_power_of_two() {
                    eprintln!("peer {peer}: {error:#}");
                }
                cursor = None;
            }
        }
        tokio::time::sleep(Duration::from_millis(if failures == 0 {
            300
        } else {
            1500
        }))
        .await;
    }
}

fn mine(app: App, miner: Address, spacing_ms: u64) -> Result<()> {
    // One worker. Yield between bounded hash chunks; never hold the lock while hashing.
    while !app.stopping.load(Ordering::Relaxed) {
        for _ in 0..spacing_ms.div_ceil(50) {
            if app.stopping.load(Ordering::Relaxed) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut block = locked(&app, |ledger| ledger.template(miner, now_ms()))?;
        while !app.stopping.load(Ordering::Relaxed) {
            if locked(&app, |ledger| {
                Ok(ledger.status()?.tip != block.header.parent)
            })? {
                break;
            }
            if nexium_consensus::mine_chunk(&mut block.header, 256) {
                let id = block.id();
                let height = block.header.height;
                match locked(&app, |ledger| ledger.import(block, now_ms())) {
                    Ok(true) => eprintln!("mined height={height} id={}", hex::encode(id)),
                    Ok(false) => {}
                    Err(error) => eprintln!("candidate rejected: {error:#}"),
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    Ok(())
}

async fn shutdown() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn write_new(path: &FsPath, bytes: &[u8], secret: bool) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if secret { 0o600 } else { 0o644 });
    }
    #[cfg(not(unix))]
    {
        let _ = secret;
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn print<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[tokio::main(worker_threads = 2)]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Init { out, block_time_ms } => {
            let mut genesis = Genesis::development();
            genesis.target_spacing_ms = block_time_ms;
            genesis.validate()?;
            write_new(&out, &serde_json::to_vec_pretty(&genesis)?, false)?;
            print(
                &json!({"genesis": hex::encode(genesis.id()), "funded_address": hex::encode(genesis.allocations[0].0), "development_only": true}),
            )?;
        }
        Command::Keygen { out } => {
            let mut bytes = [0; 32];
            rand::rngs::OsRng.fill_bytes(&mut bytes);
            let key = SigningKey::from_bytes(&bytes);
            write_new(&out, hex::encode(bytes).as_bytes(), true)?;
            print(
                &json!({"address": hex::encode(address(&key.verifying_key().to_bytes())), "key_file": out}),
            )?;
        }
        Command::Status { rpc } => {
            let status: Status = decode(
                client()?
                    .get(format!("{}/status", endpoint(&rpc)?))
                    .send()
                    .await?,
                16 * 1024,
            )
            .await?;
            print(&status)?;
        }
        Command::Transfer {
            rpc,
            key,
            to,
            amount,
            fee,
            valid_for,
        } => {
            let rpc = endpoint(&rpc)?;
            let seed = parse_hash(fs::read_to_string(key)?.trim())?;
            let key = SigningKey::from_bytes(&seed);
            let sender = address(&key.verifying_key().to_bytes());
            let client = client()?;
            let status: Status =
                decode(client.get(format!("{rpc}/status")).send().await?, 16 * 1024).await?;
            let account: Account = decode(
                client
                    .get(format!("{rpc}/account/{}", hex::encode(sender)))
                    .send()
                    .await?,
                16 * 1024,
            )
            .await?;
            let transaction = SignedTransfer::sign(
                Transfer {
                    network: status.network,
                    public_key: key.verifying_key().to_bytes(),
                    recipient: parse_hash(&to)?,
                    amount,
                    fee,
                    nonce: account.nonce,
                    valid_until: status
                        .height
                        .checked_add(valid_for)
                        .context("expiry overflow")?,
                },
                &key,
            )?;
            let response: serde_json::Value = decode(
                client
                    .post(format!("{rpc}/transactions"))
                    .json(&transaction)
                    .send()
                    .await?,
                16 * 1024,
            )
            .await?;
            print(&response)?;
        }
        Command::Run {
            genesis,
            db,
            listen,
            peer,
            mine_to,
        } => {
            ensure!(
                listen.ip().is_loopback(),
                "prototype listener must be loopback-only"
            );
            ensure!(peer.len() <= 16, "at most 16 configured peers");
            let peers: Vec<String> = peer
                .iter()
                .map(|peer| endpoint(peer))
                .collect::<Result<_>>()?;
            let genesis: Genesis = serde_json::from_slice(&fs::read(genesis)?)?;
            let spacing = genesis.target_spacing_ms;
            let miner = mine_to.map(|value| parse_hash(&value)).transpose()?;
            if let Some(parent) = db.parent().filter(|path| !path.as_os_str().is_empty()) {
                fs::create_dir_all(parent)?;
            }
            let app = App {
                ledger: Arc::new(Mutex::new(Ledger::open(&db, genesis)?)),
                requests: Arc::new(Semaphore::new(16)),
                operations: Arc::new(Semaphore::new(16)),
                stopping: Arc::new(AtomicBool::new(false)),
            };
            let listener = tokio::net::TcpListener::bind(listen).await?;
            eprintln!(
                "Nexium DEVELOPMENT ONLY listening={listen} db={} mining={}",
                db.display(),
                miner.is_some()
            );
            print(&locked(&app, |ledger| ledger.status())?)?;
            let client = client()?;
            let mut peers_tasks = vec![];
            for peer in peers {
                peers_tasks.push(tokio::spawn(follow_peer(app.clone(), client.clone(), peer)));
            }
            let mining = miner.map(|miner| {
                let app = app.clone();
                tokio::task::spawn_blocking(move || mine(app, miner, spacing))
            });
            let router = Router::new()
                .route("/status", get(status))
                .route("/account/{address}", get(account))
                .route("/transactions", get(pending).post(submit))
                .route("/blocks", post(block))
                .route("/sync", post(sync))
                .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
                .layer(middleware::from_fn_with_state(app.clone(), limit))
                .with_state(app.clone());
            let stop = app.stopping.clone();
            let result = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    shutdown().await;
                    stop.store(true, Ordering::Relaxed);
                })
                .await;
            app.stopping.store(true, Ordering::Relaxed);
            for task in peers_tasks {
                task.await?;
            }
            if let Some(mining) = mining {
                mining.await??;
            }
            result?;
        }
    }
    Ok(())
}
