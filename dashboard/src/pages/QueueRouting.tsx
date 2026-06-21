import { useCallback, useState } from "react";
import {
  listRoutingRules,
  createRoutingRule,
  deleteRoutingRule,
  listDispatchConfigs,
  setDispatchConfig,
  deleteDispatchConfig,
  type RoutingRule,
  type DispatchConfig,
} from "../api";
import { usePolling } from "../hooks/usePolling";
import { usePageTitle } from "../hooks/usePageTitle";
import { PageHeader } from "../components/ui/PageHeader";
import { PageMeta } from "../components/ui/PageMeta";
import { Section } from "../components/ui/Section";
import { Table, THead, TH, TR, TD, Empty } from "../components/ui/Table";
import { Button } from "../components/ui/Button";
import { Input, Select, FieldLabel } from "../components/ui/Input";
import { Badge } from "../components/ui/Badge";
import { Relative } from "../components/ui/Relative";
import { IconPlus, IconTrash } from "../components/ui/Icons";
import { SkeletonTable } from "../components/ui/Skeleton";

export default function QueueRouting() {
  usePageTitle("Queues");

  const rulesFetcher = useCallback(
    (signal?: AbortSignal) => listRoutingRules(undefined, signal),
    [],
  );
  const dispatchFetcher = useCallback(
    (signal?: AbortSignal) => listDispatchConfigs(undefined, signal),
    [],
  );

  const {
    data: rules,
    loading: rulesLoading,
    updatedAt: rulesUpdatedAt,
    refresh: refreshRules,
  } = usePolling<RoutingRule[]>(rulesFetcher, 5000);

  const {
    data: configs,
    loading: configsLoading,
    updatedAt: configsUpdatedAt,
    refresh: refreshConfigs,
  } = usePolling<DispatchConfig[]>(dispatchFetcher, 5000);

  const [showRuleForm, setShowRuleForm] = useState(false);
  const [showConfigForm, setShowConfigForm] = useState(false);
  const [toast, setToast] = useState<string | null>(null);

  const flash = (msg: string) => {
    setToast(msg);
    setTimeout(() => setToast(null), 2500);
  };

  const refreshAll = () => {
    refreshRules();
    refreshConfigs();
  };

  const latestUpdate = rulesUpdatedAt && configsUpdatedAt
    ? (new Date(rulesUpdatedAt) > new Date(configsUpdatedAt) ? rulesUpdatedAt : configsUpdatedAt)
    : rulesUpdatedAt ?? configsUpdatedAt ?? null;

  return (
    <div className="space-y-12">
      <PageHeader
        eyebrow="Operator"
        title="Queue Routing"
        description="Route handler steps to named queues and configure how each queue dispatches work to consumers. Routing rules decide which queue a handler lands in; dispatch configs decide how that queue delivers tasks."
        actions={
          <div className="flex items-center gap-2">
            <PageMeta updatedAt={latestUpdate} onRefresh={refreshAll} />
          </div>
        }
      />

      {toast && <div className="notice notice-ok">{toast}</div>}

      {/* ── Section 1: Routing Rules ─────────────────────────── */}

      {showRuleForm && (
        <CreateRoutingRuleForm
          onCreated={() => {
            flash("Routing rule created");
            setShowRuleForm(false);
            refreshRules();
          }}
          onError={(msg) => flash(msg)}
        />
      )}

      {rulesLoading && !rules && <SkeletonTable rows={4} cols={5} />}

      {rules && (
        <Section
          eyebrow="Routing"
          title="Routing rules"
          description="Each rule maps a handler to a named queue. When the engine needs to dispatch a step for a handler, it looks up the matching rule to find the target queue."
          meta={
            <div className="flex items-center gap-4">
              <span>
                <span className="text-faint">TOTAL</span>{" "}
                <span className="text-ink-dim">{rules.length}</span>
              </span>
              <Button
                variant="primary"
                size="sm"
                onClick={() => setShowRuleForm((v) => !v)}
              >
                <IconPlus size={13} /> {showRuleForm ? "Close" : "New rule"}
              </Button>
            </div>
          }
        >
          <Table>
            <THead>
              <TH>Handler</TH>
              <TH>Queue</TH>
              <TH>Tenant</TH>
              <TH>Priority</TH>
              <TH>Created</TH>
              <TH className="text-right">Actions</TH>
            </THead>
            <tbody>
              {rules.map((r) => (
                <TR key={`${r.tenant_id}-${r.handler_name}-${r.queue_name}`}>
                  <TD className="font-mono text-[12px] text-ink">
                    {r.handler_name}
                  </TD>
                  <TD className="font-mono text-[12px]">{r.queue_name}</TD>
                  <TD className="font-mono text-[12px] text-muted">
                    {r.tenant_id}
                  </TD>
                  <TD className="font-mono text-[12px] text-muted tabular">
                    {r.priority}
                  </TD>
                  <TD>
                    <Relative at={r.created_at} />
                  </TD>
                  <TD className="text-right">
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => {
                        if (
                          !confirm(
                            `Delete routing rule for handler "${r.handler_name}" -> queue "${r.queue_name}"?`,
                          )
                        )
                          return;
                        deleteRoutingRule(r.id)
                          .then(() => {
                            flash("Deleted");
                            refreshRules();
                          })
                          .catch((e) => flash(String(e)));
                      }}
                      title="Delete routing rule"
                    >
                      <IconTrash size={13} />
                    </Button>
                  </TD>
                </TR>
              ))}
              {rules.length === 0 && (
                <Empty colSpan={99}>
                  No routing rules yet. Create one to route handlers to queues.
                </Empty>
              )}
            </tbody>
          </Table>
        </Section>
      )}

      {/* ── Section 2: Dispatch Configuration ────────────────── */}

      {showConfigForm && (
        <CreateDispatchConfigForm
          onCreated={() => {
            flash("Dispatch config created");
            setShowConfigForm(false);
            refreshConfigs();
          }}
          onError={(msg) => flash(msg)}
        />
      )}

      {configsLoading && !configs && <SkeletonTable rows={4} cols={5} />}

      {configs && (
        <Section
          eyebrow="Dispatch"
          title="Dispatch configuration"
          description="Controls how each queue delivers tasks to consumers. Poll mode lets workers pull tasks; push mode sends tasks to a target URL."
          meta={
            <div className="flex items-center gap-4">
              <span>
                <span className="text-faint">TOTAL</span>{" "}
                <span className="text-ink-dim">{configs.length}</span>
              </span>
              <Button
                variant="primary"
                size="sm"
                onClick={() => setShowConfigForm((v) => !v)}
              >
                <IconPlus size={13} /> {showConfigForm ? "Close" : "New config"}
              </Button>
            </div>
          }
        >
          <Table>
            <THead>
              <TH>Queue</TH>
              <TH>Mode</TH>
              <TH>Target URL</TH>
              <TH>Tenant</TH>
              <TH>Created</TH>
              <TH className="text-right">Actions</TH>
            </THead>
            <tbody>
              {configs.map((c) => (
                <TR key={`${c.tenant_id}-${c.queue_name}`}>
                  <TD className="font-mono text-[12px] text-ink">
                    {c.queue_name}
                  </TD>
                  <TD>
                    <Badge tone={c.mode === "push" ? "ok" : "dim"}>
                      {c.mode}
                    </Badge>
                  </TD>
                  <TD className="font-mono text-[12px] text-muted">
                    {c.target_url || "—"}
                  </TD>
                  <TD className="font-mono text-[12px] text-muted">
                    {c.tenant_id}
                  </TD>
                  <TD>
                    <Relative at={c.created_at} />
                  </TD>
                  <TD className="text-right">
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => {
                        if (
                          !confirm(
                            `Delete dispatch config for queue "${c.queue_name}"?`,
                          )
                        )
                          return;
                        deleteDispatchConfig(c.tenant_id, c.queue_name)
                          .then(() => {
                            flash("Deleted");
                            refreshConfigs();
                          })
                          .catch((e) => flash(String(e)));
                      }}
                      title="Delete dispatch config"
                    >
                      <IconTrash size={13} />
                    </Button>
                  </TD>
                </TR>
              ))}
              {configs.length === 0 && (
                <Empty colSpan={99}>
                  No dispatch configs yet. Create one to configure how a queue
                  delivers tasks.
                </Empty>
              )}
            </tbody>
          </Table>
        </Section>
      )}
    </div>
  );
}

/* ── Create Routing Rule Form ──────────────────────────────── */

function CreateRoutingRuleForm({
  onCreated,
  onError,
}: {
  onCreated: () => void;
  onError: (msg: string) => void;
}) {
  const [tenantId, setTenantId] = useState("tenant-a");
  const [handlerName, setHandlerName] = useState("");
  const [queueName, setQueueName] = useState("");
  const [priority, setPriority] = useState(0);
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    if (!tenantId || !handlerName || !queueName) {
      onError("tenant_id, handler_name, and queue_name are required");
      return;
    }
    setBusy(true);
    try {
      await createRoutingRule({
        tenant_id: tenantId,
        handler_name: handlerName,
        queue_name: queueName,
        priority,
      });
      onCreated();
    } catch (e) {
      onError(`Failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Section
      eyebrow="New routing rule"
      title="Route a handler to a queue"
      description="Map a handler name to a named queue. Tasks for this handler will be routed to the specified queue with the given priority."
    >
      <div>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-6">
          <div>
            <FieldLabel>Tenant</FieldLabel>
            <Input
              value={tenantId}
              onChange={(e) => setTenantId(e.target.value)}
              className="w-full"
            />
          </div>
          <div>
            <FieldLabel>Handler name</FieldLabel>
            <Input
              value={handlerName}
              onChange={(e) => setHandlerName(e.target.value)}
              placeholder="send-email"
              className="w-full"
            />
          </div>
          <div>
            <FieldLabel>Queue name</FieldLabel>
            <Input
              value={queueName}
              onChange={(e) => setQueueName(e.target.value)}
              placeholder="email-queue"
              className="w-full"
            />
          </div>
          <div>
            <FieldLabel>Priority</FieldLabel>
            <Input
              type="number"
              value={priority}
              onChange={(e) => setPriority(Number(e.target.value))}
              className="w-full"
            />
            <p className="annotation mt-1">Higher = processed first.</p>
          </div>
        </div>
        <div className="mt-6 flex justify-end">
          <Button variant="primary" size="sm" disabled={busy} onClick={submit}>
            Create rule
          </Button>
        </div>
      </div>
    </Section>
  );
}

/* ── Create Dispatch Config Form ───────────────────────────── */

function CreateDispatchConfigForm({
  onCreated,
  onError,
}: {
  onCreated: () => void;
  onError: (msg: string) => void;
}) {
  const [tenantId, setTenantId] = useState("tenant-a");
  const [queueName, setQueueName] = useState("");
  const [mode, setMode] = useState<"poll" | "push">("poll");
  const [targetUrl, setTargetUrl] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    if (!tenantId || !queueName) {
      onError("tenant_id and queue_name are required");
      return;
    }
    if (mode === "push" && !targetUrl) {
      onError("target_url is required for push mode");
      return;
    }
    setBusy(true);
    try {
      await setDispatchConfig({
        tenant_id: tenantId,
        queue_name: queueName,
        mode,
        ...(mode === "push" ? { target_url: targetUrl } : {}),
      });
      onCreated();
    } catch (e) {
      onError(`Failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Section
      eyebrow="New dispatch config"
      title="Configure queue dispatch"
      description="Set how a queue delivers tasks. Poll mode lets workers pull; push mode sends tasks to a URL."
    >
      <div>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-6">
          <div>
            <FieldLabel>Tenant</FieldLabel>
            <Input
              value={tenantId}
              onChange={(e) => setTenantId(e.target.value)}
              className="w-full"
            />
          </div>
          <div>
            <FieldLabel>Queue name</FieldLabel>
            <Input
              value={queueName}
              onChange={(e) => setQueueName(e.target.value)}
              placeholder="email-queue"
              className="w-full"
            />
          </div>
          <div>
            <FieldLabel>Mode</FieldLabel>
            <Select
              value={mode}
              onChange={(e) => setMode(e.target.value as "poll" | "push")}
              className="w-full"
            >
              <option value="poll">poll</option>
              <option value="push">push</option>
            </Select>
          </div>
          {mode === "push" && (
            <div>
              <FieldLabel>Target URL</FieldLabel>
              <Input
                value={targetUrl}
                onChange={(e) => setTargetUrl(e.target.value)}
                placeholder="https://example.com/webhook"
                className="w-full"
              />
              <p className="annotation mt-1">
                Tasks will be POSTed to this URL.
              </p>
            </div>
          )}
        </div>
        <div className="mt-6 flex justify-end">
          <Button variant="primary" size="sm" disabled={busy} onClick={submit}>
            Create config
          </Button>
        </div>
      </div>
    </Section>
  );
}
