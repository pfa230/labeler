use std::{net::SocketAddr, sync::Arc};

use labeler::{app, store::Store, AppState, TemplateRegistry};
use tracing_subscriber::EnvFilter;

/// Container HEALTHCHECK: probe the local `/api/health` endpoint and exit 0 (healthy) or 1.
/// Lets the runtime image carry no shell or `wget`/`curl` (see ADR-0029). Runs before tracing
/// init so it stays quiet, and exits the process directly.
async fn run_healthcheck() -> i32 {
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let url = format!("http://127.0.0.1:{port}/api/health");
    let client = reqwest::Client::new();
    match client
        .get(&url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => 0,
        Ok(resp) => {
            eprintln!("healthcheck: {url} returned {}", resp.status());
            1
        }
        Err(err) => {
            eprintln!("healthcheck: {url} failed: {err}");
            1
        }
    }
}

/// Fatal startup error: log and exit 1 without an unwinding panic or backtrace.
macro_rules! fatal {
    ($($arg:tt)*) => {{
        tracing::error!($($arg)*);
        std::process::exit(1);
    }};
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = labeler::resolve_dir(std::env::var_os("LABELER_CONFIG_DIR"), "/config");
    let templates_dir = config_dir.join("templates");

    if let Err(err) = std::fs::create_dir_all(&templates_dir) {
        fatal!(path = %templates_dir.display(), %err, "failed to create templates dir");
    }
    // assets dir must exist: render::helpers::resolve_image_asset canonicalizes {config}/assets and
    // fails if it is missing (the entrypoint covers Docker, but local/non-entrypoint runs need this).
    let assets_dir = config_dir.join("assets");
    if let Err(err) = std::fs::create_dir_all(&assets_dir) {
        fatal!(path = %assets_dir.display(), %err, "failed to create assets dir");
    }

    let store = match Store::open(&config_dir.join("labeler.db")) {
        Ok(s) => s,
        Err(err) => fatal!(%err, "failed to open store"),
    };

    let templates = match TemplateRegistry::load_from_dir(&templates_dir) {
        Ok(t) => t,
        Err(err) => fatal!(%err, "failed to load templates"),
    };
    tracing::info!(
        count = templates.len(),
        broken = templates.broken().len(),
        "templates loaded"
    );
    for b in templates.broken() {
        tracing::warn!(filename = %b.filename, error = %b.error, "template failed to load; fix it and POST /api/templates/reload");
    }

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

    if let (Ok(u), Ok(p)) = (
        std::env::var("LABELER_INIT_USER"),
        std::env::var("LABELER_INIT_PASSWORD"),
    ) {
        if store.count_users().await.unwrap_or(0) == 0 && !u.is_empty() && !p.is_empty() {
            let hash = match labeler::auth::hash_password(&p) {
                Ok(h) => h,
                Err(err) => fatal!(%err, "failed to hash init password"),
            };
            if let Err(err) = store.create_user(&u, &hash).await {
                fatal!(user = %u, %err, "failed to create init user");
            }
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

    let server_app = app(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr: SocketAddr = match format!("0.0.0.0:{port}").parse() {
        Ok(a) => a,
        Err(err) => fatal!(port, %err, "invalid PORT value"),
    };

    tracing::info!(%addr, "labeler service listening");

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(err) => fatal!(%addr, %err, "failed to bind listener"),
    };

    if let Err(err) = axum::serve(listener, server_app).await {
        fatal!(%err, "server error");
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    // `labeler healthcheck` is the container HEALTHCHECK command; handle it before anything else.
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        std::process::exit(run_healthcheck().await);
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("labeler=info,tower_http=info")),
        )
        .init();

    // run() only returns Ok(()); all error paths call fatal!() which logs and exits.
    let _ = run().await;
}
