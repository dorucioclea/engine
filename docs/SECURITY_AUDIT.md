# Security Audit Notes — Second Rust Workspace Review

This document records the issues identified and fixed during the second deep
security audit of the `orch8.io/engine` Rust workspace.

## Fixed Issues

### 1. `orch8-publisher` — tenant ID path traversal & manifest trust boundary

**Risk:** `SequencePublisher` accepted arbitrary `tenant_id` strings and used
them directly in CDN object paths. A malicious tenant ID such as
`../../other-tenant` could overwrite another tenant's sequences or manifest.
Additionally, `publish_sequence` did not verify that the sequence belonged to
the publisher's tenant, and `publish_manifest` accepted caller-supplied
sequence URLs and signing keys without trust-boundary checks.

**Fix:**
- `SequencePublisher::new` now returns `Result<Self, PublishError>` and rejects
  empty tenant IDs and path metacharacters (`/`, `\`, `..`).
- `publish_sequence` rejects sequences whose `tenant_id` does not match the
  publisher.
- `publish_manifest` rejects sequence URLs outside the publisher's tenant
  prefix and rejects `other_keys` entries that shadow the generator's
  `signing_key_id`.
- `ManifestGenerator::generate` now detects duplicate `key_id` values among
  `other_keys` (and the generator key) instead of silently deduplicating them.

**Tests:**
- Unit tests in `orch8-publisher/src/publish.rs` for invalid tenant IDs,
  tenant mismatch, outside-tenant URLs, and generator-key shadowing.
- Unit tests in `orch8-publisher/src/manifest.rs` for duplicate signing key
  rejection.
- Updated integration tests in `orch8-publisher/tests/publisher_coverage.rs` to
  align sequence/manifest tenants with publisher tenants.

### 2. `orch8-mobile` — telemetry endpoint SSRF

**Risk:** `MobileEngine::sync` derived the telemetry flush URL from the user-
supplied `manifest_url` by stripping `/manifest.json` and appending
`/telemetry/mobile`. A malicious manifest URL could redirect telemetry batches
to an attacker-controlled or internal endpoint (SSRF / exfiltration). The
internal `reqwest` client also followed HTTP redirects by default.

**Fix:**
- Added an explicit `telemetry_url` field to `MobileEngineConfig`. The engine
  no longer derives telemetry endpoints from `manifest_url`.
- `MobileEngine::sync` only flushes telemetry when `telemetry_enabled` is true,
  `telemetry_url` is non-empty, and the URL passes SSRF validation.
- `MobileEngine::flush_telemetry` validates the caller-supplied endpoint before
  flushing.
- Reused/generalized the existing HTTPS URL validator
  (`validate_https_url`/`validate_sequences_url`) for both sequences and
  telemetry URLs. It enforces `https://`, a public host, and port 443.
- Disabled HTTP redirects in `TelemetryManager`'s `reqwest` client.

**Tests:**
- Unit tests in `orch8-mobile/src/lib.rs` for HTTP, loopback, and non-standard
  port rejection, and for valid HTTPS acceptance.
- Updated integration tests in `orch8-mobile/tests/mobile_coverage.rs` to use
  valid HTTPS URLs for empty-buffer flush scenarios.

## Verification

- `cargo fmt -- --check` passes.
- `cargo clippy --workspace --all-targets` is clean.
- `cargo test --workspace` passes.

## Remaining Design-Level Items (not addressed in this pass)

The following are documented as architectural/security design concerns that
were out of scope for this tactical fix round:

- `orch8` crate scheduler does not scope ticks by tenant.
- `orch8-storage` single-row lookups lack tenant predicates in some paths.
- `orch8-storage` plaintext secrets unless `EncryptingStorage` wrapper is used.
- `orch8-grpc` synchronous `block_in_place` auth interceptor.
- `orch8-engine` SSRF DNS-rebinding window and expression/template depth limits.
- `orch8-publisher` CDN endpoint HTTPS enforcement pending review.
