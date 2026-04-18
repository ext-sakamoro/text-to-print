use serde::Deserialize;
use std::sync::mpsc;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const RELEASES_URL: &str = "https://api.github.com/repos/ext-sakamoro/3dvbgaran/releases/latest";

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub latest: String,
    pub download_url: String,
    pub has_update: bool,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
}

pub struct UpdateChecker {
    pub result: Option<UpdateInfo>,
    rx: mpsc::Receiver<UpdateInfo>,
}

impl UpdateChecker {
    pub fn start(runtime: &tokio::runtime::Runtime) -> Self {
        let (tx, rx) = mpsc::channel();

        runtime.spawn(async move {
            if let Ok(info) = check_latest().await {
                let _ = tx.send(info);
            }
        });

        Self { result: None, rx }
    }

    pub fn poll(&mut self) {
        if let Ok(info) = self.rx.try_recv() {
            self.result = Some(info);
        }
    }
}

async fn check_latest() -> anyhow::Result<UpdateInfo> {
    let client = reqwest::Client::builder()
        .user_agent("3dvbgaran-updater")
        .build()?;

    let release: GhRelease = client.get(RELEASES_URL).send().await?.json().await?;

    let latest = release.tag_name.trim_start_matches('v').to_string();
    let has_update = latest != CURRENT_VERSION;

    Ok(UpdateInfo {
        latest,
        download_url: release.html_url,
        has_update,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_version_is_set() {
        assert!(!CURRENT_VERSION.is_empty());
    }

    #[test]
    fn releases_url_is_valid() {
        assert!(RELEASES_URL.starts_with("https://"));
        assert!(RELEASES_URL.contains("github.com"));
    }
}
