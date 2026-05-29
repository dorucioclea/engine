/**
 * agent handler — native durable ReAct loop.
 *
 * A single mock server plays two roles, routed by path:
 *   - POST /chat/completions  → OpenAI-compatible LLM. First call returns a
 *     tool call; second call returns a final answer.
 *   - POST /tool              → the HTTP tool endpoint the agent dispatches to.
 *
 * The harness sets ORCH8_ALLOW_INTERNAL_URLS=1, so the agent's nested
 * llm_call and tool_call may reach the loopback mock.
 *
 * Covered:
 *   - full reason → act → observe → answer cycle (2 LLM turns, 1 tool call).
 *   - agent with no tools completes in a single turn.
 *   - max_iterations stops a model that loops forever.
 */
import { describe, it, before, after, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { Orch8Client, testSequence, step, uuid } from "../client.ts";
import { startServer, stopServer } from "../harness.ts";
import type { ServerHandle } from "../harness.ts";

const client = new Orch8Client();

async function readBody(req: IncomingMessage): Promise<any> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) chunks.push(chunk as Buffer);
  const raw = Buffer.concat(chunks).toString("utf-8");
  if (!raw) return null;
  try {
    return JSON.parse(raw);
  } catch {
    return raw;
  }
}

interface MockOptions {
  /** "react" → tool call then answer; "answer" → answer immediately; "spin" → always tool call. */
  mode: "react" | "answer" | "spin";
}

interface MockHandle {
  baseUrl: string;
  chatCalls: number;
  toolCalls: number;
  close(): Promise<void>;
}

function toolCallTurn(): unknown {
  return {
    choices: [
      {
        index: 0,
        message: {
          role: "assistant",
          content: null,
          tool_calls: [
            {
              id: "tc1",
              type: "function",
              function: { name: "get_weather", arguments: JSON.stringify({ city: "SF" }) },
            },
          ],
        },
        finish_reason: "tool_calls",
      },
    ],
    usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 },
    model: "gpt-4o-mini",
  };
}

function answerTurn(text: string): unknown {
  return {
    choices: [{ index: 0, message: { role: "assistant", content: text }, finish_reason: "stop" }],
    usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 },
    model: "gpt-4o-mini",
  };
}

function startMock(opts: MockOptions): Promise<MockHandle> {
  return new Promise((resolvePromise) => {
    const state = { chatCalls: 0, toolCalls: 0 };

    const server = createServer((req, res) => {
      void (async () => {
        await readBody(req);
        const path = req.url ?? "";
        res.setHeader("Content-Type", "application/json");

        if (path.includes("/chat/completions")) {
          state.chatCalls += 1;
          if (opts.mode === "answer") {
            res.end(JSON.stringify(answerTurn("Hello, no tools needed.")));
          } else if (opts.mode === "spin") {
            res.end(JSON.stringify(toolCallTurn()));
          } else {
            // react: first call → tool call, subsequent → answer.
            res.end(
              JSON.stringify(
                state.chatCalls === 1 ? toolCallTurn() : answerTurn("It is 72F in SF."),
              ),
            );
          }
          return;
        }

        if (path.includes("/tool")) {
          state.toolCalls += 1;
          res.end(JSON.stringify({ temp: 72, unit: "F" }));
          return;
        }

        res.statusCode = 404;
        res.end("{}");
      })();
    });

    server.listen(0, "127.0.0.1", () => {
      const port = (server.address() as AddressInfo).port;
      resolvePromise({
        baseUrl: `http://127.0.0.1:${port}`,
        get chatCalls() {
          return state.chatCalls;
        },
        get toolCalls() {
          return state.toolCalls;
        },
        close: () => new Promise<void>((r) => server.close(() => r())),
      });
    });
  });
}

const WEATHER_TOOL = {
  type: "function",
  function: {
    name: "get_weather",
    description: "Get the weather for a city",
    parameters: {
      type: "object",
      properties: { city: { type: "string" } },
      required: ["city"],
    },
  },
};

describe("agent Handler — ReAct loop (mock LLM + tool)", () => {
  let server: ServerHandle | undefined;
  let mock: MockHandle | undefined;

  before(async () => {
    server = await startServer();
  });

  after(async () => {
    await stopServer(server);
  });

  beforeEach(async () => {
    if (mock) await mock.close();
    mock = undefined;
  });

  after(async () => {
    if (mock) await mock.close();
  });

  it("drives reason → act → observe → answer and reports the loop result", async () => {
    mock = await startMock({ mode: "react" });
    const tenantId = `agent-react-${uuid().slice(0, 8)}`;
    const seq = testSequence(
      "agent-react",
      [
        step("s1", "agent", {
          provider: "openai",
          base_url: mock.baseUrl,
          api_key: "test-key",
          model: "gpt-4o-mini",
          goal: "What is the weather in SF?",
          tools: [WEATHER_TOOL],
          tool_dispatch: { type: "http", url: `${mock.baseUrl}/tool` },
          max_iterations: 5,
        }),
      ],
      { tenantId },
    );
    await client.createSequence(seq);
    const { id } = await client.createInstance({
      sequence_id: seq.id,
      tenant_id: tenantId,
      namespace: "default",
    });
    await client.waitForState(id, "completed", { timeoutMs: 20_000 });

    assert.equal(mock.chatCalls, 2, "model should be called twice (reason, then answer)");
    assert.equal(mock.toolCalls, 1, "the tool should be dispatched once");

    const outputs = await client.getOutputs(id);
    const out = outputs.find((o) => o.block_id === "s1")!.output as {
      final: string;
      iterations: number;
      stop_reason: string;
      tool_calls_made: number;
    };
    assert.equal(out.stop_reason, "completed");
    assert.equal(out.iterations, 2);
    assert.equal(out.tool_calls_made, 1);
    assert.equal(out.final, "It is 72F in SF.");
  });

  it("completes in a single turn when no tools are needed", async () => {
    mock = await startMock({ mode: "answer" });
    const tenantId = `agent-answer-${uuid().slice(0, 8)}`;
    const seq = testSequence(
      "agent-answer",
      [
        step("s1", "agent", {
          provider: "openai",
          base_url: mock.baseUrl,
          api_key: "test-key",
          model: "gpt-4o-mini",
          goal: "Just say hi",
        }),
      ],
      { tenantId },
    );
    await client.createSequence(seq);
    const { id } = await client.createInstance({
      sequence_id: seq.id,
      tenant_id: tenantId,
      namespace: "default",
    });
    await client.waitForState(id, "completed", { timeoutMs: 20_000 });

    assert.equal(mock.chatCalls, 1);
    assert.equal(mock.toolCalls, 0);
    const outputs = await client.getOutputs(id);
    const out = outputs.find((o) => o.block_id === "s1")!.output as {
      final: string;
      iterations: number;
      stop_reason: string;
    };
    assert.equal(out.iterations, 1);
    assert.equal(out.stop_reason, "completed");
    assert.equal(out.final, "Hello, no tools needed.");
  });

  it("stops at max_iterations when the model never finishes", async () => {
    mock = await startMock({ mode: "spin" });
    const tenantId = `agent-spin-${uuid().slice(0, 8)}`;
    const seq = testSequence(
      "agent-spin",
      [
        step("s1", "agent", {
          provider: "openai",
          base_url: mock.baseUrl,
          api_key: "test-key",
          model: "gpt-4o-mini",
          goal: "spin forever",
          tools: [WEATHER_TOOL],
          tool_dispatch: { type: "http", url: `${mock.baseUrl}/tool` },
          max_iterations: 2,
        }),
      ],
      { tenantId },
    );
    await client.createSequence(seq);
    const { id } = await client.createInstance({
      sequence_id: seq.id,
      tenant_id: tenantId,
      namespace: "default",
    });
    await client.waitForState(id, "completed", { timeoutMs: 20_000 });

    const outputs = await client.getOutputs(id);
    const out = outputs.find((o) => o.block_id === "s1")!.output as {
      stop_reason: string;
      iterations: number;
      tool_calls_made: number;
    };
    assert.equal(out.stop_reason, "max_iterations");
    assert.equal(out.iterations, 2);
    assert.equal(out.tool_calls_made, 2);
  });
});
