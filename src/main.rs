use std::{net::SocketAddr, sync::Arc};

use labeler::{app, store::Store, AppState, TemplateRegistry};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("labeler=info,tower_http=info")),
        )
        .init();

    let templates = TemplateRegistry::load_from_dir("templates")
        .unwrap_or_else(|err| panic!("failed to load templates: {err}"));
    tracing::info!(count = templates.len(), "templates loaded");
    let data_dir = std::env::var_os("LABELER_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("data"));
    std::fs::create_dir_all(&data_dir).expect("failed to create data dir");
    let store = Store::open(&data_dir.join("labeler.db"))
        .unwrap_or_else(|err| panic!("failed to open store: {err}"));

    if let (Ok(u), Ok(p)) = (
        std::env::var("LABELER_INIT_USER"),
        std::env::var("LABELER_INIT_PASSWORD"),
    ) {
        if store.count_users().await.unwrap_or(0) == 0 && !u.is_empty() && !p.is_empty() {
            let hash = labeler::auth::hash_password(&p).expect("hash init password");
            store
                .create_user(&u, &hash)
                .await
                .expect("create init user");
            tracing::info!(user = %u, "bootstrapped initial user from env");
        }
    }

    let state = Arc::new(AppState::new(templates, "templates".into(), store));

    // Job-log retention: prune rows older than LABELER_JOB_LOG_RETENTION_DAYS (default 90; 0 disables).
    let retention_days: u32 = match std::env::var("LABELER_JOB_LOG_RETENTION_DAYS") {
        Ok(v) => v.parse().unwrap_or_else(|_| {
            tracing::warn!(value = %v, "invalid LABELER_JOB_LOG_RETENTION_DAYS; using default 90");
            90
        }),
        Err(_) => 90,
    };
    if retention_days > 0 {
        // Prune once now so a restarted instance applies retention immediately.
        match state.store().prune_jobs(retention_days).await {
            Ok(n) => tracing::info!(deleted = n, retention_days, "pruned job log at startup"),
            Err(err) => tracing::warn!(%err, "startup job-log prune failed"),
        }
        // Then re-prune daily. interval_at starts one period out so this does not double-prune the
        // startup run (tokio's interval() first tick fires immediately). Detached: process exit ends it.
        let prune_state = state.clone();
        tokio::spawn(async move {
            let period = std::time::Duration::from_secs(24 * 60 * 60);
            let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
            loop {
                ticker.tick().await;
                match prune_state.store().prune_jobs(retention_days).await {
                    Ok(n) => tracing::info!(deleted = n, "pruned job log"),
                    Err(err) => tracing::warn!(%err, "job-log prune failed"),
                }
            }
        });
    }

    let app = app(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().expect("invalid PORT");

    tracing::info!(%addr, "labeler service listening");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind listener");

    axum::serve(listener, app).await.expect("server error");
}
