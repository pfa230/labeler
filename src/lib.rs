pub mod api;
pub mod errors;
pub mod models;
pub mod openapi;
pub mod templates;

pub use api::app;
pub use templates::TemplateRegistry;

#[cfg(test)]
mod tests {
    use super::{app, TemplateRegistry};
    use std::future::IntoFuture;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn server_starts_and_accepts_connections() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("local addr");

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let templates = TemplateRegistry::load_from_dir("templates")
            .expect("load templates");
        let server = axum::serve(listener, app(Arc::new(templates))).with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
        });

        let handle = tokio::spawn(server.into_future());

        let connect = TcpStream::connect(addr);
        tokio::time::timeout(Duration::from_millis(250), connect)
            .await
            .expect("server did not accept connections in time")
            .expect("failed to connect to server");

        let _ = shutdown_tx.send(());
        handle.await.expect("server task failed").expect("server error");
    }
}
