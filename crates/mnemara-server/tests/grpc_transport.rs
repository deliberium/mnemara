use hyper_util::rt::TokioIo;
use mnemara_core::MemoryStore;
use mnemara_protocol::v1::SnapshotRequest;
use mnemara_protocol::v1::memory_service_client::MemoryServiceClient;
use mnemara_server::{AuthConfig, GrpcMemoryService, ServerLimits, ServerMetrics, ServerRuntime};
use mnemara_store_sled::{SledMemoryStore, SledStoreConfig};
use rcgen::generate_simple_self_signed;
use std::fs;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::{TcpListener, UnixListener, UnixStream};
use tokio::sync::oneshot;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::{
    Certificate, Channel, ClientTlsConfig, Endpoint, Identity, Server, ServerTlsConfig,
};
use tower::service_fn;

fn temp_path(label: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("mnemara-{label}-{nonce}"))
}

fn temp_store(label: &str) -> Arc<SledMemoryStore> {
    let dir = temp_path(label);
    fs::create_dir_all(&dir).unwrap();
    Arc::new(SledMemoryStore::open(SledStoreConfig::new(&dir)).unwrap())
}

fn runtime_for(store: &Arc<SledMemoryStore>) -> ServerRuntime {
    ServerRuntime::new(
        store.as_ref().backend_kind(),
        ServerLimits::default(),
        Arc::new(ServerMetrics::default()),
        AuthConfig::default(),
    )
}

async fn snapshot_call(channel: Channel) {
    let mut client = MemoryServiceClient::new(channel);
    let response = client.snapshot(SnapshotRequest {}).await.unwrap();
    assert_eq!(response.into_inner().record_count, 0);
}

#[tokio::test]
async fn grpc_service_accepts_unix_domain_socket_clients() {
    let store = temp_store("uds-store");
    let runtime = runtime_for(&store);
    let socket_path = temp_path("uds.sock");
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let listener = UnixListener::bind(&socket_path).unwrap();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let service = GrpcMemoryService::with_runtime(store.clone(), runtime).into_service();

    let server = tokio::spawn(async move {
        Server::builder()
            .add_service(service)
            .serve_with_incoming_shutdown(UnixListenerStream::new(listener), async move {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    let endpoint = Endpoint::try_from("http://[::]:50051").unwrap();
    let path = socket_path.clone();
    let channel = endpoint
        .connect_with_connector(service_fn(move |_| {
            let path = path.clone();
            async move {
                let stream = UnixStream::connect(path).await?;
                Ok::<_, std::io::Error>(TokioIo::new(stream))
            }
        }))
        .await
        .unwrap();
    snapshot_call(channel).await;

    let _ = shutdown_tx.send(());
    server.await.unwrap();
    fs::remove_file(socket_path).ok();
}

#[tokio::test]
async fn grpc_service_accepts_tls_clients() {
    let store = temp_store("tls-store");
    let runtime = runtime_for(&store);
    let server_cert = generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let server_identity =
        Identity::from_pem(server_cert.cert.pem(), server_cert.signing_key.serialize_pem());
    let tls = ServerTlsConfig::new().identity(server_identity);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let std_listener = listener.into_std().unwrap();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let service = GrpcMemoryService::with_runtime(store.clone(), runtime).into_service();

    let server = tokio::spawn(async move {
        Server::builder()
            .tls_config(tls)
            .unwrap()
            .add_service(service)
            .serve_with_incoming_shutdown(
                tokio_stream::wrappers::TcpListenerStream::new(
                    TcpListener::from_std(std_listener).unwrap(),
                ),
                async move {
                    let _ = shutdown_rx.await;
                },
            )
            .await
            .unwrap();
    });

    let channel = Endpoint::from_shared(format!("https://localhost:{}", addr.port()))
        .unwrap()
        .tls_config(
            ClientTlsConfig::new()
                .ca_certificate(Certificate::from_pem(server_cert.cert.pem()))
                .domain_name("localhost"),
        )
        .unwrap()
        .connect()
        .await
        .unwrap();
    snapshot_call(channel).await;

    let _ = shutdown_tx.send(());
    server.await.unwrap();
}

#[tokio::test]
async fn grpc_service_accepts_mutual_tls_clients() {
    let store = temp_store("mtls-store");
    let runtime = runtime_for(&store);
    let server_cert = generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let client_cert = generate_simple_self_signed(vec!["client".to_string()]).unwrap();

    let tls = ServerTlsConfig::new()
        .identity(Identity::from_pem(
            server_cert.cert.pem(),
            server_cert.signing_key.serialize_pem(),
        ))
        .client_ca_root(Certificate::from_pem(client_cert.cert.pem()));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let std_listener = listener.into_std().unwrap();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let service = GrpcMemoryService::with_runtime(store.clone(), runtime).into_service();

    let server = tokio::spawn(async move {
        Server::builder()
            .tls_config(tls)
            .unwrap()
            .add_service(service)
            .serve_with_incoming_shutdown(
                tokio_stream::wrappers::TcpListenerStream::new(
                    TcpListener::from_std(std_listener).unwrap(),
                ),
                async move {
                    let _ = shutdown_rx.await;
                },
            )
            .await
            .unwrap();
    });

    let channel = Endpoint::from_shared(format!("https://localhost:{}", addr.port()))
        .unwrap()
        .tls_config(
            ClientTlsConfig::new()
                .ca_certificate(Certificate::from_pem(server_cert.cert.pem()))
                .identity(Identity::from_pem(
                    client_cert.cert.pem(),
                    client_cert.signing_key.serialize_pem(),
                ))
                .domain_name("localhost"),
        )
        .unwrap()
        .connect()
        .await
        .unwrap();
    snapshot_call(channel).await;

    let _ = shutdown_tx.send(());
    server.await.unwrap();
}
