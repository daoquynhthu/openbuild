use std::sync::Arc;

use xai_grok_provider::types::ProviderId;
use xai_grok_shell::agent::provider_catalog::{
    ModelCatalogSnapshot, ProviderCatalogEntry, ProviderCatalogService, ProviderCatalogState,
};

fn sample_entry(provider_id: ProviderId) -> ProviderCatalogEntry {
    ProviderCatalogEntry {
        provider_id,
        state: ProviderCatalogState::Fresh,
        fetched_at: Some(std::time::SystemTime::now()),
        source_url: "http://example.test/models".into(),
        models: vec![],
        error_summary: None,
    }
}

#[tokio::test]
async fn persist_and_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.json");
    let svc = ProviderCatalogService::new();

    let mut snap = ModelCatalogSnapshot {
        catalog_revision: 1,
        providers: Default::default(),
    };
    snap.providers.insert(
        ProviderId::new("test"),
        sample_entry(ProviderId::new("test")),
    );
    svc.bootstrap_from_snapshot(snap).await;
    svc.persist_snapshot(&path).await.unwrap();
    assert!(path.exists());

    let loaded = ProviderCatalogService::load_snapshot(&path)
        .await
        .expect("must load");
    assert!(loaded.providers.contains_key(&ProviderId::new("test")));
}

#[tokio::test]
async fn persist_leaves_no_temp_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.json");
    let svc = ProviderCatalogService::new();

    let mut snap = ModelCatalogSnapshot {
        catalog_revision: 1,
        providers: Default::default(),
    };
    snap.providers.insert(
        ProviderId::new("test"),
        sample_entry(ProviderId::new("test")),
    );
    svc.bootstrap_from_snapshot(snap).await;
    svc.persist_snapshot(&path).await.unwrap();

    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    let tmp_count = entries
        .iter()
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp_"))
        .count();
    assert_eq!(tmp_count, 0, "R3-RED-12: no .tmp files after persist");
}

#[tokio::test]
async fn persist_overwrites_existing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.json");
    let svc = ProviderCatalogService::new();

    // First persist
    let snap = ModelCatalogSnapshot {
        catalog_revision: 1,
        providers: Default::default(),
    };
    svc.bootstrap_from_snapshot(snap).await;
    svc.persist_snapshot(&path).await.unwrap();

    // Second persist
    svc.persist_snapshot(&path).await.unwrap();
    let loaded = ProviderCatalogService::load_snapshot(&path)
        .await
        .expect("must load after overwrite");
    assert!(
        loaded.providers.is_empty(),
        "R3-RED-12: overwritten catalog must be loadable"
    );
}

#[tokio::test]
async fn concurrent_writers_do_not_corrupt() {
    let dir = Arc::new(tempfile::tempdir().unwrap());
    let path = dir.path().join("catalog.json");

    let mut handles = Vec::new();
    for _ in 0..5 {
        let path_clone = path.clone();
        handles.push(tokio::spawn(async move {
            let svc = ProviderCatalogService::new();
            let mut snap = ModelCatalogSnapshot {
                catalog_revision: 1,
                providers: Default::default(),
            };
            snap.providers.insert(
                ProviderId::new("test"),
                sample_entry(ProviderId::new("test")),
            );
            svc.bootstrap_from_snapshot(snap).await;
            let _ = svc.persist_snapshot(&path_clone).await;
        }));
    }

    for h in handles {
        let _ = h.await;
    }

    let loaded = ProviderCatalogService::load_snapshot(&path).await;
    assert!(
        loaded.is_some(),
        "R3-RED-12: concurrent writes must not corrupt"
    );
}

#[tokio::test]
async fn persist_fails_on_invalid_path() {
    let svc = ProviderCatalogService::new();
    let bad_path = std::path::Path::new("/nonexistent/deep/path/catalog.json");
    let result = svc.persist_snapshot(bad_path).await;
    assert!(
        result.is_err(),
        "R3-RED-12: persist to nonexistent directory must fail"
    );
}
