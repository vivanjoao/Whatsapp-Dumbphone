use anyhow::{Context, Result};
use dumbphone_gateway::{
    config::Config,
    http::{self, App},
    store::Store,
    whatsapp::NativeGateway,
};
use fs2::FileExt;
use std::{
    fs::{self, OpenOptions},
    time::Duration,
};

#[tokio::main(worker_threads = 2)]
async fn main() -> Result<()> {
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
    drop(lock);
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
