//! Built-in `agent` handler — a native, durable `ReAct` loop.
//!
//! Where `react-loop.json` expresses Reason → Act → Observe as a hand-authored
//! `Loop` + `llm_call` + `tool_call` sequence, this handler ships the loop as a
//! single first-class primitive: give it a goal, a tool set, and a tool
//! dispatch target, and it drives the conversation until the model stops
//! requesting tools (or an iteration budget is hit).
//!
//! It reuses the engine's existing primitives rather than reimplementing them:
//! the model call goes through [`super::llm::handle_llm_call`] (10 providers +
//! failover), and each tool invocation goes through [`super::tool_call`] (HTTP)
//! or [`super::mcp`] (Model Context Protocol). Because those are the same
//! handlers used everywhere else, an agent inherits provider failover,
//! SSRF guards, and MCP durability for free.
//!
//! ## Durability boundary
//!
//! The whole loop runs inside one step invocation, so a crash mid-loop
//! re-executes the agent step from the start (subject to the step's retry
//! policy) rather than resuming at the exact iteration. Full mid-loop
//! checkpointing is a block-level follow-on; the step boundary still gives
//! retry, DLQ, and circuit-breaker semantics.
//!
//! ## Params
//!
//! | Field | Type | Default | Description |
//! |-------|------|---------|-------------|
//! | `goal` | string | — | Shorthand for a single initial user message. |
//! | `messages` | array | `[]` | Initial conversation (overrides `goal` if both given). |
//! | `system` | string | — | System prompt. |
//! | `tools` | array | — | LLM tool/function schema forwarded to the model. |
//! | `max_iterations` | u64 | `6` | Hard cap on reason→act cycles (clamped to `MAX_ITERATIONS_CEILING`). |
//! | `tool_dispatch` | object | — | How to execute tool calls (see below). Required if `tools` is set. |
//! | `provider` / `providers` / `model` / `api_key` / `api_key_env` / `base_url` / `temperature` / `max_tokens` | — | Forwarded verbatim to `llm_call`. |
//!
//! `tool_dispatch` is `{ "type": "http", "url": ..., "headers": {...} }` (each
//! tool call becomes a `tool_call`) or `{ "type": "mcp", "url": ..., "headers": {...} }`
//! (each tool call becomes an `mcp_call`).
//!
//! ## Result
//!
//! `{ "final": <assistant text | null>, "iterations": N, "stop_reason":
//! "completed" | "max_iterations", "tool_calls_made": M, "messages": [...] }`

use serde_json::{json, Value};
use tracing::debug;

use orch8_types::error::StepError;

use super::StepContext;

/// Default reason→act cycle budget when the caller does not set one.
const DEFAULT_MAX_ITERATIONS: u64 = 6;
/// Absolute ceiling on iterations regardless of caller request — a runaway
/// agent must not spin forever inside a single step.
const MAX_ITERATIONS_CEILING: u64 = 50;

/// LLM-config keys forwarded verbatim from the agent params into each
/// `llm_call`. `messages`, `system`, and `tools` are handled separately.
const LLM_PASSTHROUGH_KEYS: &[&str] = &[
    "provider",
    "providers",
    "model",
    "api_key",
    "api_key_env",
    "base_url",
    "temperature",
    "max_tokens",
    "total_timeout_secs",
    "per_provider_timeout_secs",
];

/// A tool call the model requested, normalized from the `llm_call` output.
#[derive(Debug, Clone, PartialEq)]
struct ToolCall {
    id: String,
    name: String,
    arguments: Value,
}

pub async fn handle_agent(ctx: StepContext) -> Result<Value, StepError> {
    let max_iterations = ctx
        .params
        .get("max_iterations")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_MAX_ITERATIONS)
        .clamp(1, MAX_ITERATIONS_CEILING);

    // Closures wire the real handlers. They clone the step context and swap in
    // freshly-built params, so sub-calls inherit tenant/instance identity and
    // the storage handle.
    let llm_ctx = ctx.clone();
    let call_llm = move |llm_params: Value| {
        let mut sub = llm_ctx.clone();
        sub.params = llm_params;
        async move { super::llm::handle_llm_call(sub).await }
    };

    let tool_ctx = ctx.clone();
    let dispatch_cfg = ctx.params.get("tool_dispatch").cloned();
    let dispatch_tool = move |call: ToolCall| {
        let mut sub = tool_ctx.clone();
        let cfg = dispatch_cfg.clone();
        async move {
            let (kind, params) = build_tool_dispatch(cfg.as_ref(), &call)?;
            sub.params = params;
            match kind {
                ToolKind::Http => super::tool_call::handle_tool_call(sub).await,
                ToolKind::Mcp => super::mcp::handle_mcp_call(sub).await,
            }
        }
    };

    run_agent_loop(&ctx.params, max_iterations, call_llm, dispatch_tool).await
}

/// Which sub-handler executes a tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolKind {
    Http,
    Mcp,
}

/// The reason→act→observe driver, generic over how the model and tools are
/// invoked so it can be unit-tested with deterministic closures (no network).
async fn run_agent_loop<L, LFut, T, TFut>(
    agent_params: &Value,
    max_iterations: u64,
    call_llm: L,
    dispatch_tool: T,
) -> Result<Value, StepError>
where
    L: Fn(Value) -> LFut,
    LFut: std::future::Future<Output = Result<Value, StepError>>,
    T: Fn(ToolCall) -> TFut,
    TFut: std::future::Future<Output = Result<Value, StepError>>,
{
    let mut messages = initial_messages(agent_params);
    let mut tool_calls_made: u64 = 0;

    for iteration in 0..max_iterations {
        let llm_params = build_llm_params(agent_params, &messages);
        let output = call_llm(llm_params).await?;

        let assistant = assistant_message(&output);
        messages.push(assistant.clone());

        let calls = extract_tool_calls(&assistant);
        if calls.is_empty() {
            debug!(
                iteration,
                "agent: model produced no tool calls — completing"
            );
            return Ok(result(
                final_text(&assistant),
                iteration + 1,
                "completed",
                tool_calls_made,
                messages,
            ));
        }

        for call in calls {
            tool_calls_made += 1;
            let observation = match dispatch_tool(call.clone()).await {
                Ok(v) => v,
                // A tool error becomes an observation so the model can react
                // and self-correct rather than failing the whole agent.
                Err(e) => json!({ "error": step_error_message(&e) }),
            };
            messages.push(tool_result_message(&call.id, &observation));
        }
    }

    debug!(max_iterations, "agent: iteration budget exhausted");
    Ok(result(
        None,
        max_iterations,
        "max_iterations",
        tool_calls_made,
        messages,
    ))
}

/// Seed the conversation from `messages` (preferred) or `goal` (shorthand).
fn initial_messages(agent_params: &Value) -> Vec<Value> {
    if let Some(arr) = agent_params.get("messages").and_then(Value::as_array) {
        if !arr.is_empty() {
            return arr.clone();
        }
    }
    if let Some(goal) = agent_params.get("goal").and_then(Value::as_str) {
        return vec![json!({ "role": "user", "content": goal })];
    }
    Vec::new()
}

/// Build the params for one `llm_call`: forward the LLM config, then set the
/// running conversation, system prompt, and tool schema.
fn build_llm_params(agent_params: &Value, messages: &[Value]) -> Value {
    let mut params = serde_json::Map::new();
    for key in LLM_PASSTHROUGH_KEYS {
        if let Some(v) = agent_params.get(*key) {
            params.insert((*key).to_string(), v.clone());
        }
    }
    params.insert("messages".into(), json!(messages));
    if let Some(system) = agent_params.get("system") {
        params.insert("system".into(), system.clone());
    }
    if let Some(tools) = agent_params.get("tools") {
        params.insert("tools".into(), tools.clone());
    }
    Value::Object(params)
}

/// Pull the assistant `message` object out of the normalized `llm_call`
/// output. Falls back to an empty assistant message if absent.
fn assistant_message(llm_output: &Value) -> Value {
    llm_output
        .get("message")
        .cloned()
        .unwrap_or_else(|| json!({ "role": "assistant", "content": "" }))
}

/// Normalize `message.tool_calls` (`OpenAI` shape, also produced for Anthropic)
/// into [`ToolCall`]s. `function.arguments` is a JSON string; a parse failure
/// yields `Value::Null` arguments so the tool can report the problem rather
/// than the loop crashing.
fn extract_tool_calls(assistant: &Value) -> Vec<ToolCall> {
    let Some(arr) = assistant.get("tool_calls").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|tc| {
            let function = tc.get("function")?;
            let name = function.get("name").and_then(Value::as_str)?.to_string();
            let id = tc
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let arguments = match function.get("arguments") {
                Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
                Some(other) => other.clone(),
                None => json!({}),
            };
            Some(ToolCall {
                id,
                name,
                arguments,
            })
        })
        .collect()
}

/// Build the `(handler, params)` for executing one tool call, from the
/// `tool_dispatch` config.
fn build_tool_dispatch(
    cfg: Option<&Value>,
    call: &ToolCall,
) -> Result<(ToolKind, Value), StepError> {
    let cfg = cfg.ok_or_else(|| StepError::Permanent {
        message: format!(
            "agent: model requested tool {:?} but no `tool_dispatch` is configured",
            call.name
        ),
        details: None,
    })?;

    let kind = cfg.get("type").and_then(Value::as_str).unwrap_or("http");
    let url = cfg
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| StepError::Permanent {
            message: "agent: tool_dispatch is missing `url`".into(),
            details: None,
        })?;
    let headers = cfg.get("headers").cloned();

    match kind {
        "http" => {
            let mut params = json!({
                "url": url,
                "tool_name": call.name,
                "arguments": call.arguments,
            });
            if let Some(h) = headers {
                params["headers"] = h;
            }
            Ok((ToolKind::Http, params))
        }
        "mcp" => {
            let mut params = json!({
                "url": url,
                "action": "call",
                "tool_name": call.name,
                "arguments": call.arguments,
            });
            if let Some(h) = headers {
                params["headers"] = h;
            }
            Ok((ToolKind::Mcp, params))
        }
        other => Err(StepError::Permanent {
            message: format!("agent: unknown tool_dispatch type {other:?} (expected http or mcp)"),
            details: None,
        }),
    }
}

/// Build the `role:"tool"` observation message for the conversation.
fn tool_result_message(tool_call_id: &str, observation: &Value) -> Value {
    json!({
        "role": "tool",
        "tool_call_id": tool_call_id,
        "content": serde_json::to_string(observation).unwrap_or_else(|_| "null".into()),
    })
}

/// Extract the final assistant text, if any (`null` when the turn was purely
/// tool calls with no text content).
fn final_text(assistant: &Value) -> Option<String> {
    assistant
        .get("content")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Assemble the handler's output value.
fn result(
    final_text: Option<String>,
    iterations: u64,
    stop_reason: &str,
    tool_calls_made: u64,
    messages: Vec<Value>,
) -> Value {
    // Build via an owned map so `final_text` and `messages` are moved in
    // (the `json!` macro would only borrow them).
    let mut m = serde_json::Map::new();
    m.insert(
        "final".into(),
        final_text.map_or(Value::Null, Value::String),
    );
    m.insert("iterations".into(), json!(iterations));
    m.insert("stop_reason".into(), json!(stop_reason));
    m.insert("tool_calls_made".into(), json!(tool_calls_made));
    m.insert("messages".into(), Value::Array(messages));
    Value::Object(m)
}

fn step_error_message(e: &StepError) -> String {
    match e {
        StepError::Permanent { message, .. } | StepError::Retryable { message, .. } => {
            message.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn assistant_text(text: &str) -> Value {
        json!({ "message": { "role": "assistant", "content": text } })
    }

    fn assistant_tool_call(id: &str, name: &str, args_json: &str) -> Value {
        json!({
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [
                    { "id": id, "type": "function", "function": { "name": name, "arguments": args_json } }
                ]
            }
        })
    }

    #[test]
    fn initial_messages_from_goal() {
        let msgs = initial_messages(&json!({ "goal": "find the weather" }));
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"], "find the weather");
    }

    #[test]
    fn initial_messages_prefers_explicit_messages() {
        let msgs = initial_messages(&json!({
            "goal": "ignored",
            "messages": [{ "role": "user", "content": "real" }]
        }));
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["content"], "real");
    }

    #[test]
    fn initial_messages_empty_when_neither_present() {
        assert!(initial_messages(&json!({})).is_empty());
    }

    #[test]
    fn build_llm_params_forwards_config_and_sets_conversation() {
        let agent = json!({
            "provider": "openai",
            "model": "gpt-4o",
            "api_key": "k",
            "temperature": 0.2,
            "system": "you are helpful",
            "tools": [{ "type": "function", "function": { "name": "t" } }],
            "goal": "hi"
        });
        let messages = vec![json!({ "role": "user", "content": "hi" })];
        let params = build_llm_params(&agent, &messages);
        assert_eq!(params["provider"], "openai");
        assert_eq!(params["model"], "gpt-4o");
        assert_eq!(params["api_key"], "k");
        assert_eq!(params["temperature"], 0.2);
        assert_eq!(params["system"], "you are helpful");
        assert_eq!(params["messages"][0]["content"], "hi");
        assert_eq!(params["tools"][0]["function"]["name"], "t");
        // `goal` is not an LLM param and must not leak through.
        assert!(params.get("goal").is_none());
    }

    #[test]
    fn extract_tool_calls_parses_arguments_json_string() {
        let assistant = assistant_tool_call("tc1", "search", r#"{"q":"rust"}"#)["message"].clone();
        let calls = extract_tool_calls(&assistant);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "tc1");
        assert_eq!(calls[0].name, "search");
        assert_eq!(calls[0].arguments, json!({ "q": "rust" }));
    }

    #[test]
    fn extract_tool_calls_tolerates_bad_arguments_json() {
        let assistant = assistant_tool_call("tc1", "search", "not json")["message"].clone();
        let calls = extract_tool_calls(&assistant);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].arguments, Value::Null);
    }

    #[test]
    fn extract_tool_calls_empty_when_absent() {
        let assistant = assistant_text("done")["message"].clone();
        assert!(extract_tool_calls(&assistant).is_empty());
    }

    #[test]
    fn build_tool_dispatch_http() {
        let call = ToolCall {
            id: "1".into(),
            name: "search".into(),
            arguments: json!({ "q": "x" }),
        };
        let (kind, params) = build_tool_dispatch(
            Some(&json!({ "type": "http", "url": "https://tools.example/run" })),
            &call,
        )
        .unwrap();
        assert_eq!(kind, ToolKind::Http);
        assert_eq!(params["url"], "https://tools.example/run");
        assert_eq!(params["tool_name"], "search");
        assert_eq!(params["arguments"]["q"], "x");
    }

    #[test]
    fn build_tool_dispatch_mcp_with_headers() {
        let call = ToolCall {
            id: "1".into(),
            name: "echo".into(),
            arguments: json!({}),
        };
        let (kind, params) = build_tool_dispatch(
            Some(&json!({
                "type": "mcp",
                "url": "https://mcp.example/rpc",
                "headers": { "Authorization": "Bearer x" }
            })),
            &call,
        )
        .unwrap();
        assert_eq!(kind, ToolKind::Mcp);
        assert_eq!(params["action"], "call");
        assert_eq!(params["headers"]["Authorization"], "Bearer x");
    }

    #[test]
    fn build_tool_dispatch_defaults_to_http() {
        let call = ToolCall {
            id: "1".into(),
            name: "t".into(),
            arguments: json!({}),
        };
        let (kind, _) =
            build_tool_dispatch(Some(&json!({ "url": "https://e.example" })), &call).unwrap();
        assert_eq!(kind, ToolKind::Http);
    }

    #[test]
    fn build_tool_dispatch_missing_config_is_permanent() {
        let call = ToolCall {
            id: "1".into(),
            name: "search".into(),
            arguments: json!({}),
        };
        let err = build_tool_dispatch(None, &call).unwrap_err();
        assert!(matches!(err, StepError::Permanent { .. }));
    }

    #[test]
    fn build_tool_dispatch_missing_url_is_permanent() {
        let call = ToolCall {
            id: "1".into(),
            name: "search".into(),
            arguments: json!({}),
        };
        let err = build_tool_dispatch(Some(&json!({ "type": "http" })), &call).unwrap_err();
        assert!(matches!(err, StepError::Permanent { .. }));
    }

    #[test]
    fn build_tool_dispatch_unknown_type_is_permanent() {
        let call = ToolCall {
            id: "1".into(),
            name: "search".into(),
            arguments: json!({}),
        };
        let err =
            build_tool_dispatch(Some(&json!({ "type": "smtp", "url": "x" })), &call).unwrap_err();
        assert!(matches!(err, StepError::Permanent { .. }));
    }

    #[test]
    fn tool_result_message_shape() {
        let m = tool_result_message("tc1", &json!({ "result": 42 }));
        assert_eq!(m["role"], "tool");
        assert_eq!(m["tool_call_id"], "tc1");
        // content is a JSON string of the observation.
        let parsed: Value = serde_json::from_str(m["content"].as_str().unwrap()).unwrap();
        assert_eq!(parsed["result"], 42);
    }

    #[test]
    fn final_text_none_when_empty_or_null() {
        assert_eq!(final_text(&json!({ "content": "hi" })), Some("hi".into()));
        assert_eq!(final_text(&json!({ "content": "" })), None);
        assert_eq!(final_text(&json!({ "content": null })), None);
        assert_eq!(final_text(&json!({})), None);
    }

    // ---- loop integration tests with deterministic closures ----------------

    #[tokio::test]
    async fn loop_completes_immediately_when_no_tool_calls() {
        let out = run_agent_loop(
            &json!({ "goal": "say hi" }),
            6,
            |_params| async { Ok(assistant_text("hello!")) },
            |_call| async { Ok(json!({})) },
        )
        .await
        .unwrap();
        assert_eq!(out["stop_reason"], "completed");
        assert_eq!(out["iterations"], 1);
        assert_eq!(out["final"], "hello!");
        assert_eq!(out["tool_calls_made"], 0);
    }

    #[tokio::test]
    async fn loop_runs_tool_then_completes() {
        // Iteration 0: model asks for a tool. Iteration 1: model answers.
        let step = RefCell::new(0u32);
        let out = run_agent_loop(
            &json!({
                "goal": "weather?",
                "tool_dispatch": { "type": "http", "url": "https://x.example" }
            }),
            6,
            |_params| {
                let n = {
                    let mut s = step.borrow_mut();
                    let cur = *s;
                    *s += 1;
                    cur
                };
                async move {
                    if n == 0 {
                        Ok(assistant_tool_call(
                            "tc1",
                            "get_weather",
                            r#"{"city":"SF"}"#,
                        ))
                    } else {
                        Ok(assistant_text("It is sunny."))
                    }
                }
            },
            |call| async move {
                assert_eq!(call.name, "get_weather");
                assert_eq!(call.arguments["city"], "SF");
                Ok(json!({ "tool_name": "get_weather", "result": "sunny" }))
            },
        )
        .await
        .unwrap();

        assert_eq!(out["stop_reason"], "completed");
        assert_eq!(out["iterations"], 2);
        assert_eq!(out["final"], "It is sunny.");
        assert_eq!(out["tool_calls_made"], 1);
        // Conversation: user, assistant(tool_call), tool result, assistant(final)
        assert_eq!(out["messages"].as_array().unwrap().len(), 4);
        assert_eq!(out["messages"][2]["role"], "tool");
    }

    #[tokio::test]
    async fn loop_hits_max_iterations() {
        // Model asks for a tool forever.
        let out = run_agent_loop(
            &json!({
                "goal": "loop",
                "tool_dispatch": { "type": "http", "url": "https://x.example" }
            }),
            3,
            |_params| async { Ok(assistant_tool_call("tc", "spin", "{}")) },
            |_call| async { Ok(json!({ "ok": true })) },
        )
        .await
        .unwrap();
        assert_eq!(out["stop_reason"], "max_iterations");
        assert_eq!(out["iterations"], 3);
        assert_eq!(out["tool_calls_made"], 3);
        assert_eq!(out["final"], Value::Null);
    }

    #[tokio::test]
    async fn loop_surfaces_tool_error_as_observation_and_continues() {
        let step = RefCell::new(0u32);
        let out = run_agent_loop(
            &json!({
                "goal": "x",
                "tool_dispatch": { "type": "http", "url": "https://x.example" }
            }),
            6,
            |_params| {
                let n = {
                    let mut s = step.borrow_mut();
                    let cur = *s;
                    *s += 1;
                    cur
                };
                async move {
                    if n == 0 {
                        Ok(assistant_tool_call("tc1", "flaky", "{}"))
                    } else {
                        Ok(assistant_text("recovered"))
                    }
                }
            },
            |_call| async {
                Err(StepError::Permanent {
                    message: "tool exploded".into(),
                    details: None,
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(out["stop_reason"], "completed");
        assert_eq!(out["final"], "recovered");
        // The tool observation carries the error text so the model can react.
        let tool_msg = &out["messages"][2];
        assert_eq!(tool_msg["role"], "tool");
        assert!(tool_msg["content"]
            .as_str()
            .unwrap()
            .contains("tool exploded"));
    }

    #[tokio::test]
    async fn loop_propagates_llm_error() {
        let result = run_agent_loop(
            &json!({ "goal": "x" }),
            6,
            |_params| async {
                Err(StepError::Retryable {
                    message: "llm down".into(),
                    details: None,
                })
            },
            |_call| async { Ok(json!({})) },
        )
        .await;
        assert!(matches!(result, Err(StepError::Retryable { .. })));
    }

    #[tokio::test]
    async fn loop_multiple_tool_calls_in_one_turn() {
        let step = RefCell::new(0u32);
        let out = run_agent_loop(
            &json!({
                "goal": "x",
                "tool_dispatch": { "type": "http", "url": "https://x.example" }
            }),
            6,
            |_params| {
                let n = {
                    let mut s = step.borrow_mut();
                    let cur = *s;
                    *s += 1;
                    cur
                };
                async move {
                    if n == 0 {
                        Ok(json!({ "message": {
                            "role": "assistant",
                            "content": null,
                            "tool_calls": [
                                { "id": "a", "type": "function", "function": { "name": "t1", "arguments": "{}" } },
                                { "id": "b", "type": "function", "function": { "name": "t2", "arguments": "{}" } }
                            ]
                        }}))
                    } else {
                        Ok(assistant_text("both done"))
                    }
                }
            },
            |_call| async { Ok(json!({ "ok": true })) },
        )
        .await
        .unwrap();
        assert_eq!(out["tool_calls_made"], 2);
        assert_eq!(out["final"], "both done");
    }
}
