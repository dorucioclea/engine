import { useCallback, useState } from "react";
import { usePolling } from "../hooks/usePolling";
import { usePageTitle } from "../hooks/usePageTitle";
import {
  listRollbackPolicies,
  createRollbackPolicy,
  deleteRollbackPolicy,
  type RollbackPolicy,
} from "../api";
import { PageHeader } from "../components/ui/PageHeader";
import { PageMeta } from "../components/ui/PageMeta";
import { Section } from "../components/ui/Section";
import { Glossary, type GlossaryItem } from "../components/ui/Glossary";
import { Table, THead, TH, TR, TD, Empty } from "../components/ui/Table";
import { Button } from "../components/ui/Button";
import { Input, FieldLabel } from "../components/ui/Input";
import { Relative } from "../components/ui/Relative";
import { IconPlus, IconTrash } from "../components/ui/Icons";
import { SkeletonTable } from "../components/ui/Skeleton";

const PAGE_GLOSSARY: GlossaryItem[] = [
  {
    term: "Rollback Policy",
    definition:
      "A named configuration that governs how the engine rolls back failed or cancelled instances. Policies are referenced by name from sequences.",
  },
  {
    term: "Config",
    definition:
      "JSON object defining rollback behavior — strategy, retry limits, compensation steps, etc.",
  },
];

export default function RollbackPolicies() {
  usePageTitle("Rollback Policies");
  const fetcher = useCallback((signal?: AbortSignal) => listRollbackPolicies(signal), []);
  const { data, loading, updatedAt, refresh } = usePolling<RollbackPolicy[]>(fetcher, 5000);
  const [showForm, setShowForm] = useState(false);
  const [toast, setToast] = useState<string | null>(null);

  const flash = (msg: string) => {
    setToast(msg);
    setTimeout(() => setToast(null), 2500);
  };

  return (
    <div className="space-y-12">
      <PageHeader
        eyebrow="Operator"
        title="Rollback Policies"
        description="Named configurations that govern how the engine rolls back failed or cancelled instances."
        actions={
          <div className="flex items-center gap-2">
            <Button variant="primary" size="sm" onClick={() => setShowForm((v) => !v)}>
              <IconPlus size={13} /> {showForm ? "Close" : "New policy"}
            </Button>
            <PageMeta updatedAt={updatedAt} onRefresh={refresh} />
          </div>
        }
      />

      <Glossary items={PAGE_GLOSSARY} />

      {toast && <div className="notice notice-ok">{toast}</div>}

      {showForm && (
        <CreateRollbackPolicyForm
          onCreated={() => {
            flash("Policy created");
            setShowForm(false);
            refresh();
          }}
          onError={(msg) => flash(msg)}
        />
      )}

      {loading && !data && <SkeletonTable rows={6} cols={5} />}

      {data && (
        <Section
          eyebrow="Registry"
          title="Stored policies"
          description="Every rollback policy the engine can apply to sequences."
          meta={
            <span>
              <span className="text-faint">TOTAL</span>{" "}
              <span className="text-ink-dim">{data.length}</span>
            </span>
          }
        >
          <Table>
            <THead>
              <TH>Name</TH>
              <TH>Tenant</TH>
              <TH>Config</TH>
              <TH>Created</TH>
              <TH className="text-right">Actions</TH>
            </THead>
            <tbody>
              {data.map((p) => (
                <TR key={p.name}>
                  <TD className="font-mono text-[12px] text-ink">{p.name}</TD>
                  <TD className="font-mono text-[12px] text-muted">
                    {p.tenant_id || "global"}
                  </TD>
                  <TD className="font-mono text-[11px] text-muted max-w-[320px] truncate">
                    {JSON.stringify(p.config)}
                  </TD>
                  <TD>
                    <Relative at={p.created_at} />
                  </TD>
                  <TD className="text-right">
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => {
                        if (!confirm(`Delete policy "${p.name}"?`)) return;
                        deleteRollbackPolicy(p.name)
                          .then(() => {
                            flash("Deleted");
                            refresh();
                          })
                          .catch((e) => flash(String(e)));
                      }}
                      title="Delete policy"
                    >
                      <IconTrash size={13} />
                    </Button>
                  </TD>
                </TR>
              ))}
              {data.length === 0 && (
                <Empty colSpan={99}>
                  No rollback policies yet. Create one to define rollback behavior for sequences.
                </Empty>
              )}
            </tbody>
          </Table>
        </Section>
      )}
    </div>
  );
}

function CreateRollbackPolicyForm({
  onCreated,
  onError,
}: {
  onCreated: () => void;
  onError: (msg: string) => void;
}) {
  const [name, setName] = useState("");
  const [tenantId, setTenantId] = useState("");
  const [config, setConfig] = useState("{}");
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    if (!name) {
      onError("name is required");
      return;
    }
    let parsed: Record<string, unknown>;
    try {
      parsed = JSON.parse(config);
    } catch {
      onError("config must be valid JSON");
      return;
    }
    setBusy(true);
    try {
      await createRollbackPolicy({
        name,
        tenant_id: tenantId || undefined,
        config: parsed,
      });
      onCreated();
    } catch (e) {
      onError(`Failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Section eyebrow="New policy" title="Create a rollback policy">
      <div>
        <div className="grid grid-cols-2 md:grid-cols-3 gap-6">
          <div>
            <FieldLabel>Name</FieldLabel>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="default-rollback"
              className="w-full font-mono"
            />
          </div>
          <div>
            <FieldLabel>Tenant (optional)</FieldLabel>
            <Input
              value={tenantId}
              onChange={(e) => setTenantId(e.target.value)}
              placeholder="global if empty"
              className="w-full"
            />
          </div>
          <div className="col-span-2 md:col-span-3">
            <FieldLabel>Config (JSON)</FieldLabel>
            <textarea
              value={config}
              onChange={(e) => setConfig(e.target.value)}
              placeholder='{"strategy":"compensate","max_retries":3}'
              className="w-full h-24 rounded border border-rule bg-bg px-3 py-2 text-[13px] font-mono text-ink placeholder:text-faint focus:outline-none focus:ring-1 focus:ring-signal"
            />
          </div>
        </div>
        <div className="mt-6 flex justify-end">
          <Button variant="primary" size="sm" disabled={busy} onClick={submit}>
            Create policy
          </Button>
        </div>
      </div>
    </Section>
  );
}
