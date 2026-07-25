use std::sync::Arc;
use std::time::Duration;

use xai_grok_provider::types::{ProviderDefaults, ProviderId};
use xai_grok_shell::agent::provider_catalog::{
    ModelCatalogSnapshot, ProviderCatalogEntry, ProviderCatalogService, ProviderCatalogState,
    RefreshStrategy::ForceRefresh, parse_ollama_tags_models,
};

fn dummy_defaults() -> ProviderDefaults {
    let mut d = ProviderDefaults::default();
    d.id = ProviderId::new("test");
    d.name = "test".into();
    d.base_url = "http://127.0.0.1:0/v1/".into();
    d
}

#[tokio::test]
async fn persisted_stale_models_visible_after_bootstrap() {
    let svc = ProviderCatalogService::new();
    let pid = ProviderId::new("test");

    let persisted = ModelCatalogSnapshot {
        catalog_revision: 0,
        providers: [(
            pid.clone(),
            ProviderCatalogEntry {
                provider_id: pid.clone(),
                state: ProviderCatalogState::Fresh,
                fetched_at: Some(std::time::SystemTime::now()),
                source_url: "http://example.test/models".into(),
                models: vec![],
                error_summary: None,
            },
        )]
        .into(),
    };

    svc.bootstrap_from_snapshot(persisted).await;

    let snap = svc.snapshot().await;
    let entry = snap.providers.get(&pid).unwrap();
    assert_eq!(
        entry.state,
        ProviderCatalogState::Stale,
        "R3-RED-08 #1: after bootstrap, models should be Stale"
    );
}

#[tokio::test]
async fn refresh_failure_preserves_prior_models() {
    let svc = ProviderCatalogService::new();
    let pid = ProviderId::new("test");

    let persisted = ModelCatalogSnapshot {
        catalog_revision: 0,
        providers: [(
            pid.clone(),
            ProviderCatalogEntry {
                provider_id: pid.clone(),
                state: ProviderCatalogState::Fresh,
                fetched_at: Some(std::time::SystemTime::now()),
                source_url: "http://example.test/models".into(),
                models: vec![],
                error_summary: None,
            },
        )]
        .into(),
    };
    svc.bootstrap_from_snapshot(persisted).await;

    svc.refresh_provider(
        pid.clone(),
        "http://127.0.0.1:1/models",
        parse_ollama_tags_models,
        &dummy_defaults(),
        ForceRefresh,
        Duration::from_secs(300),
    )
    .await;

    let snap = svc.snapshot().await;
    let entry = snap.providers.get(&pid).unwrap();
    assert!(
        entry.models.is_empty(),
        "R3-RED-08 #2: refresh failure should preserve prior models"
    );
    match &entry.state {
        ProviderCatalogState::Failed(_) => {}
        other => panic!("R3-RED-08 #2: expected Failed state, got {other:?}"),
    }
}

#[tokio::test]
async fn provider_deletion_emits_revision_notification() {
    let svc = Arc::new(ProviderCatalogService::new());
    let pid = ProviderId::new("test");

    let rx = svc.subscribe_catalog_revision();

    let persisted = ModelCatalogSnapshot {
        catalog_revision: 0,
        providers: [(
            pid.clone(),
            ProviderCatalogEntry {
                provider_id: pid.clone(),
                state: ProviderCatalogState::Stale,
                fetched_at: None,
                source_url: "http://example.test/models".into(),
                models: vec![],
                error_summary: None,
            },
        )]
        .into(),
    };
    svc.bootstrap_from_snapshot(persisted).await;

    let rev_before = *rx.borrow();

    let reg: indexmap::IndexMap<ProviderId, Arc<xai_grok_provider::provider::ConfiguredProvider>> =
        indexmap::IndexMap::new();

    svc.refresh_changed(
        &reg,
        |_pid| None::<(String, ProviderDefaults)>,
        Duration::from_secs(300),
    )
    .await;

    let rev_after = *rx.borrow();
    assert!(
        rev_after > rev_before,
        "R3-RED-08 #3: provider deletion must emit revision (rev {rev_after} <= {rev_before})"
    );

    let snap = svc.snapshot().await;
    assert!(
        !snap.providers.contains_key(&pid),
        "R3-RED-08 #3: provider should be removed"
    );
}

#[tokio::test]
async fn superseded_refresh_cannot_publish_after_newer_revision() {
    let svc = Arc::new(ProviderCatalogService::new());
    let defaults = dummy_defaults();

    let slow_pid = ProviderId::new("slow");

    let (slow_shutdown, slow_rx) = tokio::sync::oneshot::channel::<()>();
    let slow_app = axum::Router::new().route(
        "/v1/models",
        axum::routing::get(|| async {
            tokio::time::sleep(Duration::from_millis(500)).await;
            axum::Json(serde_json::json!({"models": [{"name": "slow-model"}]}))
        }),
    );
    let slow_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let slow_addr = slow_listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(slow_listener, slow_app)
            .with_graceful_shutdown(async {
                slow_rx.await.ok();
            })
            .await;
    });
    let slow_url = format!("http://{slow_addr}/v1/models");

    // Bootstrap empty, then trigger refresh_all for slow_pid only
    let empty = ModelCatalogSnapshot {
        catalog_revision: 0,
        providers: Default::default(),
    };
    svc.bootstrap_from_snapshot(empty).await;

    svc.refresh_all(
        std::slice::from_ref(&slow_pid),
        |pid| {
            if pid == &slow_pid {
                Some((slow_url.clone(), defaults.clone()))
            } else {
                None
            }
        },
        Duration::from_secs(300),
    )
    .await;

    // Small delay to let the spawned task start, then inject newer snapshot
    tokio::time::sleep(Duration::from_millis(50)).await;

    let newer = ModelCatalogSnapshot {
        catalog_revision: 100,
        providers: [(
            slow_pid.clone(),
            ProviderCatalogEntry {
                provider_id: slow_pid.clone(),
                state: ProviderCatalogState::Fresh,
                fetched_at: Some(std::time::SystemTime::now()),
                source_url: slow_url.clone(),
                models: vec![],
                error_summary: None,
            },
        )]
        .into(),
    };
    svc.bootstrap_from_snapshot(newer).await;

    svc.join_active_refresh().await;

    let snap = svc.snapshot().await;
    let slow_entry = snap.providers.get(&slow_pid).unwrap();
    assert!(
        slow_entry.models.is_empty(),
        "R3-RED-08 #4: superseded refresh must NOT overwrite — \
         models = {:?} (BUG: spawned task didn't check revision)",
        slow_entry.models,
    );

    let _ = slow_shutdown.send(());
}

#[tokio::test]
async fn shutdown_awaits_active_refresh() {
    let svc = Arc::new(ProviderCatalogService::new());
    let defaults = dummy_defaults();

    let (server_shutdown, server_rx) = tokio::sync::oneshot::channel::<()>();
    let pid = ProviderId::new("test");

    let app = axum::Router::new().route(
        "/v1/models",
        axum::routing::get(move || async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            axum::Json(serde_json::json!({"models": [{"name": "m1"}]}))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                server_rx.await.ok();
            })
            .await;
    });
    let url = format!("http://{addr}/v1/models");

    svc.refresh_all(
        &[pid],
        |_| Some((url.clone(), defaults.clone())),
        Duration::from_secs(300),
    )
    .await;

    let result = svc.shutdown().await;
    assert!(
        result.is_ok(),
        "R3-RED-08 #5: shutdown must succeed: {:?}",
        result
    );

    let _ = server_shutdown.send(());
}
