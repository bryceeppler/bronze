use std::io::{Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::process::ExitCode;
use std::time::Duration;

use bronze_sync::{Config, Store, serve};

fn main() -> ExitCode {
    let command = std::env::args().nth(1);
    let outcome = match command.as_deref() {
        None => run(),
        Some("health") => health(),
        Some(_) => Err("usage: bronze-sync [health], configured by BRONZE_SYNC_* env vars".into()),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bronze-sync: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    let store = Store::open(&config.data)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let listener = tokio::net::TcpListener::bind(config.listen).await?;
        eprintln!("bronze-sync listening on {}, data in {}", listener.local_addr()?, config.data.display());
        serve(listener, store, config.limits, stop_signal()).await?;
        Ok(())
    })
}

async fn stop_signal() {
    #[cfg(unix)]
    {
        let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("a terminate signal handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}

fn health() -> Result<(), Box<dyn std::error::Error>> {
    let mut address = Config::from_env()?.listen;
    if address.ip().is_unspecified() {
        address.set_ip(match address {
            SocketAddr::V4(_) => Ipv4Addr::LOCALHOST.into(),
            SocketAddr::V6(_) => Ipv6Addr::LOCALHOST.into(),
        });
    }
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    write!(stream, "GET {} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n", bronze_wire::HEALTH_ROUTE)?;
    let mut status = [0; 12];
    stream.read_exact(&mut status)?;
    match &status[9..12] {
        b"200" => Ok(()),
        other => Err(format!("health answered {}", String::from_utf8_lossy(other)).into()),
    }
}
