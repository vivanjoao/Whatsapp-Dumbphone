use anyhow::{Context, Result};
use dumbphone_gateway::{
    config::Config,
    http::{self, App},
    store::Store,
    whatsapp::NativeGateway,
};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    future::Future,
    time::Duration,
};

fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("--version") => {
            println!("dumbphone-gateway {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help") => {
            println!("Usage: dumbphone-gateway [--check-config|--version|--help]");
            return Ok(());
        }
        Some("--check-config") | None => {}
        Some(_) => anyhow::bail!("Unknown option. Use --help"),
    }
    dotenvy::dotenv().ok();
    let config = Config::from_env()?;
    if std::env::args().nth(1).as_deref() == Some("--check-config") {
        println!("Configuration is valid; no connection started.");
        return Ok(());
    }
    fs::create_dir_all(&config.data_dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&config.data_dir, fs::Permissions::from_mode(0o700))?;
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(config.data_dir.join("gateway.lock"))?;
    lock.try_lock_exclusive()
        .context("Another gateway owns this DATA_DIR. Do not remove its lock.")?;
    run_with_runtime(lock, Duration::from_secs(2), run(config))
}

fn run_with_runtime(
    lock: File,
    shutdown_timeout: Duration,
    future: impl Future<Output = Result<()>>,
) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let result = runtime.block_on(future);
    // A cancelled spawn_blocking job may still be running. Bound the runtime
    // teardown on both normal shutdown and startup errors (35 + 2 seconds).
    runtime.shutdown_timeout(shutdown_timeout);
    // Timed-out workers can outlive the runtime. Keep exclusive ownership until
    // the OS closes this descriptor at process exit, including the error path.
    std::mem::forget(lock);
    result
}

async fn run(config: Config) -> Result<()> {
    let store = Store::open(&config.data_dir.join("web.db")).await?;
    store.prune().await?;
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    let (gateway, bot) = NativeGateway::open(&config.data_dir).await?;
    let app = http::router(App::new(config, store.clone(), gateway.clone()));
    let mut bot = bot.spawn();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = shutdown_rx.await;
    });
    let mut server = tokio::spawn(async move { server.await });
    let cleanup = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(600));
        loop {
            tick.tick().await;
            if store.prune().await.is_err() {
                eprintln!("Web store cleanup failed");
            }
        }
    });
    eprintln!("Native gateway started; no Node or Chromium. Waiting for pairing/connection.");
    let mut server_ended = false;
    let bot_ended = tokio::select! {
        _ = shutdown_signal() => false,
        _ = &mut bot => { eprintln!("WhatsApp worker stopped"); true },
        _ = &mut server => { eprintln!("HTTP server stopped"); server_ended = true; false },
    };
    let _ = shutdown_tx.send(());
    // Drain HTTP before stopping the account owner. Pending ambiguous sends are
    // never replayed on restart. A bounded shutdown is required for docker stop.
    let drained = if server_ended {
        false
    } else {
        matches!(
            tokio::time::timeout(Duration::from_secs(25), &mut server).await,
            Ok(Ok(Ok(())))
        )
    };
    if !drained {
        server.abort();
    }
    cleanup.abort();
    let stopped = if bot_ended {
        drop(bot);
        true
    } else {
        tokio::time::timeout(Duration::from_secs(5), bot.shutdown())
            .await
            .is_ok()
    };
    let flushed = tokio::time::timeout(Duration::from_secs(5), gateway.flush()).await;
    anyhow::ensure!(
        drained && stopped && !bot_ended && matches!(flushed, Ok(Ok(()))),
        "Shutdown could not be fully drained"
    );
    Ok(())
}
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = signal.recv() => {} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{process::Command, time::Instant};

    #[test]
    fn bounded_runtime_shutdown_preserves_lock_and_result() {
        const CHILD_PATH: &str = "GATEWAY_RUNTIME_TEST_LOCK";
        const CHILD_FAILURE: &str = "GATEWAY_RUNTIME_TEST_FAILURE";
        if let Some(path) = std::env::var_os(CHILD_PATH) {
            let lock = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap();
            lock.try_lock_exclusive().unwrap();
            let fail = std::env::var_os(CHILD_FAILURE).is_some();
            let result = run_with_runtime(lock, Duration::from_millis(150), async {
                let (started, ready) = tokio::sync::oneshot::channel();
                tokio::task::spawn_blocking(move || {
                    started.send(()).unwrap();
                    // Deliberately uninterruptible by future cancellation.
                    loop {
                        std::thread::park();
                    }
                });
                ready.await.unwrap();
                if fail {
                    anyhow::bail!("simulated startup failure");
                }
                Ok(())
            });
            assert_eq!(result.is_err(), fail);
            // Even after shutdown_timeout returned, the account is still owned.
            let second = OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .unwrap();
            assert!(second.try_lock_exclusive().is_err());
            std::process::exit(if result.is_err() { 23 } else { 0 });
        }

        for failure in [false, true] {
            let lock = tempfile::NamedTempFile::new().unwrap();
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "tests::bounded_runtime_shutdown_preserves_lock_and_result",
                ])
                .env(CHILD_PATH, lock.path())
                .env_remove(CHILD_FAILURE);
            if failure {
                command.env(CHILD_FAILURE, "1");
            }
            let started = Instant::now();
            let mut child = command.spawn().unwrap();
            let status = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break status;
                }
                if started.elapsed() > Duration::from_secs(3) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("runtime teardown waited for the stuck blocking worker");
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            assert_eq!(status.code(), Some(if failure { 23 } else { 0 }));
            // Process exit, rather than runtime teardown, releases ownership.
            lock.as_file().try_lock_exclusive().unwrap();
        }
    }
}
