# Sequences: Create, Publish, and Generate

This guide is written for two readers:

- Humans who need the fastest path from an idea to a deployed Orch8 sequence.
- LLMs that need enough exact structure to generate valid sequence JSON.

Public product docs describe Orch8 sequences as JSON workflows with steps, delays, conditions, retries, rate limits, crash recovery, signals, A/B split routing, and human review. The canonical wire format is the Rust API contract in `orch8-types/src/sequence.rs` and `orch8-api/src/sequences.rs`. When examples differ, prefer the Rust contract.

Sources cross-checked:

- Public docs: https://www.orch8.io/
- REST API: `orch8-api/src/sequences.rs`
- Sequence schema and validation: `orch8-types/src/sequence.rs`
- Duration JSON format: `orch8-types/src/lib.rs`
- E2E examples: `tests/e2e/client.ts`, `loadgen/src/catalog/*.ts`

## Fast Start

A sequence is a versioned workflow definition. Publishing a sequence means sending a complete `SequenceDefinition` JSON document to:

```http
POST /sequences
Content-Type: application/json
X-Tenant-Id: demo
```

Minimal valid sequence:

```json
{
  "id": "018f85d4-9c2b-7f5f-a2a5-f4143fa7d001",
  "tenant_id": "demo",
  "namespace": "default",
  "name": "hello-world",
  "version": 1,
  "deprecated": false,
  "blocks": [
    {
      "type": "step",
      "id": "greet",
      "handler": "noop",
      "params": {}
    }
  ],
  "created_at": "2026-05-05T12:00:00Z"
}
```

Publish it:

```bash
curl -s -X POST "$ORCH8_URL/sequences" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: demo" \
  -d @sequence.json
```

Run it:

```bash
curl -s -X POST "$ORCH8_URL/instances" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: demo" \
  -d '{
    "sequence_id": "018f85d4-9c2b-7f5f-a2a5-f4143fa7d001",
    "tenant_id": "demo",
    "namespace": "default",
    "context": { "data": { "user_id": "user_123" } },
    "idempotency_key": "hello-world:user_123:v1"
  }'
```

## Generator Contract

An LLM generating a sequence should emit a complete JSON object with these top-level fields:

| Field | Required | Notes |
|---|---:|---|
| `id` | yes | UUID string. Generate a new ID for each published version. |
| `tenant_id` | yes | Must match the tenant scope. If `X-Tenant-Id` is set, the API enforces or overwrites this to the scoped tenant. |
| `namespace` | yes | Usually `default`. Use namespaces to separate app domains. |
| `name` | yes | Stable logical name, e.g. `customer-onboarding`. |
| `version` | yes | Integer. Increment when publishing a changed definition. |
| `deprecated` | no | Boolean, default `false`. Deprecated versions are hidden from latest by-name lookup. |
| `blocks` | yes | Non-empty array of block definitions. Block IDs must be unique across the whole sequence, including nested blocks. |
| `interceptors` | no | Optional lifecycle interceptor definition. Omit unless intentionally using interceptors. |
| `created_at` | yes | RFC 3339 timestamp string. |

Rules that prevent most invalid generations:

- Use snake_case block types exactly: `step`, `parallel`, `race`, `loop`, `for_each`, `router`, `try_catch`, `sub_sequence`, `ab_split`, `cancellation_scope`.
- Do not use compact marketing router shapes like `if`, `then`, `else`, or `while`; the API expects the Rust fields documented below. `branches` is valid only for `parallel` and `race`.
- Give every block a unique `id`. Duplicate IDs return HTTP 400.
- Durations are numbers. Integers are milliseconds. Non-integer numbers are accepted as seconds for backwards compatibility, but generators should prefer integer milliseconds.
- Unknown `handler` names are accepted with warnings so external workers can handle them. Built-ins include `noop`, `log`, `sleep`, `fail`, `http_request`, `llm_call`, `tool_call`, `human_review`, `self_modify`, `emit_event`, `send_signal`, `query_instance`, `set_state`, `get_state`, `delete_state`, `transform`, `assert`, and `merge_state`.
- Put business data under instance `context.data`, tenant/app configuration under `context.config`, and keep sequence `params` for static block configuration.

## Block Reference

### Step

Runs one handler, either built-in or external worker.

```json
{
  "type": "step",
  "id": "send_welcome",
  "handler": "send_email",
  "params": {
    "template": "welcome"
  },
  "retry": {
    "max_attempts": 3,
    "initial_backoff": 1000,
    "max_backoff": 60000,
    "backoff_multiplier": 2.0
  },
  "timeout": 30000
}
```

Useful step fields:

| Field | Notes |
|---|---|
| `handler` | Required non-empty string. Built-in handler or external worker name. |
| `params` | JSON passed to the handler. Defaults to `{}`. |
| `delay` | Defers before the step runs. See scheduling below. |
| `retry` | `max_attempts > 0`, `initial_backoff <= max_backoff`, `backoff_multiplier > 0`. |
| `timeout` | Optional duration in milliseconds. |
| `rate_limit_key` | Defers over-limit steps instead of dropping them. |
| `send_window` | Restricts execution to local hours/days. |
| `context_access` | Limits what context sections a handler can read. |
| `cancellable` | Defaults to `true`; set `false` for cleanup/finalization work. |
| `wait_for_input` | Pauses for human input via `human_input:{block_id}` signal. |
| `queue_name` | Routes external work to a named worker queue. |
| `deadline` | SLA wall-clock duration from step start. |
| `on_deadline_breach` | Handler and params to invoke on SLA breach. |
| `fallback_handler` | Handler used when the primary handler's circuit breaker is open. |
| `cache_key` | Template-resolved key for cached step outputs. |

### Parallel

Runs branches concurrently and waits for all branches.

```json
{
  "type": "parallel",
  "id": "preflight",
  "branches": [
    [
      { "type": "step", "id": "validate", "handler": "order_validate", "params": {} }
    ],
    [
      { "type": "step", "id": "charge", "handler": "order_charge", "params": {} }
    ]
  ]
}
```

`branches` must contain at least one branch. Each branch is an array of blocks.

### Race

Runs branches concurrently. Default semantics are `first_to_resolve`; optional `first_to_succeed` waits for a successful branch.

```json
{
  "type": "race",
  "id": "fastest_provider",
  "semantics": "first_to_succeed",
  "branches": [
    [
      { "type": "step", "id": "try_primary", "handler": "primary_api", "params": {} }
    ],
    [
      { "type": "step", "id": "try_backup", "handler": "backup_api", "params": {} }
    ]
  ]
}
```

### Router

Evaluates routes in order and executes the first condition that matches. If none match, `default` runs when present.

```json
{
  "type": "router",
  "id": "route_engagement",
  "routes": [
    {
      "condition": "context.data.opened == true",
      "blocks": [
        { "type": "step", "id": "send_followup", "handler": "send_email", "params": { "template": "followup" } }
      ]
    }
  ],
  "default": [
    { "type": "step", "id": "send_reminder", "handler": "send_email", "params": { "template": "reminder" } }
  ]
}
```

The router must have at least one route or a default. Route conditions must be non-empty.

### TryCatch

Runs `try_block`; on failure runs `catch_block`; always runs `finally_block` when present.

```json
{
  "type": "try_catch",
  "id": "payment_failover",
  "try_block": [
    { "type": "step", "id": "charge_stripe", "handler": "stripe_charge", "params": {} }
  ],
  "catch_block": [
    { "type": "step", "id": "charge_braintree", "handler": "braintree_charge", "params": {} }
  ],
  "finally_block": [
    { "type": "step", "id": "record_payment_attempt", "handler": "log", "params": { "event": "payment_attempted" } }
  ]
}
```

`try_block` must be non-empty. `catch_block` may be empty.

### Loop

Repeats `body` while `condition` is true, up to `max_iterations`.

```json
{
  "type": "loop",
  "id": "agent_loop",
  "condition": "context.data.done != true",
  "max_iterations": 10,
  "continue_on_error": false,
  "body": [
    { "type": "step", "id": "think", "handler": "llm_call", "params": { "provider": "openai", "model": "gpt-4o-mini" } },
    { "type": "step", "id": "act", "handler": "tool_call", "params": {} }
  ]
}
```

`condition` and `body` are required. `max_iterations` defaults to `1000` and must be greater than zero.

### ForEach

Iterates over a collection expression.

```json
{
  "type": "for_each",
  "id": "process_items",
  "collection": "context.data.items",
  "item_var": "item",
  "max_iterations": 500,
  "body": [
    { "type": "step", "id": "process_item", "handler": "process_item", "params": { "item": "{{item}}" } }
  ]
}
```

`collection`, `item_var`, and `body` must be non-empty.

### SubSequence

Starts a child instance by sequence name and waits for it to finish.

```json
{
  "type": "sub_sequence",
  "id": "run_reusable_onboarding_stage",
  "sequence_name": "onboarding-stage",
  "version": 2,
  "input": {
    "user_id": "{{context.data.user_id}}"
  }
}
```

If `version` is omitted, Orch8 resolves the latest non-deprecated version by tenant, namespace, and name.

### ABSplit

Deterministically routes each instance to one weighted variant.

```json
{
  "type": "ab_split",
  "id": "subject_test",
  "variants": [
    {
      "name": "control",
      "weight": 70,
      "blocks": [
        { "type": "step", "id": "send_control", "handler": "send_email", "params": { "subject": "Welcome" } }
      ]
    },
    {
      "name": "variant_a",
      "weight": 30,
      "blocks": [
        { "type": "step", "id": "send_variant_a", "handler": "send_email", "params": { "subject": "You're invited" } }
      ]
    }
  ]
}
```

Requires at least two variants, unique non-empty variant names, and total weight greater than zero.

### CancellationScope

Protects critical child blocks from external cancel signals until they finish.

```json
{
  "type": "cancellation_scope",
  "id": "payment_commit",
  "blocks": [
    { "type": "step", "id": "capture_payment", "handler": "capture_payment", "params": {} },
    { "type": "step", "id": "write_ledger", "handler": "write_ledger", "params": {} }
  ]
}
```

`blocks` must be non-empty.

## Scheduling Fields

Use step `delay` for durable waits:

```json
{
  "type": "step",
  "id": "wait_three_days",
  "handler": "noop",
  "params": {},
  "delay": {
    "duration": 259200000,
    "business_days_only": true,
    "jitter": 1800000,
    "holidays": ["2026-05-25"],
    "timezone": "America/New_York"
  }
}
```

`delay` fields:

| Field | Notes |
|---|---|
| `duration` | Required duration. Prefer integer milliseconds. |
| `business_days_only` | Skips weekends and configured holidays when true. |
| `jitter` | Optional duration. Scheduler adds random spread around the base delay. |
| `holidays` | Array of `YYYY-MM-DD`; merged with `context.config.holidays`. |
| `fire_at_local` | Local wall-clock timestamp like `2026-03-08T02:30:00`; converted to UTC using `timezone` or instance timezone. |
| `timezone` | IANA timezone for `fire_at_local`. |

Use step `send_window` to restrict execution windows:

```json
{
  "type": "step",
  "id": "send_business_hours",
  "handler": "send_email",
  "params": {},
  "send_window": {
    "start_hour": 9,
    "end_hour": 17,
    "days": [0, 1, 2, 3, 4]
  }
}
```

Days are `0=Monday` through `6=Sunday`. `start_hour` and `end_hour` are `0..23` and must differ.

## Human Review

For a human approval step, use the `human_review` handler plus `wait_for_input`.

```json
{
  "type": "step",
  "id": "approval",
  "handler": "human_review",
  "params": {
    "instructions": "Approve or reject this request",
    "reviewer": "ops"
  },
  "wait_for_input": {
    "prompt": "Approve deployment?",
    "timeout": 3600000,
    "choices": [
      { "label": "Approve", "value": "approved" },
      { "label": "Reject", "value": "rejected" }
    ],
    "store_as": "approval_result"
  }
}
```

Validation rules:

- `choices`, when present, must be non-empty.
- Choice `value` strings must be unique.
- `store_as`, when present, must be non-empty.
- If `choices` is omitted, the engine uses a yes/no default.

The engine waits for a signal named `human_input:{block_id}`. A simpler integration can also update context with `update_context` and route on the updated value.

## Publishing Flow

Recommended flow for generated sequences:

1. Generate a complete sequence JSON document.
2. Ensure every block ID is unique.
3. Publish with `POST /sequences`.
4. Store the returned `id`; it is the immutable sequence version ID.
5. Create test instances with deterministic `idempotency_key` values.
6. Observe instance state and block outputs.
7. Promote by creating a new version with the same `tenant_id`, `namespace`, and `name`, a higher `version`, and a new `id`.
8. Deprecate old versions only when new starts should stop using them.

Useful endpoints:

| Task | Endpoint |
|---|---|
| Create/publish sequence | `POST /sequences` |
| List sequences | `GET /sequences?tenant_id=demo&namespace=default` |
| Fetch by immutable ID | `GET /sequences/{id}` |
| Fetch latest non-deprecated by name | `GET /sequences/by-name?tenant_id=demo&namespace=default&name=hello-world` |
| Fetch exact version by name | `GET /sequences/by-name?tenant_id=demo&namespace=default&name=hello-world&version=2` |
| List all versions | `GET /sequences/versions?tenant_id=demo&namespace=default&name=hello-world` |
| Deprecate a version | `POST /sequences/{id}/deprecate` |
| Delete inactive sequence | `DELETE /sequences/{id}` |
| Migrate active instance | `POST /sequences/migrate-instance` |

Deprecation:

```bash
curl -s -X POST "$ORCH8_URL/sequences/$SEQUENCE_ID/deprecate" \
  -H "X-Tenant-Id: demo"
```

Migration:

```bash
curl -s -X POST "$ORCH8_URL/sequences/migrate-instance" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: demo" \
  -d '{
    "instance_id": "018f85d4-9c2b-7f5f-a2a5-f4143fa7d010",
    "target_sequence_id": "018f85d4-9c2b-7f5f-a2a5-f4143fa7d020"
  }'
```

Migration is tenant-isolated and only valid for non-terminal instances. Completed, failed, and cancelled instances should be treated as immutable history.

Deletion is intentionally conservative. A sequence cannot be deleted while active instances still reference it in `scheduled`, `running`, `paused`, or `waiting` states.

## Complete Example: Onboarding Drip

```json
{
  "id": "018f85d4-9c2b-7f5f-a2a5-f4143fa7d100",
  "tenant_id": "acme",
  "namespace": "lifecycle",
  "name": "customer-onboarding",
  "version": 1,
  "deprecated": false,
  "blocks": [
    {
      "type": "step",
      "id": "send_welcome",
      "handler": "send_email",
      "params": {
        "template": "welcome",
        "to": "{{context.data.email}}"
      },
      "retry": {
        "max_attempts": 3,
        "initial_backoff": 1000,
        "max_backoff": 60000,
        "backoff_multiplier": 2.0
      },
      "timeout": 30000,
      "rate_limit_key": "email:transactional",
      "send_window": {
        "start_hour": 9,
        "end_hour": 17,
        "days": [0, 1, 2, 3, 4]
      }
    },
    {
      "type": "step",
      "id": "wait_two_days",
      "handler": "noop",
      "params": {},
      "delay": {
        "duration": 172800000,
        "business_days_only": true,
        "jitter": 1800000
      }
    },
    {
      "type": "router",
      "id": "activation_branch",
      "routes": [
        {
          "condition": "context.data.activated == true",
          "blocks": [
            {
              "type": "step",
              "id": "send_success_tips",
              "handler": "send_email",
              "params": {
                "template": "success_tips",
                "to": "{{context.data.email}}"
              }
            }
          ]
        }
      ],
      "default": [
        {
          "type": "step",
          "id": "send_activation_nudge",
          "handler": "send_email",
          "params": {
            "template": "activation_nudge",
            "to": "{{context.data.email}}"
          }
        },
        {
          "type": "step",
          "id": "notify_csm",
          "handler": "ap://slack.send_channel_message",
          "params": {
            "auth": { "access_token": "credentials://slack-bot" },
            "props": {
              "channel": "#customer-success",
              "text": "User {{context.data.user_id}} has not activated."
            }
          }
        }
      ]
    }
  ],
  "created_at": "2026-05-05T12:00:00Z"
}
```

Create an instance:

```json
{
  "sequence_id": "018f85d4-9c2b-7f5f-a2a5-f4143fa7d100",
  "tenant_id": "acme",
  "namespace": "lifecycle",
  "timezone": "America/New_York",
  "metadata": {
    "campaign": "onboarding"
  },
  "context": {
    "data": {
      "user_id": "user_123",
      "email": "alice@example.com",
      "activated": false
    },
    "config": {
      "holidays": ["2026-05-25"]
    }
  },
  "concurrency_key": "user:user_123:onboarding",
  "max_concurrency": 1,
  "idempotency_key": "customer-onboarding:user_123:v1"
}
```

## LLM Prompt Template

Use this prompt when asking an LLM to generate a sequence:

```text
Generate an Orch8 SequenceDefinition JSON object.

Hard requirements:
- Emit only JSON.
- Use snake_case block types from this list: step, parallel, race, loop, for_each, router, try_catch, sub_sequence, ab_split, cancellation_scope.
- Include top-level id, tenant_id, namespace, name, version, deprecated, blocks, created_at.
- Every block must have a globally unique id.
- Durations must be integer milliseconds.
- Router uses routes [{ condition, blocks }] and optional default.
- TryCatch uses try_block, catch_block, and optional finally_block.
- Parallel and Race use branches as arrays of block arrays.
- Do not use if/then/else, while, block, or marketing shorthand.
- Put dynamic user fields under context.data references such as {{context.data.email}}.
- Use external handler names only when a worker will exist; otherwise use built-ins.

Workflow intent:
<describe business workflow here>

Tenant:
<tenant_id>

Namespace:
<namespace>
```

## Pre-Publish Checklist

Before sending `POST /sequences`, verify:

- JSON parses cleanly.
- Top-level `blocks` is not empty.
- No two blocks share an `id`, even across nested branches.
- Every step has a non-empty `handler`.
- Every retry policy has positive `max_attempts`, positive `backoff_multiplier`, and `initial_backoff <= max_backoff`.
- Every router has at least one `routes` entry or a `default`.
- Every loop and foreach has a non-empty body and non-zero `max_iterations`.
- Every `ab_split` has at least two variants and total weight greater than zero.
- Every `sub_sequence.sequence_name` refers to a published sequence in the same tenant and namespace.
- Unknown handlers are intentional external workers or plugin/Activepieces handlers.
- Version changes use a new sequence `id`; do not mutate an already-published definition in place.
