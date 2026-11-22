use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tracing::{debug, info};

use crate::error::{CdpError, Result};

/// OpenGL backend option passed to headless Chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum GlRenderer {
    AngleEgl,
    Swangle,
    Angle,
    #[default]
    None,
}

const HEADLESS_SHELL_REL: &str =
    "node_modules/.remotion/chrome-headless-shell/linux64/chrome-headless-shell-linux64/chrome-headless-shell";

fn is_headless_shell_path(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.contains("headless-shell"))
}

/// Discovers a Chrome or Chrome Headless Shell binary.
///
/// Order: explicit path -> env (`RSMOTION_BROWSER` / `REMOTION_CHROME_EXECUTABLE`) ->
/// upward search for Remotion's cached `chrome-headless-shell` -> system Chrome/Chromium.
pub fn discover_browser(
    explicit: Option<&Path>,
    search_hint: Option<&Path>,
) -> Result<(PathBuf, bool)> {
    if let Some(p) = explicit {
        if !p.exists() {
            return Err(CdpError::BrowserLaunch(
                p.display().to_string(),
                "explicit browser executable does not exist".to_string(),
            ));
        }
        return Ok((p.to_path_buf(), is_headless_shell_path(p)));
    }

    for var in ["RSMOTION_BROWSER", "REMOTION_CHROME_EXECUTABLE"] {
        if let Ok(val) = std::env::var(var) {
            let p = PathBuf::from(val);
            if p.exists() {
                return Ok((p.clone(), is_headless_shell_path(&p)));
            }
        }
    }

    let mut roots = Vec::new();
    if let Some(hint) = search_hint {
        roots.push(hint.to_path_buf());
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }

    for start in roots {
        let mut cur = Some(start.as_path());
        while let Some(dir) = cur {
            let direct = dir.join(HEADLESS_SHELL_REL);
            if direct.exists() {
                return Ok((direct, true));
            }
            let fixtures = dir.join("tests/fixtures").join(HEADLESS_SHELL_REL);
            if fixtures.exists() {
                return Ok((fixtures, true));
            }
            cur = dir.parent();
        }
    }

    for sys in [
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
    ] {
        let p = PathBuf::from(sys);
        if p.exists() {
            return Ok((p, false));
        }
    }

    Err(CdpError::BrowserNotFound)
}

/// Manages a headless Chrome subprocess and its temporary user-data directory.
pub struct BrowserProcess {
    child: Child,
    ws_url: String,
    user_data_dir: PathBuf,
    is_headless_shell: bool,
}

impl BrowserProcess {
    /// Launches Chrome and waits (up to 15s) for the DevTools WebSocket endpoint on stderr.
    pub async fn launch(
        executable: &Path,
        is_headless_shell: bool,
        gl: GlRenderer,
    ) -> Result<Self> {
        let unique = format!(
            "rsmotion-chrome-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let user_data_dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&user_data_dir)?;

        let mut args: Vec<String> = vec![
            "--remotion-shared-memory-capture".to_string(),
            "--remote-debugging-port=0".to_string(),
            format!("--user-data-dir={}", user_data_dir.display()),
            "--no-sandbox".to_string(),
            "--disable-setuid-sandbox".to_string(),
            "--disable-dev-shm-usage".to_string(),
            "--disable-background-networking".to_string(),
            "--disable-background-timer-throttling".to_string(),
            "--disable-backgrounding-occluded-windows".to_string(),
            "--disable-breakpad".to_string(),
            "--disable-client-side-phishing-detection".to_string(),
            "--disable-component-extensions-with-background-pages".to_string(),
            "--disable-default-apps".to_string(),
            "--disable-extensions".to_string(),
            "--disable-hang-monitor".to_string(),
            "--disable-ipc-flooding-protection".to_string(),
            "--disable-popup-blocking".to_string(),
            "--disable-prompt-on-repost".to_string(),
            "--disable-renderer-backgrounding".to_string(),
            "--disable-background-media-suspend".to_string(),
            "--disable-component-update".to_string(),
            "--disable-domain-reliability".to_string(),
            "--disable-features=AudioServiceOutOfProcess,IsolateOrigins,site-per-process,Translate,BackForwardCache,AvoidUnnecessaryBeforeUnloadCheckSync,IntensiveWakeUpThrottling,LocalNetworkAccessChecks,BlockInsecurePrivateNetworkRequests,PrivateNetworkAccessSendPreflights,PrivateNetworkAccessRespectPreflightResults".to_string(),
            "--disable-print-preview".to_string(),
            "--disable-site-isolation-trials".to_string(),
            "--disk-cache-size=268435456".to_string(),
            "--hide-scrollbars".to_string(),
            "--no-default-browser-check".to_string(),
            "--no-pings".to_string(),
            "--font-render-hinting=none".to_string(),
            "--no-zygote".to_string(),
            "--ignore-gpu-blocklist".to_string(),
            "--disable-sync".to_string(),
            "--force-color-profile=srgb".to_string(),
            "--force-gpu-mem-available-mb=4096".to_string(),
            "--mute-audio".to_string(),
            "--no-first-run".to_string(),
            "--password-store=basic".to_string(),
            "--use-mock-keychain".to_string(),
            "--enable-automation".to_string(),
            "--intensive-wake-up-throttling-policy=0".to_string(),
            "--allow-pre-commit-input".to_string(),
            if is_headless_shell {
                "--headless=old".to_string()
            } else {
                "--headless=new".to_string()
            },
        ];

        match gl {
            GlRenderer::AngleEgl => {
                args.push("--use-gl=angle".to_string());
                args.push("--use-angle=gl-egl".to_string());
                args.push(
                    "--enable-features=NetworkService,NetworkServiceInProcess,CanvasDrawElement,VaapiVideoDecoder"
                        .to_string(),
                );
            }
            GlRenderer::Swangle => {
                args.push("--use-gl=angle".to_string());
                args.push("--use-angle=swiftshader".to_string());
                args.push(
                    "--enable-features=NetworkService,NetworkServiceInProcess,CanvasDrawElement"
                        .to_string(),
                );
            }
            GlRenderer::Angle => {
                args.push("--use-gl=angle".to_string());
                args.push("--enable-unsafe-swiftshader".to_string());
                args.push(
                    "--enable-features=NetworkService,NetworkServiceInProcess,CanvasDrawElement"
                        .to_string(),
                );
            }
            GlRenderer::None => {
                args.push(
                    "--enable-features=NetworkService,NetworkServiceInProcess,CanvasDrawElement"
                        .to_string(),
                );
            }
        }

        args.push("about:blank".to_string());

        let mut cmd = Command::new(executable);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| {
            CdpError::BrowserLaunch(executable.display().to_string(), e.to_string())
        })?;

        let stderr = child.stderr.take().ok_or_else(|| {
            CdpError::BrowserLaunch(
                executable.display().to_string(),
                "missing stderr pipe".to_string(),
            )
        })?;

        let mut lines = BufReader::new(stderr).lines();
        let ws_url = tokio::time::timeout(Duration::from_secs(15), async {
            let mut recent_err = String::new();
            while let Ok(Some(line)) = lines.next_line().await {
                debug!("chrome stderr: {line}");
                if let Some(pos) = line.find("DevTools listening on ") {
                    let url = line[pos + "DevTools listening on ".len()..].trim();
                    return Ok(url.to_string());
                }
                if recent_err.len() < 2048 {
                    recent_err.push_str(&line);
                    recent_err.push('\n');
                }
            }
            Err(CdpError::BrowserLaunch(
                executable.display().to_string(),
                format!("browser exited before reporting DevTools URL: {recent_err}"),
            ))
        })
        .await
        .map_err(|_| CdpError::DevToolsTimeout)??;

        // Keep draining stderr so the browser never blocks on a full stderr pipe.
        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                debug!("chrome stderr: {line}");
            }
        });

        info!(
            "Launched Chrome ({}) gl={:?} ws={}",
            executable.display(),
            gl,
            ws_url
        );

        Ok(Self {
            child,
            ws_url,
            user_data_dir,
            is_headless_shell,
        })
    }

    pub fn ws_url(&self) -> &str {
        &self.ws_url
    }

    pub fn is_headless_shell(&self) -> bool {
        self.is_headless_shell
    }

    /// Cleanly terminates the Chrome process and removes its temporary profile directory.
    pub async fn shutdown(mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
        let _ = std::fs::remove_dir_all(&self.user_data_dir);
    }
}

impl Drop for BrowserProcess {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        let _ = std::fs::remove_dir_all(&self.user_data_dir);
    }
}
