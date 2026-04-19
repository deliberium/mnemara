use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use mnemara_core::{
    EmbeddingProviderKind, EngineConfig, MemoryStore, RecallPlanningProfile, RecallPolicyProfile,
    RecallScorerKind, RecallScoringProfile,
};
use mnemara_server::{
    AuthConfig, AuthPermission, GrpcMemoryService, ServerLimits, ServerMetrics, ServerRuntime,
    TokenPolicy, serve_http_with_runtime,
};
use mnemara_store_sled::{SledMemoryStore, SledStoreConfig};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};

fn bind_addr_from_env() -> Result<SocketAddr, String> {
    let value = env::var("MNEMARA_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:50051".to_string());
    value
        .parse::<SocketAddr>()
        .map_err(|err| format!("invalid MNEMARA_BIND_ADDR '{value}': {err}"))
}

fn data_dir_from_env() -> PathBuf {
    env::var("MNEMARA_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./data/mnemara/sled"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeploymentProfile {
    Default,
    UdsLocal,
    TlsService,
    MtlsService,
}

#[derive(Debug, Clone)]
struct GrpcTlsFiles {
    cert_pem: Vec<u8>,
    key_pem: Vec<u8>,
    client_ca_pem: Option<Vec<u8>>,
}

fn deployment_profile_from_env() -> Result<DeploymentProfile, String> {
    let value = env::var("MNEMARA_DEPLOYMENT_PROFILE")
        .unwrap_or_else(|_| "default".to_string())
        .trim()
        .to_ascii_lowercase();
    match value.as_str() {
        "" | "default" => Ok(DeploymentProfile::Default),
        "uds-local" | "uds_local" => Ok(DeploymentProfile::UdsLocal),
        "tls-service" | "tls_service" => Ok(DeploymentProfile::TlsService),
        "mtls-service" | "mtls_service" => Ok(DeploymentProfile::MtlsService),
        _ => Err(format!(
            "invalid MNEMARA_DEPLOYMENT_PROFILE '{value}': expected default, uds-local, tls-service, or mtls-service"
        )),
    }
}

fn http_bind_addr_from_env(profile: DeploymentProfile) -> Result<Option<SocketAddr>, String> {
    let default = match profile {
        DeploymentProfile::Default => Some("127.0.0.1:50052"),
        DeploymentProfile::UdsLocal
        | DeploymentProfile::TlsService
        | DeploymentProfile::MtlsService => None,
    };
    let Some(value) = env::var("MNEMARA_HTTP_BIND_ADDR")
        .ok()
        .or_else(|| default.map(str::to_string))
    else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    value
        .parse::<SocketAddr>()
        .map(Some)
        .map_err(|err| format!("invalid MNEMARA_HTTP_BIND_ADDR '{value}': {err}"))
}

fn grpc_uds_path_from_env(profile: DeploymentProfile) -> Option<PathBuf> {
    env::var("MNEMARA_GRPC_UDS_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            (profile == DeploymentProfile::UdsLocal)
                .then(|| PathBuf::from("/tmp/mnemara-grpc.sock"))
        })
}

fn read_env_file(name: &str) -> Result<Vec<u8>, String> {
    let path = env::var(name).map_err(|_| format!("{name} is required"))?;
    std::fs::read(&path).map_err(|err| format!("failed to read {name} from {path}: {err}"))
}

fn grpc_tls_from_env(profile: DeploymentProfile) -> Result<Option<GrpcTlsFiles>, String> {
    if matches!(
        profile,
        DeploymentProfile::Default | DeploymentProfile::UdsLocal
    ) {
        return Ok(None);
    }

    let cert_pem = read_env_file("MNEMARA_TLS_CERT_PATH")?;
    let key_pem = read_env_file("MNEMARA_TLS_KEY_PATH")?;
    let client_ca_pem = match profile {
        DeploymentProfile::TlsService => env::var("MNEMARA_TLS_CLIENT_CA_PATH")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|path| {
                std::fs::read(&path).map_err(|err| {
                    format!("failed to read MNEMARA_TLS_CLIENT_CA_PATH from {path}: {err}")
                })
            })
            .transpose()?,
        DeploymentProfile::MtlsService => Some(read_env_file("MNEMARA_TLS_CLIENT_CA_PATH")?),
        DeploymentProfile::Default | DeploymentProfile::UdsLocal => None,
    };
    Ok(Some(GrpcTlsFiles {
        cert_pem,
        key_pem,
        client_ca_pem,
    }))
}

fn parse_usize(raw: Option<String>, name: &str, default: usize) -> Result<usize, String> {
    match raw {
        Some(value) => value
            .parse::<usize>()
            .map_err(|err| format!("invalid {name} '{value}': {err}")),
        None => Ok(default),
    }
}

fn env_usize(name: &str, default: usize) -> Result<usize, String> {
    parse_usize(env::var(name).ok(), name, default)
}

fn parse_u32(raw: Option<String>, name: &str, default: u32) -> Result<u32, String> {
    match raw {
        Some(value) => value
            .parse::<u32>()
            .map_err(|err| format!("invalid {name} '{value}': {err}")),
        None => Ok(default),
    }
}

fn parse_u16(raw: Option<String>, name: &str, default: u16) -> Result<u16, String> {
    match raw {
        Some(value) => value
            .parse::<u16>()
            .map_err(|err| format!("invalid {name} '{value}': {err}")),
        None => Ok(default),
    }
}

fn parse_recall_scoring_profile(raw: Option<String>) -> Result<RecallScoringProfile, String> {
    let Some(value) = raw
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(RecallScoringProfile::Balanced);
    };

    match value.as_str() {
        "balanced" => Ok(RecallScoringProfile::Balanced),
        "lexical" | "lexical-first" | "lexical_first" => Ok(RecallScoringProfile::LexicalFirst),
        "importance" | "importance-first" | "importance_first" => {
            Ok(RecallScoringProfile::ImportanceFirst)
        }
        _ => Err(format!(
            "invalid MNEMARA_RECALL_SCORING_PROFILE '{value}': expected balanced, lexical-first, or importance-first"
        )),
    }
}

fn parse_recall_scorer_kind(raw: Option<String>) -> Result<RecallScorerKind, String> {
    let Some(value) = raw
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(RecallScorerKind::Profile);
    };

    match value.as_str() {
        "profile" => Ok(RecallScorerKind::Profile),
        "curated" => Ok(RecallScorerKind::Curated),
        _ => Err(format!(
            "invalid MNEMARA_RECALL_SCORER_KIND '{value}': expected profile or curated"
        )),
    }
}

fn parse_recall_planning_profile(raw: Option<String>) -> Result<RecallPlanningProfile, String> {
    let Some(value) = raw
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(RecallPlanningProfile::FastPath);
    };

    match value.as_str() {
        "fast" | "fast-path" | "fast_path" => Ok(RecallPlanningProfile::FastPath),
        "continuity-aware" | "continuity_aware" => Ok(RecallPlanningProfile::ContinuityAware),
        _ => Err(format!(
            "invalid MNEMARA_RECALL_PLANNING_PROFILE '{value}': expected fast_path or continuity_aware"
        )),
    }
}

fn parse_recall_policy_profile(raw: Option<String>) -> Result<RecallPolicyProfile, String> {
    let Some(value) = raw
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(RecallPolicyProfile::General);
    };

    match value.as_str() {
        "general" => Ok(RecallPolicyProfile::General),
        "support" => Ok(RecallPolicyProfile::Support),
        "research" => Ok(RecallPolicyProfile::Research),
        "assistant" => Ok(RecallPolicyProfile::Assistant),
        "autonomous-agent" | "autonomous_agent" => Ok(RecallPolicyProfile::AutonomousAgent),
        _ => Err(format!(
            "invalid MNEMARA_RECALL_POLICY_PROFILE '{value}': expected general, support, research, assistant, or autonomous_agent"
        )),
    }
}

fn parse_embedding_provider_kind(raw: Option<String>) -> Result<EmbeddingProviderKind, String> {
    let Some(value) = raw
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(EmbeddingProviderKind::Disabled);
    };

    match value.as_str() {
        "disabled" => Ok(EmbeddingProviderKind::Disabled),
        "deterministic-local" | "deterministic_local" | "local" => {
            Ok(EmbeddingProviderKind::DeterministicLocal)
        }
        _ => Err(format!(
            "invalid MNEMARA_EMBEDDING_PROVIDER_KIND '{value}': expected disabled or deterministic-local"
        )),
    }
}

fn engine_config_from<F>(get: F) -> Result<EngineConfig, String>
where
    F: Fn(&str) -> Option<String>,
{
    let mut config = EngineConfig::default();
    config.compaction.summarize_after_record_count = parse_usize(
        get("MNEMARA_COMPACTION_SUMMARIZE_AFTER_RECORD_COUNT"),
        "MNEMARA_COMPACTION_SUMMARIZE_AFTER_RECORD_COUNT",
        config.compaction.summarize_after_record_count,
    )?;
    config.compaction.cold_archive_after_days = parse_u32(
        get("MNEMARA_COMPACTION_COLD_ARCHIVE_AFTER_DAYS"),
        "MNEMARA_COMPACTION_COLD_ARCHIVE_AFTER_DAYS",
        config.compaction.cold_archive_after_days,
    )?;
    config
        .compaction
        .cold_archive_importance_threshold_per_mille = parse_u16(
        get("MNEMARA_COMPACTION_COLD_ARCHIVE_IMPORTANCE_THRESHOLD_PER_MILLE"),
        "MNEMARA_COMPACTION_COLD_ARCHIVE_IMPORTANCE_THRESHOLD_PER_MILLE",
        config
            .compaction
            .cold_archive_importance_threshold_per_mille,
    )?;
    config.recall_scorer_kind = parse_recall_scorer_kind(get("MNEMARA_RECALL_SCORER_KIND"))?;
    config.recall_scoring_profile =
        parse_recall_scoring_profile(get("MNEMARA_RECALL_SCORING_PROFILE"))?;
    config.recall_planning_profile =
        parse_recall_planning_profile(get("MNEMARA_RECALL_PLANNING_PROFILE"))?;
    config.recall_policy_profile =
        parse_recall_policy_profile(get("MNEMARA_RECALL_POLICY_PROFILE"))?;
    config.embedding_provider_kind =
        parse_embedding_provider_kind(get("MNEMARA_EMBEDDING_PROVIDER_KIND"))?;
    config.embedding_dimensions = parse_usize(
        get("MNEMARA_EMBEDDING_DIMENSIONS"),
        "MNEMARA_EMBEDDING_DIMENSIONS",
        config.embedding_dimensions,
    )?;
    config.graph_expansion_max_hops = parse_u16(
        get("MNEMARA_GRAPH_EXPANSION_MAX_HOPS"),
        "MNEMARA_GRAPH_EXPANSION_MAX_HOPS",
        u16::from(config.graph_expansion_max_hops),
    )?
    .try_into()
    .map_err(|_| {
        "invalid MNEMARA_GRAPH_EXPANSION_MAX_HOPS: expected a value between 0 and 255".to_string()
    })?;
    Ok(config)
}

fn engine_config_from_env() -> Result<EngineConfig, String> {
    engine_config_from(|name| env::var(name).ok())
}

fn parse_auth_permission(value: &str) -> Result<AuthPermission, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "read" => Ok(AuthPermission::Read),
        "write" => Ok(AuthPermission::Write),
        "admin" => Ok(AuthPermission::Admin),
        "metrics" => Ok(AuthPermission::Metrics),
        other => Err(format!("unknown auth permission '{other}'")),
    }
}

fn token_policies_from_env() -> Result<Vec<TokenPolicy>, String> {
    let Some(raw) = env::var("MNEMARA_AUTH_TOKENS")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(Vec::new());
    };

    raw.split(';')
        .filter(|entry| !entry.trim().is_empty())
        .map(|entry| {
            let (token, permissions) = entry
                .split_once('=')
                .ok_or_else(|| format!("invalid MNEMARA_AUTH_TOKENS entry '{entry}': expected token=perm1,perm2"))?;
            let token = token.trim();
            if token.is_empty() {
                return Err(format!("invalid MNEMARA_AUTH_TOKENS entry '{entry}': token is required"));
            }
            let permissions = permissions
                .split(',')
                .filter(|value| !value.trim().is_empty())
                .map(parse_auth_permission)
                .collect::<Result<Vec<_>, _>>()?;
            if permissions.is_empty() {
                return Err(format!("invalid MNEMARA_AUTH_TOKENS entry '{entry}': at least one permission is required"));
            }
            Ok(TokenPolicy {
                token: token.to_string(),
                permissions,
            })
        })
        .collect()
}

async fn serve_grpc_transport(
    bind_addr: SocketAddr,
    uds_path: Option<PathBuf>,
    tls: Option<GrpcTlsFiles>,
    store: Arc<SledMemoryStore>,
    runtime: ServerRuntime,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut builder = Server::builder();
    if let Some(tls) = tls {
        let mut tls_config =
            ServerTlsConfig::new().identity(Identity::from_pem(tls.cert_pem, tls.key_pem));
        if let Some(client_ca_pem) = tls.client_ca_pem {
            tls_config = tls_config.client_ca_root(Certificate::from_pem(client_ca_pem));
        }
        builder = builder.tls_config(tls_config)?;
    }

    let service = GrpcMemoryService::with_runtime(store, runtime).into_service();
    if let Some(uds_path) = uds_path {
        if uds_path.exists() {
            std::fs::remove_file(&uds_path).map_err(|err| {
                std::io::Error::other(format!(
                    "failed to remove existing UDS socket {}: {err}",
                    uds_path.display()
                ))
            })?;
        }
        let listener = tokio::net::UnixListener::bind(&uds_path)?;
        builder
            .add_service(service)
            .serve_with_incoming(UnixListenerStream::new(listener))
            .await?;
        Ok(())
    } else {
        builder.add_service(service).serve(bind_addr).await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let profile = deployment_profile_from_env().map_err(std::io::Error::other)?;
    let bind_addr = bind_addr_from_env().map_err(std::io::Error::other)?;
    let http_bind_addr = http_bind_addr_from_env(profile).map_err(std::io::Error::other)?;
    let grpc_uds_path = grpc_uds_path_from_env(profile);
    let grpc_tls = grpc_tls_from_env(profile).map_err(std::io::Error::other)?;
    let data_dir = data_dir_from_env();
    let limits = ServerLimits {
        max_http_body_bytes: env_usize("MNEMARA_MAX_HTTP_BODY_BYTES", 64 * 1024)
            .map_err(std::io::Error::other)?,
        max_batch_upsert_requests: env_usize("MNEMARA_MAX_BATCH_UPSERT_REQUESTS", 128)
            .map_err(std::io::Error::other)?,
        max_recall_items: env_usize("MNEMARA_MAX_RECALL_ITEMS", 64)
            .map_err(std::io::Error::other)?,
        max_query_text_bytes: env_usize("MNEMARA_MAX_QUERY_TEXT_BYTES", 4 * 1024)
            .map_err(std::io::Error::other)?,
        max_record_content_bytes: env_usize("MNEMARA_MAX_RECORD_CONTENT_BYTES", 32 * 1024)
            .map_err(std::io::Error::other)?,
        max_labels_per_scope: env_usize("MNEMARA_MAX_LABELS_PER_SCOPE", 32)
            .map_err(std::io::Error::other)?,
        max_inflight_reads: env_usize("MNEMARA_MAX_INFLIGHT_READS", 64)
            .map_err(std::io::Error::other)?,
        max_inflight_writes: env_usize("MNEMARA_MAX_INFLIGHT_WRITES", 32)
            .map_err(std::io::Error::other)?,
        max_inflight_admin: env_usize("MNEMARA_MAX_INFLIGHT_ADMIN", 8)
            .map_err(std::io::Error::other)?,
        max_queued_requests: env_usize("MNEMARA_MAX_QUEUED_REQUESTS", 256)
            .map_err(std::io::Error::other)?,
        max_tenant_inflight: env_usize("MNEMARA_MAX_TENANT_INFLIGHT", 16)
            .map_err(std::io::Error::other)?,
        queue_wait_timeout_ms: env_usize("MNEMARA_QUEUE_WAIT_TIMEOUT_MS", 2_000)
            .map_err(std::io::Error::other)? as u64,
        trace_retention: env_usize("MNEMARA_TRACE_RETENTION", 256)
            .map_err(std::io::Error::other)?,
    };
    let auth = AuthConfig {
        bearer_token: env::var("MNEMARA_AUTH_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty()),
        protect_metrics: env::var("MNEMARA_AUTH_PROTECT_METRICS")
            .ok()
            .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false),
        token_policies: token_policies_from_env().map_err(std::io::Error::other)?,
    };
    let engine_config = engine_config_from_env().map_err(std::io::Error::other)?;

    let store = Arc::new(
        SledMemoryStore::open(SledStoreConfig::new(&data_dir).with_engine_config(engine_config))
            .map_err(|err| {
                std::io::Error::other(format!(
                    "failed to open Mnemara data dir {}: {err}",
                    data_dir.display()
                ))
            })?,
    );

    println!(
        "mnemara-server profile={:?} grpc={} uds={} http={} using data dir {}",
        profile,
        bind_addr,
        grpc_uds_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "disabled".to_string()),
        http_bind_addr
            .map(|addr| addr.to_string())
            .unwrap_or_else(|| "disabled".to_string()),
        data_dir.display()
    );

    let runtime = ServerRuntime::new(
        store.backend_kind(),
        limits.clone(),
        Arc::new(ServerMetrics::default()),
        auth.clone(),
    );

    match http_bind_addr {
        Some(http_bind_addr) => {
            tokio::try_join!(
                async {
                    serve_grpc_transport(
                        bind_addr,
                        grpc_uds_path.clone(),
                        grpc_tls.clone(),
                        Arc::clone(&store),
                        runtime.clone(),
                    )
                    .await
                    .map_err(|err| std::io::Error::other(err.to_string()))
                },
                serve_http_with_runtime(http_bind_addr, Arc::clone(&store), runtime.clone()),
            )?;
        }
        None => {
            serve_grpc_transport(
                bind_addr,
                grpc_uds_path,
                grpc_tls,
                Arc::clone(&store),
                runtime,
            )
            .await
            .map_err(|err| std::io::Error::other(err.to_string()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::engine_config_from;
    use mnemara_core::{
        EmbeddingProviderKind, RecallPlanningProfile, RecallPolicyProfile, RecallScorerKind,
        RecallScoringProfile,
    };
    use std::collections::HashMap;

    #[test]
    fn engine_config_from_defaults_when_env_is_absent() {
        let config = engine_config_from(|_| None).unwrap();
        assert_eq!(config.recall_scorer_kind, RecallScorerKind::Profile);
        assert_eq!(
            config.recall_scoring_profile,
            RecallScoringProfile::Balanced
        );
        assert_eq!(
            config.embedding_provider_kind,
            EmbeddingProviderKind::Disabled
        );
        assert_eq!(config.embedding_dimensions, 64);
        assert_eq!(
            config.recall_planning_profile,
            RecallPlanningProfile::FastPath
        );
        assert_eq!(config.recall_policy_profile, RecallPolicyProfile::General);
        assert_eq!(config.graph_expansion_max_hops, 1);
        assert_eq!(config.compaction.summarize_after_record_count, 50);
        assert_eq!(config.compaction.cold_archive_after_days, 0);
        assert_eq!(
            config
                .compaction
                .cold_archive_importance_threshold_per_mille,
            250
        );
    }

    #[test]
    fn engine_config_from_reads_scoring_and_compaction_overrides() {
        let vars = HashMap::from([
            (
                "MNEMARA_RECALL_SCORER_KIND".to_string(),
                "curated".to_string(),
            ),
            (
                "MNEMARA_RECALL_SCORING_PROFILE".to_string(),
                "importance-first".to_string(),
            ),
            (
                "MNEMARA_RECALL_PLANNING_PROFILE".to_string(),
                "continuity_aware".to_string(),
            ),
            (
                "MNEMARA_RECALL_POLICY_PROFILE".to_string(),
                "research".to_string(),
            ),
            (
                "MNEMARA_GRAPH_EXPANSION_MAX_HOPS".to_string(),
                "0".to_string(),
            ),
            (
                "MNEMARA_EMBEDDING_PROVIDER_KIND".to_string(),
                "deterministic-local".to_string(),
            ),
            ("MNEMARA_EMBEDDING_DIMENSIONS".to_string(), "96".to_string()),
            (
                "MNEMARA_COMPACTION_SUMMARIZE_AFTER_RECORD_COUNT".to_string(),
                "8".to_string(),
            ),
            (
                "MNEMARA_COMPACTION_COLD_ARCHIVE_AFTER_DAYS".to_string(),
                "14".to_string(),
            ),
            (
                "MNEMARA_COMPACTION_COLD_ARCHIVE_IMPORTANCE_THRESHOLD_PER_MILLE".to_string(),
                "175".to_string(),
            ),
        ]);

        let config = engine_config_from(|name| vars.get(name).cloned()).unwrap();
        assert_eq!(config.recall_scorer_kind, RecallScorerKind::Curated);
        assert_eq!(
            config.recall_scoring_profile,
            RecallScoringProfile::ImportanceFirst
        );
        assert_eq!(
            config.recall_planning_profile,
            RecallPlanningProfile::ContinuityAware
        );
        assert_eq!(config.recall_policy_profile, RecallPolicyProfile::Research);
        assert_eq!(
            config.embedding_provider_kind,
            EmbeddingProviderKind::DeterministicLocal
        );
        assert_eq!(config.embedding_dimensions, 96);
        assert_eq!(config.graph_expansion_max_hops, 0);
        assert_eq!(config.compaction.summarize_after_record_count, 8);
        assert_eq!(config.compaction.cold_archive_after_days, 14);
        assert_eq!(
            config
                .compaction
                .cold_archive_importance_threshold_per_mille,
            175
        );
    }

    #[test]
    fn engine_config_from_rejects_invalid_scoring_profile() {
        let error = engine_config_from(|name| {
            (name == "MNEMARA_RECALL_SCORING_PROFILE").then(|| "nonsense".to_string())
        })
        .unwrap_err();
        assert!(error.contains("invalid MNEMARA_RECALL_SCORING_PROFILE"));
    }

    #[test]
    fn engine_config_from_rejects_invalid_scorer_kind() {
        let error = engine_config_from(|name| {
            (name == "MNEMARA_RECALL_SCORER_KIND").then(|| "nonsense".to_string())
        })
        .unwrap_err();
        assert!(error.contains("invalid MNEMARA_RECALL_SCORER_KIND"));
    }

    #[test]
    fn engine_config_from_rejects_invalid_planning_profile() {
        let error = engine_config_from(|name| {
            (name == "MNEMARA_RECALL_PLANNING_PROFILE").then(|| "nonsense".to_string())
        })
        .unwrap_err();
        assert!(error.contains("invalid MNEMARA_RECALL_PLANNING_PROFILE"));
    }

    #[test]
    fn engine_config_from_rejects_invalid_policy_profile() {
        let error = engine_config_from(|name| {
            (name == "MNEMARA_RECALL_POLICY_PROFILE").then(|| "nonsense".to_string())
        })
        .unwrap_err();
        assert!(error.contains("invalid MNEMARA_RECALL_POLICY_PROFILE"));
    }

    #[test]
    fn engine_config_from_rejects_out_of_range_graph_hops() {
        let error = engine_config_from(|name| {
            (name == "MNEMARA_GRAPH_EXPANSION_MAX_HOPS").then(|| "999".to_string())
        })
        .unwrap_err();
        assert!(error.contains("invalid MNEMARA_GRAPH_EXPANSION_MAX_HOPS"));
    }

    #[test]
    fn engine_config_from_rejects_invalid_embedding_provider_kind() {
        let error = engine_config_from(|name| {
            (name == "MNEMARA_EMBEDDING_PROVIDER_KIND").then(|| "nonsense".to_string())
        })
        .unwrap_err();
        assert!(error.contains("invalid MNEMARA_EMBEDDING_PROVIDER_KIND"));
    }
}
