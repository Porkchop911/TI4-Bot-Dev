//! Standalone HTTP and WebSocket server binary for Twilight Imperium 4 online multiplayer.

use std::sync::Arc;
use std::time::Duration;
use ti4_server::http::create_app;
use ti4_server::session::GameRegistry;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_owned());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let args: Vec<String> = std::env::args().collect();
    let mut data_dir = std::env::var("TI4_DATA_DIR").unwrap_or_else(|_| "./data/games".to_owned());
    for i in 0..args.len() {
        if args[i] == "--data-dir" && i + 1 < args.len() {
            data_dir = args[i + 1].clone();
        }
    }

    let store = Arc::new(ti4_server::storage::FileGameStore::new(&data_dir)?);
    let mut registry = GameRegistry::new().with_store(store);
    if let Ok(millis) = std::env::var("TI4_DEV_PRESENCE_GRACE_MS") {
        registry = registry.with_presence_grace(Duration::from_millis(millis.parse()?));
    }
    if let Ok(bot_password) = std::env::var("TI4_BOT_PASSWORD")
        && !bot_password.trim().is_empty()
    {
        let advisor_url =
            std::env::var("TI4_ADVISOR_URL").unwrap_or_else(|_| "http://127.0.0.1:8081".to_owned());
        let bot_agent_bin = std::env::var("TI4_BOT_AGENT_BIN")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                let release_path = std::path::PathBuf::from("./target/release/ti4-bot-agent");
                let debug_path = std::path::PathBuf::from("./target/debug/ti4-bot-agent");
                if release_path.exists() {
                    release_path
                } else if debug_path.exists() {
                    debug_path
                } else {
                    std::path::PathBuf::from("ti4-bot-agent")
                }
            });
        let max_active_bots: usize = std::env::var("TI4_MAX_ACTIVE_BOTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(6);

        info!("MLP bot service enabled with advisor at {advisor_url}");
        registry = registry.with_bot_service(ti4_server::session::BotServiceConfig {
            password: bot_password,
            advisor_url,
            bot_agent_bin,
            server_port: port,
            max_active_bots,
        });
    }
    let registry = Arc::new(registry);

    // Print recovery results even when tracing has no RUST_LOG filter configured.
    let recovery = registry.recover_all_games_report()?;
    for gid in &recovery.recovered {
        info!("Durable storage: recovered '{gid}' from disk");
    }
    for failure in &recovery.failed {
        eprintln!(
            "Durable storage: could not recover '{}' ({}): {}",
            failure.game_id, failure.stage, failure.error
        );
    }
    println!(
        "Durable storage: recovered {} game(s); {} failed (saved files retained)",
        recovery.recovered.len(),
        recovery.failed.len()
    );

    let app = create_app(registry);
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("authoritative server listening on http://{addr}");

    println!("\n╔══════════════════════════════════════════════════════════════════╗");
    println!("║  Twilight Imperium 4 — Authoritative Server                     ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║  Listening on:  http://127.0.0.1:{port:<29}║");
    println!(
        "║  Health Check:  http://127.0.0.1:{port}/health{pad:<22}║",
        pad = ""
    );
    println!("║  Storage Dir:   {data_dir:<48}║");
    println!("║                                                                  ║");
    println!("║  Web Client:    cd web && npm run dev                            ║");
    println!("║  Open in UI:    http://127.0.0.1:3000                            ║");
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
            info!("Received shutdown signal, terminating server");
            tokio::time::sleep(Duration::from_millis(100)).await;
        })
        .await?;

    Ok(())
}
