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

    let templates_dir =
        labeler::resolve_dir(std::env::var_os("LABELER_TEMPLATES_DIR"), "templates");
    let templates = TemplateRegistry::load_from_dir(&templates_dir)
        .unwrap_or_else(|err| panic!("failed to load templates: {err}"));
    tracing::info!(count = templates.len(), "templates loaded");

    // Dev-only: warn if the locally served ui/dist bundle is missing or older than ui/src. Skipped when
    // LABELER_UI_DIR is set (the container sets it). Never fails startup. See #69.
    if std::env::var_os("LABELER_UI_DIR").is_none() {
        use labeler::ui_freshness::{ui_dist_status, UiDistStatus};
        match ui_dist_status(
            std::path::Path::new("ui/src"),
            std::path::Path::new("ui/dist"),
        ) {
            UiDistStatus::MissingDist => tracing::warn!(
                "ui/dist not found; the web UI will not load. Run `npm --prefix ui run build`, or use \
                 the Vite dev server (`npm --prefix ui run dev`)."
            ),
            UiDistStatus::Stale => tracing::warn!(
                "ui/dist is older than ui/src; serving a stale UI. Rebuild with \
                 `npm --prefix ui run build`, or use the Vite dev server (`npm --prefix ui run dev`)."
            ),
            UiDistStatus::Fresh | UiDistStatus::Unknown => {}
        }
    }

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

    let state = Arc::new(AppState::new(templates, templates_dir, store));

    // Job-log retention is an app setting (see ADR-0024), resolved live each run; no env var.
    // Prune once at startup, then daily. The ticker always runs because the setting can change at runtime.
    match labeler::settings::prune_job_log_once(state.store()).await {
        Ok(n) => tracing::info!(deleted = n, "pruned job log at startup"),
        Err(err) => tracing::warn!(%err, "startup job-log prune failed"),
    }
    {
        let prune_state = state.clone();
        tokio::spawn(async move {
            let period = std::time::Duration::from_secs(24 * 60 * 60);
            // interval_at starts one period out so this does not double-prune the startup run.
            let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
            loop {
                ticker.tick().await;
                match labeler::settings::prune_job_log_once(prune_state.store()).await {
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
