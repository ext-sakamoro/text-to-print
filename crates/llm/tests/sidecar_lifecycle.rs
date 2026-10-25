//! The bundled sidecar does not outlive the app, and listens on loopback only
//!
//! A helper process stands in for the app: it calls `SidecarProcess::spawn`
//! with the real `alice-llm-server` binary and a FIFO as the model, so the
//! sidecar stays blocked in the model read (alive, already bound) without a
//! model file. The test then stops the helper with SIGTERM or SIGKILL (neither
//! runs `Drop`, so `kill_on_drop` cannot help) and requires the sidecar to be
//! gone.
//!
//! Needs the binary: `ALICE_LLM_SERVER_BIN=<path> cargo test -p
//! text-to-print-llm --test sidecar_lifecycle -- --ignored`. CI builds it with
//! `scripts/build_sidecar.sh` and runs this file in its own step.
#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const HELPER_MODEL_ENV: &str = "TTP_SIDECAR_HELPER_MODEL";
const BIN_ENV: &str = "ALICE_LLM_SERVER_BIN";

/// Runs only inside the helper process started by the tests below
#[test]
#[ignore = "helper process for the lifecycle tests"]
fn helper_app_holding_a_sidecar() {
    let Ok(model) = std::env::var(HELPER_MODEL_ENV) else {
        return;
    };
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let model = text_to_print_llm::downloader::ModelFile::user_supplied(
            PathBuf::from(model),
            text_to_print_llm::model::ModelChoice::default(),
        );
        let cfg = text_to_print_llm::sidecar::SidecarConfig {
            bin_path: Some(PathBuf::from(std::env::var(BIN_ENV).unwrap())),
            health_timeout: Duration::from_secs(600),
            ..Default::default()
        };
        // Never becomes healthy (the model read blocks), so this waits until
        // the test stops the process
        let _ = text_to_print_llm::sidecar::SidecarProcess::spawn(&model, cfg).await;
    });
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fifo(tag: &str) -> (Scratch, PathBuf) {
    let dir = std::env::temp_dir().join(format!("ttp-sidecar-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("model.gguf");
    assert!(
        Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    (Scratch(dir), path)
}

fn pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}

fn sidecar_pid(model: &Path) -> u32 {
    let out = Command::new("pgrep")
        .args(["-f", &model.display().to_string()])
        .output()
        .unwrap();
    let pids: Vec<u32> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect();
    assert_eq!(
        pids.len(),
        1,
        "expected one sidecar for {}, got {pids:?}",
        model.display()
    );
    pids[0]
}

/// Start the helper app and wait until its sidecar is bound and reading the
/// model; returns (helper, sidecar pid, sidecar address)
fn start_app(model: &Path) -> (Child, u32, std::net::SocketAddr) {
    let bin = std::env::var(BIN_ENV)
        .unwrap_or_else(|_| panic!("{BIN_ENV} must point at alice-llm-server"));
    assert!(Path::new(&bin).is_file(), "{BIN_ENV}={bin} is not a file");
    let mut helper = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "helper_app_holding_a_sidecar",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(HELPER_MODEL_ENV, model)
        .env(BIN_ENV, &bin)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // Lines arrive through a thread so a silent helper fails after a bound
    // wait instead of blocking the test; the thread keeps draining the pipe
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let stdout = helper.stdout.take().unwrap();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    // The server binds and prints its line within a second or two; if a marker
    // never arrives (the drain or the re-emit regressed), fail fast rather than
    // hang Readers of the parent's stdout see both markers within this window
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut wait_for = |marker: &str| -> String {
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = rx.recv_timeout(left).unwrap_or_else(|_| {
                let _ = helper.kill();
                panic!("no `{marker}` from the sidecar within 8 s")
            });
            // libtest prints `test <name> ... ` without a newline, so the
            // sidecar's first line can start in the middle of a line
            if let Some(rest) = line.split(marker).nth(1) {
                return rest.trim().to_string();
            }
        }
    };
    let addr: std::net::SocketAddr = wait_for("Bound to http://").parse().unwrap();
    wait_for("Loading:");
    let pid = sidecar_pid(model);
    (helper, pid, addr)
}

fn gone_within(pid: u32, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if !pid_alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn stop_app_and_check(signal: &str, tag: &str) {
    let (_scratch, model) = fifo(tag);
    let (mut helper, sidecar, addr) = start_app(&model);
    // `addr` is the port the app resolved and re-emitted, not just a string that
    // matched: connecting here proves something listens on exactly that port, and
    // the refused connect after the sidecar dies (below) proves that listener was
    // the sidecar A wrong ephemeral port fails this pre-kill connect
    assert!(addr.ip().is_loopback(), "the sidecar listens on {addr}");
    assert!(
        std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_ok(),
        "nothing accepts on {addr}"
    );
    assert!(
        Command::new("kill")
            .args([signal, &helper.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let _ = helper.wait();
    let gone = gone_within(sidecar, Duration::from_secs(10));
    if !gone {
        let _ = Command::new("kill")
            .args(["-9", &sidecar.to_string()])
            .status();
    }
    assert!(
        gone,
        "the sidecar (pid {sidecar}) outlived the app stopped with {signal}"
    );
    assert!(
        std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(1)).is_err(),
        "{addr} still accepts after the app stopped"
    );
}

#[test]
#[ignore = "needs ALICE_LLM_SERVER_BIN (see the module doc)"]
fn sigterm_to_the_app_stops_the_sidecar() {
    stop_app_and_check("-TERM", "term");
}

#[test]
#[ignore = "needs ALICE_LLM_SERVER_BIN (see the module doc)"]
fn sigkill_to_the_app_stops_the_sidecar() {
    stop_app_and_check("-KILL", "kill");
}
