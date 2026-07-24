# R3-BASE-01 Evidence — Repository identity

- Baseline commit: 6875625349a4a067dbb41d1249adae4143fc35d9
- Result commit: 6875625349a4a067dbb41d1249adae4143fc35d9
- Files changed: none (evidence only)

## git status --short

```
?? docs/provider-adapter-v1/openbuild_provider_adapter_production_closure_remediation_6875625.md
```

## git branch --show-current

```
feat/provider-adapter
```

## git rev-parse HEAD

```
6875625349a4a067dbb41d1249adae4143fc35d9
```

HEAD matches the required baseline.

## git fsck --full

```
dangling commit 15851221c128d564dc7d6efb419390acf247f052
dangling commit 609224bbbd4b96c7c5b5bb064561ea3447ff9f96
dangling commit b293970baf708d99b34704df9f569a3868bfe5b3
dangling commit b39872a1ab23ef0fe8af1d5b4a7850914dbdc7bd
dangling blob a91a32d025e9d0f48b36d0e7f2dce7da8b6261f0
dangling blob f51e4823e03f5bad70b311055e792d4855d167b9
dangling commit 18a25c6fc6531b7e70184e7cb2a6660a39da9ee1
dangling blob e6a628a165960a681817d6edb541b5d14c0d8611
dangling blob bc3b3a6953ebd61415c56fc5d75030fe6eb40ba7
dangling commit 3551e65d0056ae880fb301639ac7be98aa9f770d
dangling commit 1a5788f9afc1267749178a27d682ab15033c8d2c
dangling commit a264d5b8536815062e9688c4cd71dab2d9998e4f
dangling commit 1eedbd7d785a7e0f7292924560b2a8e76a275e5f
dangling commit 927f6d06dbef76f536f13d7ae739b8d8dbabdbd7
```

Dangling commits/blobs are expected after forced pushes. No corrupt objects.

## git submodule status --recursive

No output; no submodules configured.

## git diff --check origin/main...HEAD

```
fatal: ambiguous argument 'origin/main...HEAD': unknown revision or path not in the working tree.
```

`origin/main` is not fetched locally. This is a baseline observation — the comparison reference is missing from the local clone.

## Deviations

- `git diff --check origin/main...HEAD` cannot run locally because `origin/main` is not available. The check will be run in CI or after fetching the remote default branch.
