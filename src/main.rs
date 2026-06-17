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

    let app = app(Arc::new(AppState::new(
        templates,
        "templates".into(),
        store,
    )));

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().expect("invalid PORT");

    tracing::info!(%addr, "labeler service listening");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind listener");

    axum::serve(listener, app).await.expect("server error");
}
