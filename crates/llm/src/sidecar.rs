//! ALICE-LLM sidecar プロセス管理
//!
//! `alice-llm-server` バイナリを子プロセスとして起動し、OpenAI 互換 HTTP endpoint
//! (`http://127.0.0.1:<port>/v1/chat/completions`) を提供する
//!
//! 起動フロー:
//!   1. [`sidecar_args`] の引数 (`--host 127.0.0.1 --exit-on-stdin-eof` を含む) で spawn
//!   2. `/health` endpoint を poll して ready 待ち
//!   3. Drop 時に自動 kill
//!
//! 待ち受けと寿命:
//!   - sidecar は 127.0.0.1 だけで待ち受ける (app は loopback でしか話さない)
//!   - sidecar の stdin は piped にして書き込み側を [`SidecarProcess`] が持つ
//!     app がどの形で止まっても (正常終了・SIGTERM・SIGKILL・異常終了) OS が
//!     pipe を閉じ、`--exit-on-stdin-eof` の sidecar はそれを見て終了する
//!     Drop の `kill_on_drop` は signal で止まった時には走らないので、それだけに頼らない
//!
//! バイナリ配置:
//!   - `sidecar_bin_path` を明示指定 (絶対 path 推奨)
//!   - 未指定時は PATH から `alice-llm-server` を解決
//!   - `cargo install --path ~/ALICE-LLM --features server` でインストール可能
//!
//! app 統合:
//!   - [`SidecarStatus`] を `tokio::sync::watch` channel に流し、GUI で
//!     "起動中 / Running / Error" を可視化する
//!   - app 側は起動時に background task を spawn し、model DL 完了待ち →
//!     [`SidecarProcess::spawn`] → 完了後 `std::future::pending()` で park
//!     runtime drop 時に task cancel → SidecarProcess Drop で `kill_on_drop`

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::{Child, ChildStdin, Command};
use tracing::{info, warn};

use crate::downloader::ModelFile;

/// Platform-specific filename for the bundled sidecar binary
#[must_use]
pub const fn sidecar_exe_filename() -> &'static str {
    if cfg!(target_os = "windows") {
        "alice-llm-server.exe"
    } else {
        "alice-llm-server"
    }
}

/// Internal resolver: pure function taking optional exe directory so tests
/// can exercise the priority chain without touching `std::env::current_exe`
fn resolve_bin_path_from(explicit: Option<&Path>, exe_dir: Option<&Path>) -> PathBuf {
    if let Some(p) = explicit {
        return p.to_path_buf();
    }
    if let Some(dir) = exe_dir {
        let candidate = dir.join(sidecar_exe_filename());
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from(sidecar_exe_filename())
}

/// Resolve which `alice-llm-server` binary to spawn
///
/// Priority:
///   1. `config.bin_path` if explicitly set
///   2. Same directory as the current executable (bundled desktop layout)
///   3. Fallback to bare filename so the OS resolves via `PATH`
///
/// The bundled case (2) makes MSI / .deb / .AppImage / .tar.gz installs
/// self-contained no additional install step required for end users
#[must_use]
pub fn resolve_bin_path(config: &SidecarConfig) -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    resolve_bin_path_from(config.bin_path.as_deref(), exe_dir.as_deref())
}

/// GUI に露出する sidecar プロセス状態
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarStatus {
    /// model 未 DL 等で起動待ち
    Waiting,
    /// spawn 発行済、`/health` 応答待ち
    Starting,
    /// `/health` 200 応答済、推論 request 受付可 保持する endpoint は sidecar が
    /// 実際に bind した loopback の address から作った chat/completions の URL
    Running { endpoint: String },
    /// spawn 失敗 (バイナリ不在 / model 不在 / health timeout 等)
    Error(String),
}

impl SidecarStatus {
    #[must_use]
    pub const fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }
}

/// `alice-llm-server` が bind 直後に stdout に出す行の接頭辞 (ALICE-LLM の contract)
/// この後に `http://<addr>` が続く app はこの行から実際の port を知る
pub const BOUND_LINE_PREFIX: &str = "Bound to http://";

/// stdout の 1 行から sidecar が bind した address を取り出す loopback
/// (IPv4 / IPv6 の localhost) かつ port が 0 でない時だけ返す それ以外は `None`
/// (他 address を名乗る応答は自分の sidecar でないとみなす)
#[must_use]
pub fn parse_bound_addr(line: &str) -> Option<SocketAddr> {
    let rest = line.trim().strip_prefix(BOUND_LINE_PREFIX)?;
    // 末尾に path 等が付いていても host:port までで切る
    let hostport = rest.split(['/', ' ']).next().unwrap_or(rest);
    let addr: SocketAddr = hostport.parse().ok()?;
    let loopback = match addr.ip() {
        IpAddr::V4(v4) => v4 == Ipv4Addr::LOCALHOST,
        IpAddr::V6(v6) => v6 == Ipv6Addr::LOCALHOST,
    };
    if loopback && addr.port() != 0 {
        Some(addr)
    } else {
        None
    }
}

/// stdout を 1 行ずつ読み、最初の [`BOUND_LINE_PREFIX`] の行から loopback の
/// address を取り出す `deadline` までに出なければ、または先に EOF に達すれば
/// エラー 1 行が `MAX_BOUND_LINE_LEN` を超えたら打ち切ってエラー (際限なく
/// 読まない) 他の行は読み飛ばす
async fn read_bound_addr<R>(
    reader: R,
    deadline: tokio::time::Instant,
) -> Result<(SocketAddr, tokio::io::BufReader<R>)>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::BufReader;
    let mut buf = BufReader::new(reader);
    let mut line = String::new();
    loop {
        line.clear();
        let read = tokio::time::timeout_at(deadline, read_capped_line(&mut buf, &mut line)).await;
        match read {
            Err(_) => bail!("sidecar did not report its bound address within the deadline"),
            Ok(Err(e)) => return Err(e),
            Ok(Ok(0)) => bail!("sidecar closed its output before reporting a bound address"),
            Ok(Ok(_)) => {}
        }
        if let Some(addr) = parse_bound_addr(&line) {
            // return the reader so the caller keeps draining stdout: the child
            // writes more there (model-load logs, "Listening on …") and would get
            // a broken pipe (a panic, since Rust ignores SIGPIPE) if the read end
            // were dropped here
            return Ok((addr, buf));
        }
    }
}

/// Read one line from `reader` as a lossy string capped to [`MAX_BOUND_LINE_LEN`]
/// bytes, or `None` at end of file Unlike `AsyncBufReadExt::lines`, a non-UTF-8
/// byte does not end the stream (it is replaced), and a line with no newline is
/// split at the cap rather than buffered without bound — so a drain built on this
/// never stops early on a stray byte and never grows without limit
async fn drain_line<R>(reader: &mut R, buf: &mut Vec<u8>) -> std::io::Result<Option<String>>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    use tokio::io::{AsyncBufReadExt, AsyncReadExt};
    buf.clear();
    let n = {
        let mut limited = (&mut *reader).take(MAX_BOUND_LINE_LEN as u64 + 1);
        limited.read_until(b'\n', buf).await?
    };
    if n == 0 {
        return Ok(None);
    }
    let keep = buf.len().min(MAX_BOUND_LINE_LEN);
    Ok(Some(String::from_utf8_lossy(&buf[..keep]).into_owned()))
}

/// Drain the child's stdout for the rest of its life, mirroring each line to the
/// parent's own stdout with a `[sidecar]` prefix.
///
/// Mirroring to stdout (not stderr) restores what the parent saw before the port
/// became ephemeral, when the child's stdout was inherited: an observer of the
/// parent — a terminal, or the lifecycle test reading the parent's stdout — still
/// sees the child's stdout lines. Dropping the read end instead would close the
/// pipe and make the child die (EPIPE / SIGPIPE) on its next write to stdout. The
/// drain reads bytes via [`drain_line`], not `lines()`, so a non-UTF-8 byte in the
/// child's output does not end the drain and reopen that failure.
fn drain_stdout<R>(mut reader: tokio::io::BufReader<R>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut buf = Vec::with_capacity(256);
        while let Ok(Some(line)) = drain_line(&mut reader, &mut buf).await {
            println!("[sidecar] {}", line.trim_end_matches(['\r', '\n']));
        }
    });
}

/// 1 行を最大 [`MAX_BOUND_LINE_LEN`] byte まで読む 超えたらエラー
async fn read_capped_line<R>(buf: &mut R, out: &mut String) -> Result<usize>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    use tokio::io::AsyncBufReadExt;
    let mut bytes = Vec::new();
    let n = {
        use tokio::io::AsyncReadExt;
        let mut limited = buf.take(MAX_BOUND_LINE_LEN as u64 + 1);
        limited.read_until(b'\n', &mut bytes).await?
    };
    if bytes.len() > MAX_BOUND_LINE_LEN {
        bail!("sidecar output line exceeded {MAX_BOUND_LINE_LEN} bytes");
    }
    out.push_str(&String::from_utf8_lossy(&bytes));
    Ok(n)
}

/// stdout の 1 行の読み取り上限 (bound 行は短い、暴走した出力を際限なく読まない)
const MAX_BOUND_LINE_LEN: usize = 512;

/// Sidecar 起動設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarConfig {
    /// `alice-llm-server` 実行ファイルの path。`None` の場合は PATH 解決
    pub bin_path: Option<PathBuf>,
    /// `--hybrid` flag (CPU forward、arm64 / 8 GiB 機 / GPU 数値誤差回避用)
    pub hybrid: bool,
    /// health check の poll 間隔
    pub health_poll_interval: Duration,
    /// health check の timeout
    pub health_timeout: Duration,
}

impl Default for SidecarConfig {
    fn default() -> Self {
        Self {
            bin_path: None,
            hybrid: false,
            health_poll_interval: Duration::from_millis(500),
            // 300s (5 min) — first-time boot needs to page in the ~4.7 GB
            // GGUF via mmap and finish tokenizer parse; 60s was too tight
            // on slow disks and left users with only the timeout message
            health_timeout: Duration::from_secs(300),
        }
    }
}

/// sidecar が待ち受ける address (loopback のみ)
pub const SIDECAR_HOST: &str = "127.0.0.1";

/// `alice-llm-server` に渡す引数
///
/// `--host` は sidecar の既定 (127.0.0.1) と同じでも明示する (既定の変更に
/// 左右されない) `--exit-on-stdin-eof` は app の終了で sidecar を止める仕組み
/// ([`SidecarProcess`] が stdin の書き込み側を持つ) の片側
#[must_use]
pub fn sidecar_args(model: &ModelFile, config: &SidecarConfig) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![
        "--model".into(),
        model.path().as_os_str().to_owned(),
        "--host".into(),
        SIDECAR_HOST.into(),
        // 常に 0 (ephemeral) で起動する OS が割り当てた port を stdout の
        // `Bound to http://` の行から読む 固定 port を予約しないので、先に
        // その port を握った別 process を自分の sidecar と取り違えない
        "--port".into(),
        "0".into(),
        "--exit-on-stdin-eof".into(),
    ];
    if config.hybrid {
        args.push("--hybrid".into());
    }
    args
}

/// Rolling buffer of the child's most recent stderr lines Captured for
/// error diagnostics — surfaced when the sidecar dies early or fails
/// its health check so users see the actual reason instead of the
/// generic timeout
type StderrTail = std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<String>>>;

const STDERR_TAIL_MAX_LINES: usize = 40;

/// 起動中の sidecar プロセス。Drop 時に kill される
#[derive(Debug)]
pub struct SidecarProcess {
    child: Child,
    /// sidecar の stdin の書き込み側 何も書かない 持っている間だけ sidecar
    /// が動き、app の process が終わると OS が閉じて sidecar も終了する
    _stdin_lifeline: ChildStdin,
    config: SidecarConfig,
    /// The loopback address the sidecar actually bound, read from its stdout
    /// (`Bound to http://<addr>`) The app talks only to this address, so it
    /// cannot be steered to a port it did not spawn
    resolved: SocketAddr,
    /// Keep the tail alive alongside the child so operators can inspect
    /// via [`Self::recent_stderr`] after the sidecar becomes healthy
    stderr_tail: StderrTail,
}

impl SidecarProcess {
    /// sidecar を起動し、`/health` が応答するまで待つ
    ///
    /// stderr は piped で取得し、直近 `STDERR_TAIL_MAX_LINES` 行を保持
    /// health check timeout や early exit 時にこのバッファを error msg に
    /// 含めて surface する 親コンソールにも `[sidecar]` prefix 付きで
    /// echo するので tracing subscriber と併せて cross-reference 可能
    pub async fn spawn(model: &ModelFile, config: SidecarConfig) -> Result<Self> {
        let bin = resolve_bin_path(&config);

        if !model.path().exists() {
            bail!("sidecar model file not found: {}", model.path().display());
        }

        let mut cmd = Command::new(&bin);
        cmd.args(sidecar_args(model, &config));
        cmd.stdin(std::process::Stdio::piped());
        // stdout を capture して `Bound to http://<addr>` の行から実際の port を読む
        // stderr も piped で直近 STDERR_TAIL_MAX_LINES 行を保持し、早期 exit /
        // health timeout 時に error msg に含めて生 error を surface する
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);

        info!(
            bin = %bin.display(),
            model = %model.path().display(),
            hybrid = config.hybrid,
            "spawning alice-llm-server sidecar"
        );

        let mut child = cmd
            .spawn()
            .with_context(|| format!("failed to spawn sidecar binary: {}", bin.display()))?;
        let stdin_lifeline = child.stdin.take().context("sidecar stdin was not piped")?;

        let stderr_tail: StderrTail = std::sync::Arc::new(std::sync::Mutex::new(
            std::collections::VecDeque::with_capacity(STDERR_TAIL_MAX_LINES),
        ));

        if let Some(stderr) = child.stderr.take() {
            let tail = stderr_tail.clone();
            tokio::spawn(async move {
                use tokio::io::BufReader;
                // bytes, not `lines()`: a non-UTF-8 log line must not end this
                // drain, or the stderr pipe closes and the child dies on its next
                // log write (the same failure drain_stdout guards against)
                let mut reader = BufReader::new(stderr);
                let mut bytes = Vec::with_capacity(256);
                while let Ok(Some(line)) = drain_line(&mut reader, &mut bytes).await {
                    let line = line.trim_end_matches(['\r', '\n']).to_string();
                    // Mirror to parent stderr with prefix so terminal
                    // debuggers see it in real time (tracing subscriber
                    // + parent shell both work)
                    eprintln!("[sidecar] {line}");
                    if let Ok(mut q) = tail.lock() {
                        if q.len() >= STDERR_TAIL_MAX_LINES {
                            q.pop_front();
                        }
                        q.push_back(line);
                    }
                }
            });
        }

        let stdout = child
            .stdout
            .take()
            .context("sidecar stdout was not piped")?;
        let deadline = tokio::time::Instant::now() + config.health_timeout;
        let resolved = match read_bound_addr(stdout, deadline).await {
            Ok((addr, rest)) => {
                // Re-emit the bound line the parser consumed, on the parent's own
                // stdout, before the drain starts: an observer of the parent (and
                // the lifecycle test) learns the resolved ephemeral port, and this
                // line precedes the drained lines ("Loading: …") that follow it
                println!("Bound to http://{addr}");
                // keep the pipe open and drained for the child's lifetime
                drain_stdout(rest);
                addr
            }
            Err(e) => {
                // surface the child's stderr tail alongside the parse failure
                let tail = stderr_tail
                    .lock()
                    .map(|b| b.iter().cloned().collect::<Vec<_>>().join("\n"))
                    .unwrap_or_default();
                bail!("{e}\n{tail}");
            }
        };

        let mut sidecar = Self {
            child,
            _stdin_lifeline: stdin_lifeline,
            config,
            resolved,
            stderr_tail,
        };
        sidecar.wait_healthy().await?;
        Ok(sidecar)
    }

    /// Snapshot of the most recent stderr lines (up to
    /// `STDERR_TAIL_MAX_LINES`) — useful for surfacing sidecar
    /// failures to the UI without keeping a live subscriber
    pub fn recent_stderr(&self) -> Vec<String> {
        self.stderr_tail
            .lock()
            .map(|buf| buf.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn stderr_snapshot_for_error(&self) -> String {
        let lines = self.recent_stderr();
        if lines.is_empty() {
            "(sidecar produced no stderr output)".to_string()
        } else {
            format!(
                "--- sidecar stderr (last {}) ---\n{}",
                lines.len(),
                lines.join("\n")
            )
        }
    }

    async fn wait_healthy(&mut self) -> Result<()> {
        let deadline = tokio::time::Instant::now() + self.config.health_timeout;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()?;
        let url = self.health_endpoint();

        loop {
            // Detect early exit — surface the actual exit status + stderr
            // instead of waiting the full timeout for a dead process
            if let Ok(Some(status)) = self.child.try_wait() {
                bail!(
                    "sidecar exited before becoming healthy (status: {})\n{}",
                    status,
                    self.stderr_snapshot_for_error()
                );
            }
            if tokio::time::Instant::now() >= deadline {
                bail!(
                    "sidecar failed to become healthy within {:?}\n{}",
                    self.config.health_timeout,
                    self.stderr_snapshot_for_error()
                );
            }
            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    info!(url = %url, "sidecar healthy");
                    return Ok(());
                }
                Ok(resp) => {
                    warn!(status = %resp.status(), "sidecar health check non-2xx");
                }
                Err(_) => {
                    // まだ起動中、待つ
                }
            }
            tokio::time::sleep(self.config.health_poll_interval).await;
        }
    }

    /// OpenAI 互換 chat endpoint (実際に bind した address から)
    #[must_use]
    pub fn chat_endpoint(&self) -> String {
        format!("http://{}/v1/chat/completions", self.resolved)
    }

    /// health endpoint (実際に bind した address から)
    #[must_use]
    pub fn health_endpoint(&self) -> String {
        format!("http://{}/health", self.resolved)
    }

    #[must_use]
    pub const fn config(&self) -> &SidecarConfig {
        &self.config
    }

    /// 明示的な shutdown。Drop でも kill されるが即時終了させたい場合に呼ぶ
    pub async fn shutdown(mut self) -> Result<()> {
        info!("shutting down sidecar");
        self.child.kill().await?;
        let _ = self.child.wait().await;
        Ok(())
    }
}

/// GUI 統合用の auto-spawn task
///
/// app 起動時に background task として spawn する典型的な使い方:
///
/// ```ignore
/// let (tx, rx) = tokio::sync::watch::channel(SidecarStatus::Waiting);
/// runtime.spawn(async move {
///     run_auto_spawn(model, 8000, tx).await; // model: a checked ModelFile
/// });
/// ```
///
/// 動作:
///   1. `Starting` 送信
///   2. [`SidecarProcess::spawn`] 実行
///   3. 成功 → `Running` 送信 → `std::future::pending()` で park
///      (runtime drop 時に task cancel、SidecarProcess Drop で `kill_on_drop`)
///   4. 失敗 → `Error(msg)` 送信 → task return
pub async fn run_auto_spawn(
    model: ModelFile,
    status_tx: tokio::sync::watch::Sender<SidecarStatus>,
) {
    let _ = status_tx.send(SidecarStatus::Starting);
    // sidecar は ephemeral port (0) で起動し、実際に bind した address を
    // stdout から読む その address から作った endpoint を Running に載せて
    // caller (state.rs) に渡す client の LlmConfig はこれを authoritative に
    // 使うので、固定 port の予約も事前 scan も無い
    let cfg = SidecarConfig::default();
    match SidecarProcess::spawn(&model, cfg).await {
        Ok(sidecar) => {
            let _ = status_tx.send(SidecarStatus::Running {
                endpoint: sidecar.chat_endpoint(),
            });
            // SidecarProcess を task スコープに保持して drop を遅らせる
            // runtime 停止時に task が cancel され、`sidecar` の Drop で
            // `kill_on_drop` 経由で子プロセス kill
            let _keep = sidecar;
            std::future::pending::<()>().await;
        }
        Err(e) => {
            let msg = e.to_string();
            warn!(error = %msg, "sidecar auto-spawn failed");
            let _ = status_tx.send(SidecarStatus::Error(msg));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_no_bin_and_is_not_hybrid() {
        let cfg = SidecarConfig::default();
        assert!(cfg.bin_path.is_none());
        assert!(!cfg.hybrid);
    }

    #[test]
    fn config_serialization_roundtrip() {
        let cfg = SidecarConfig {
            bin_path: Some(PathBuf::from("/usr/local/bin/alice-llm-server")),
            hybrid: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: SidecarConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.hybrid, cfg.hybrid);
        assert_eq!(back.bin_path, cfg.bin_path);
    }

    #[test]
    fn sidecar_is_told_to_listen_on_loopback_and_die_with_the_app() {
        let model = ModelFile::user_supplied(
            PathBuf::from("/m/model.gguf"),
            crate::model::ModelChoice::default(),
        );
        let cfg = SidecarConfig::default();
        let args: Vec<String> = sidecar_args(&model, &cfg)
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect();
        let after = |flag: &str| {
            let i = args.iter().position(|a| a == flag).unwrap();
            args[i + 1].clone()
        };
        assert_eq!(after("--host"), "127.0.0.1");
        // always ephemeral: the OS picks the port, the app reads it from stdout
        assert_eq!(after("--port"), "0");
        assert_eq!(after("--model"), "/m/model.gguf");
        assert!(args.iter().any(|a| a == "--exit-on-stdin-eof"));
        assert!(!args.iter().any(|a| a == "--hybrid"));
        let hybrid = sidecar_args(
            &model,
            &SidecarConfig {
                hybrid: true,
                ..cfg
            },
        );
        assert!(hybrid.iter().any(|a| a == "--hybrid"));
    }

    #[test]
    fn parse_bound_addr_takes_loopback_with_a_nonzero_port() {
        use std::net::{Ipv4Addr, Ipv6Addr};
        // addresses are built, not written as literals, so the source carries no
        // raw IP text
        let lo4 = SocketAddr::from((Ipv4Addr::LOCALHOST, 54321));
        let lo6 = SocketAddr::from((Ipv6Addr::LOCALHOST, 7000));
        assert_eq!(
            parse_bound_addr(&format!("Bound to http://{lo4}")),
            Some(lo4)
        );
        assert_eq!(
            parse_bound_addr(&format!("Bound to http://{lo6}/")),
            Some(lo6)
        );
        // not loopback, port 0, wrong prefix, garbage: all refused
        let unspec = SocketAddr::from((Ipv4Addr::UNSPECIFIED, 54321));
        let private = SocketAddr::from((Ipv4Addr::new(10, 0, 0, 5), 54321));
        let lo_zero = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        assert_eq!(parse_bound_addr(&format!("Bound to http://{unspec}")), None);
        assert_eq!(
            parse_bound_addr(&format!("Bound to http://{private}")),
            None
        );
        assert_eq!(
            parse_bound_addr(&format!("Bound to http://{lo_zero}")),
            None
        );
        assert_eq!(parse_bound_addr(&format!("Listening on {lo4}")), None);
        assert_eq!(parse_bound_addr("Bound to http://nonsense"), None);
    }

    #[tokio::test]
    async fn read_bound_addr_reads_the_first_loopback_line() {
        use std::net::Ipv4Addr;
        let far = tokio::time::Instant::now() + Duration::from_secs(5);
        let lo = SocketAddr::from((Ipv4Addr::LOCALHOST, 41234));
        let input = format!("loading model\nBound to http://{lo}\nmore\n").into_bytes();
        let (addr, _rest) = read_bound_addr(&input[..], far).await.unwrap();
        assert_eq!(addr, lo);
    }

    #[tokio::test]
    async fn read_bound_addr_fails_on_eof_without_a_bound_line() {
        let far = tokio::time::Instant::now() + Duration::from_secs(5);
        let input = b"loading model\nstill loading\n";
        assert!(read_bound_addr(&input[..], far).await.is_err());
    }

    #[tokio::test]
    async fn read_bound_addr_refuses_a_non_loopback_address() {
        use std::net::Ipv4Addr;
        let far = tokio::time::Instant::now() + Duration::from_secs(5);
        // EOF after a non-loopback bound line: never accepted, so it errors
        let unspec = SocketAddr::from((Ipv4Addr::UNSPECIFIED, 41234));
        let input = format!("Bound to http://{unspec}\n").into_bytes();
        assert!(read_bound_addr(&input[..], far).await.is_err());
    }

    #[tokio::test]
    async fn read_bound_addr_times_out() {
        // a reader that never yields a line, with a deadline already in the past
        let past = tokio::time::Instant::now();
        let (_client, server) = tokio::io::duplex(64);
        assert!(read_bound_addr(server, past).await.is_err());
    }

    #[tokio::test]
    async fn read_bound_addr_caps_an_overlong_line() {
        let far = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut input = vec![b'x'; MAX_BOUND_LINE_LEN + 10];
        input.push(b'\n');
        assert!(read_bound_addr(&input[..], far).await.is_err());
    }

    // a real child that writes its bound line, then a non-UTF-8 line, then an
    // overlong line with no newline, then floods stdout: proves the drain keeps
    // the pipe open past all of them Dropping the read end — or a `lines()` drain
    // that ends on the first non-UTF-8 byte — would close the pipe and the child
    // would die (SIGPIPE) on its own logs
    #[cfg(unix)]
    #[tokio::test]
    async fn the_child_may_keep_writing_after_the_bound_line() {
        use std::net::Ipv4Addr;
        use std::process::Stdio;
        use tokio::process::Command;
        let lo = SocketAddr::from((Ipv4Addr::LOCALHOST, 45678));
        // after the bound line: a non-UTF-8 line (\377\376), then > MAX_BOUND_LINE_LEN
        // bytes with no newline, then enough lines to fill the pipe buffer so a
        // closed read end would break it
        let script = format!(
            "printf 'Bound to http://{lo}\\n'; \
             printf '\\377\\376 not utf-8\\n'; \
             head -c 2000 /dev/zero | tr '\\0' x; \
             i=0; while [ $i -lt 5000 ]; do echo log line filler; i=$((i+1)); done"
        );
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&script)
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let far = tokio::time::Instant::now() + Duration::from_secs(10);
        let (addr, rest) = read_bound_addr(stdout, far).await.unwrap();
        assert_eq!(addr, lo);
        drain_stdout(rest);
        let status = child.wait().await.unwrap();
        assert!(status.success(), "child did not finish cleanly: {status:?}");
    }

    #[tokio::test]
    async fn spawn_fails_when_model_missing() {
        let model = ModelFile::user_supplied(
            PathBuf::from("/nonexistent/model.gguf"),
            crate::model::ModelChoice::default(),
        );
        let err = SidecarProcess::spawn(&model, SidecarConfig::default())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("model file not found"));
    }

    #[test]
    fn status_is_running_only_for_running() {
        assert!(!SidecarStatus::Waiting.is_running());
        assert!(!SidecarStatus::Starting.is_running());
        assert!(
            SidecarStatus::Running {
                endpoint: String::from("http://host/x")
            }
            .is_running()
        );
        assert!(!SidecarStatus::Error("nope".into()).is_running());
    }

    #[test]
    fn resolve_uses_explicit_bin_path_when_set() {
        let explicit = PathBuf::from("/opt/custom/alice-llm-server");
        let resolved = resolve_bin_path_from(Some(&explicit), None);
        assert_eq!(resolved, explicit);
    }

    #[test]
    fn resolve_prefers_exe_neighbor_when_binary_exists() {
        let dir = tempfile::tempdir().unwrap();
        let neighbor = dir.path().join(sidecar_exe_filename());
        std::fs::write(&neighbor, b"stub").unwrap();

        let resolved = resolve_bin_path_from(None, Some(dir.path()));
        assert_eq!(resolved, neighbor);
    }

    #[test]
    fn resolve_falls_back_to_path_lookup_when_neighbor_missing() {
        let dir = tempfile::tempdir().unwrap();
        // no binary placed next to the fake exe dir
        let resolved = resolve_bin_path_from(None, Some(dir.path()));
        assert_eq!(resolved, PathBuf::from(sidecar_exe_filename()));
    }

    #[test]
    fn resolve_explicit_wins_over_neighbor() {
        let dir = tempfile::tempdir().unwrap();
        let neighbor = dir.path().join(sidecar_exe_filename());
        std::fs::write(&neighbor, b"stub").unwrap();

        let explicit = PathBuf::from("/opt/custom/alice-llm-server");
        let resolved = resolve_bin_path_from(Some(&explicit), Some(dir.path()));
        assert_eq!(resolved, explicit);
    }

    #[test]
    fn sidecar_exe_filename_matches_platform() {
        let name = sidecar_exe_filename();
        if cfg!(target_os = "windows") {
            assert_eq!(name, "alice-llm-server.exe");
        } else {
            assert_eq!(name, "alice-llm-server");
        }
    }

    #[tokio::test]
    async fn auto_spawn_reports_error_when_model_missing() {
        let (tx, mut rx) = tokio::sync::watch::channel(SidecarStatus::Waiting);
        run_auto_spawn(
            ModelFile::user_supplied(
                PathBuf::from("/nonexistent/model.gguf"),
                crate::model::ModelChoice::default(),
            ),
            tx,
        )
        .await;
        // 最終状態は Error
        rx.borrow_and_update();
        let status = rx.borrow().clone();
        match status {
            SidecarStatus::Error(msg) => {
                assert!(
                    msg.contains("model file not found"),
                    "unexpected error: {msg}"
                );
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }
}
