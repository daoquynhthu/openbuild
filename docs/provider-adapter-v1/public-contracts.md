# Provider Adapter V1 — Public API Contracts

> Exact public type fields, ownership, fallible constructors, error variants,
> and test names that Phase 2 must implement.

## Validated IDs

```rust
// ProviderId, RouteId, ModelId validate non-empty trimmed values.
// Reject whitespace-only strings and invalid separators.
```

## ProviderError Variants

- `InvalidProviderId` — provider ID is empty or contains invalid characters
- `InvalidRouteId` — route ID is empty or contains invalid characters
- `DuplicateProvider` — definition registration with duplicate ID
- `DuplicateRoute` — route with duplicate ID
- `UnknownProvider` — referenced provider not found
- `UnknownRoute` — referenced route not found
- `InvalidEndpoint` — malformed URL, unsupported scheme, embedded credentials
- `MissingCredential` — required credential not available
- `InvalidHeader` — header name/value violates HTTP token rules
- `UnknownProtocol` — protocol ID not recognized
- `AmbiguousModel` — bare model ID matches more than one provider
- `Config` — configuration-level error

Error display text must never contain secret values.

## Mandatory Test Cases

| Test Name | What It Proves |
|-----------|----------------|
| `configured_provider_contains_multiple_routes` | ConfiguredProvider owns IndexMap of routes, not a single route |
| `registry_snapshot_revision_increments_once` | Each successful rebuild increments revision by exactly 1 |
| `registry_snapshot_order_is_deterministic` | Route/provider iteration order is deterministic across rebuilds |
| `openai_selector_routes_chat_model` | OpenAI route selector sends known chat model to Chat Completions route |
| `openai_selector_routes_responses_model` | OpenAI route selector sends known responses model to Responses route |
| `selector_rejects_unknown_route` | Route selector returns error for model not in its table |
| `provider_config_debug_redacts_secret` | Debug output does not contain sentinel secret values |
| `invalid_endpoint_never_falls_back_to_localhost` | Malformed endpoint URL produces error, not http://localhost |

## Phase 2 Contract

Phase 2 must implement all of the above in `xai-grok-provider`:
1. ID validation types and tests
2. Declarative AuthPolicy (AD-05)
3. Fallible safe endpoint rendering
4. Route per AD-02 shape (no route-level framing)
5. ConfiguredProvider with route set and RouteSelector
6. Transactional registry snapshots (AD-04)
7. Focused `pub` API with doc comments and `cargo doc` passing

No compile-failing tests are added in Phase 1. Each test is created immediately
before its implementation under the normal red/green task cycle in Phase 2.
