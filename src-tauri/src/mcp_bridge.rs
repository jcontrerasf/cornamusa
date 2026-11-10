//! `cornamusa mcp`: the process Claude Code / Claude Desktop launches. It doesn't run the app:
//! it connects to the running instance (port and token in `mcp.json`) and pipes stdio to it,
//! starting Cornamusa minimized first if needed. stdout carries only protocol messages, so
//! every diagnostic goes to stderr.

use cornamusa_core::mcp::{Discovery, DISCOVERY_FILE};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

/// Must match `identifier` in tauri.conf.json (Tauri's app_data_dir = data_dir/identifier).
const IDENTIFIER: &str = "app.cornamusa.mail";
const STARTUP_WAIT: Duration = Duration::from_secs(25);

fn data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(IDENTIFIER))
}

async fn connect() -> Option<TcpStream> {
    let text = tokio::fs::read_to_string(data_dir()?.join(DISCOVERY_FILE)).await.ok()?;
    let d: Discovery = serde_json::from_str(&text).ok()?;
    let mut stream = tokio::time::timeout(Duration::from_secs(2), TcpStream::connect(("127.0.0.1", d.port))).await.ok()?.ok()?;
    stream.write_all(format!("{}\n", d.token).as_bytes()).await.ok()?;
    Some(stream)
}

/// Starts Cornamusa in the tray, detached so it outlives this process.
fn launch_app() {
    let Ok(exe) = std::env::current_exe() else { return };
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--minimized")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    if let Err(e) = cmd.spawn() {
        eprintln!("cornamusa mcp: no se pudo abrir Cornamusa: {e}");
    }
}

/// Copies one direction, flushing after every chunk so messages go out immediately.
async fn pump<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(mut from: R, mut to: W) {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match from.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if to.write_all(&buf[..n]).await.is_err() || to.flush().await.is_err() {
                    break;
                }
            }
        }
    }
    let _ = to.shutdown().await;
}

async fn bridge() -> i32 {
    let stream = match connect().await {
        Some(s) => s,
        None => {
            launch_app();
            let deadline = Instant::now() + STARTUP_WAIT;
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if let Some(s) = connect().await {
                    break s;
                }
                if Instant::now() > deadline {
                    eprintln!(
                        "cornamusa mcp: no pude conectarme a Cornamusa. Ábrelo y activa \
                         Configuración → General → «Integración con Claude (MCP)»."
                    );
                    return 1;
                }
            }
        }
    };
    let (r, w) = stream.into_split();
    // The session ends when Claude closes stdin or the app closes the connection.
    tokio::select! {
        _ = pump(tokio::io::stdin(), w) => {}
        _ = pump(r, tokio::io::stdout()) => {}
    }
    0
}

pub fn run() -> i32 {
    match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt.block_on(bridge()),
        Err(e) => {
            eprintln!("cornamusa mcp: {e}");
            1
        }
    }
}
