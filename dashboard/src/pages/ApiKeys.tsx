import { useCallback, useState } from "react";
import { usePolling } from "../hooks/usePolling";
import { usePageTitle } from "../hooks/usePageTitle";
import {
  listApiKeys,
  createApiKey,
  revokeApiKey,
  type ApiKeyDef,
} from "../api";
import { PageHeader } from "../components/ui/PageHeader";
import { PageMeta } from "../components/ui/PageMeta";
import { Section } from "../components/ui/Section";
import { Table, THead, TH, TR, TD, Empty } from "../components/ui/Table";
import { Button } from "../components/ui/Button";
import { Input, FieldLabel } from "../components/ui/Input";
import { Badge } from "../components/ui/Badge";
import { Relative } from "../components/ui/Relative";
import { IconPlus, IconTrash, IconCopy, IconCheck } from "../components/ui/Icons";
import { SkeletonTable } from "../components/ui/Skeleton";

export default function ApiKeys() {
  usePageTitle("API Keys");
  const fetcher = useCallback((signal?: AbortSignal) => listApiKeys(signal), []);
  const { data, loading, updatedAt, refresh } = usePolling<ApiKeyDef[]>(fetcher, 5000);
  const [showForm, setShowForm] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [revealedKey, setRevealedKey] = useState<string | null>(null);

  const flash = (msg: string) => {
    setToast(msg);
    setTimeout(() => setToast(null), 2500);
  };

  return (
    <div className="space-y-12">
      <PageHeader
        eyebrow="Operator"
        title="API Keys"
        description="Per-tenant API keys for authenticating with the engine. Keys are shown once on creation and cannot be retrieved again."
        actions={
          <div className="flex items-center gap-2">
            <Button variant="primary" size="sm" onClick={() => setShowForm((v) => !v)}>
              <IconPlus size={13} /> {showForm ? "Close" : "New API key"}
            </Button>
            <PageMeta updatedAt={updatedAt} onRefresh={refresh} />
          </div>
        }
      />

      {toast && <div className="notice notice-ok">{toast}</div>}

      {revealedKey && (
        <RevealedKey
          apiKey={revealedKey}
          onDismiss={() => setRevealedKey(null)}
        />
      )}

      {showForm && (
        <CreateApiKeyForm
          onCreated={(fullKey) => {
            flash("API key created");
            setShowForm(false);
            setRevealedKey(fullKey);
            refresh();
          }}
          onError={(msg) => flash(msg)}
        />
      )}

      {loading && !data && <SkeletonTable rows={6} cols={6} />}

      {data && (
        <Section
          eyebrow="Registry"
          title="Issued keys"
          description="All API keys the engine recognises. Revoked keys stop working immediately."
          meta={
            <>
              <span>
                <span className="text-faint">TOTAL</span>{" "}
                <span className="text-ink-dim">{data.length}</span>
              </span>
            </>
          }
        >
          <Table>
            <THead>
              <TH>Name</TH>
              <TH>Prefix</TH>
              <TH>Tenant</TH>
              <TH>Last used</TH>
              <TH>Created</TH>
              <TH className="text-right">Actions</TH>
            </THead>
            <tbody>
              {data.map((k) => (
                <TR key={k.id}>
                  <TD className="font-mono text-[12px] text-ink">{k.name}</TD>
                  <TD>
                    <Badge tone="dim">{k.prefix}...</Badge>
                  </TD>
                  <TD className="font-mono text-[12px] text-muted">
                    {k.tenant_id || "global"}
                  </TD>
                  <TD>
                    {k.last_used_at ? (
                      <Relative at={k.last_used_at} />
                    ) : (
                      <span className="text-faint">never</span>
                    )}
                  </TD>
                  <TD>
                    <Relative at={k.created_at} />
                  </TD>
                  <TD className="text-right">
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => {
                        if (!confirm(`Revoke API key "${k.name}"? This cannot be undone.`)) return;
                        revokeApiKey(k.id)
                          .then(() => {
                            flash("Revoked");
                            refresh();
                          })
                          .catch((e) => flash(String(e)));
                      }}
                      title="Revoke API key"
                    >
                      <IconTrash size={13} />
                    </Button>
                  </TD>
                </TR>
              ))}
              {data.length === 0 && (
                <Empty colSpan={99}>
                  No API keys issued. Create one to authenticate API requests.
                </Empty>
              )}
            </tbody>
          </Table>
        </Section>
      )}
    </div>
  );
}

function RevealedKey({
  apiKey,
  onDismiss,
}: {
  apiKey: string;
  onDismiss: () => void;
}) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    await navigator.clipboard.writeText(apiKey);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <Section eyebrow="New key" title="Copy your API key now">
      <div>
        <p className="annotation mb-3">
          This is the only time the full key will be shown. Store it securely.
        </p>
        <div className="flex items-center gap-2">
          <code className="flex-1 bg-surface border border-hairline px-3 py-2 text-[12px] font-mono text-ink break-all rounded-sm">
            {apiKey}
          </code>
          <Button variant="primary" size="sm" onClick={copy} title="Copy to clipboard">
            {copied ? <IconCheck size={13} /> : <IconCopy size={13} />}
            {copied ? "Copied" : "Copy"}
          </Button>
        </div>
        <div className="mt-4 flex justify-end">
          <Button variant="ghost" size="sm" onClick={onDismiss}>
            Dismiss
          </Button>
        </div>
      </div>
    </Section>
  );
}

function CreateApiKeyForm({
  onCreated,
  onError,
}: {
  onCreated: (fullKey: string) => void;
  onError: (msg: string) => void;
}) {
  const [name, setName] = useState("");
  const [tenantId, setTenantId] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    if (!name) {
      onError("Name is required");
      return;
    }
    setBusy(true);
    try {
      const result = await createApiKey({ name, tenant_id: tenantId || undefined });
      onCreated(result.key);
    } catch (e) {
      onError(`Failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Section eyebrow="New API key" title="Issue a key">
      <div>
        <div className="grid grid-cols-2 gap-6">
          <div>
            <FieldLabel>Name</FieldLabel>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="production-backend"
              className="w-full font-mono"
            />
            <p className="annotation mt-1">A human-readable label for this key.</p>
          </div>
          <div>
            <FieldLabel>Tenant ID</FieldLabel>
            <Input
              value={tenantId}
              onChange={(e) => setTenantId(e.target.value)}
              placeholder="global if empty"
              className="w-full"
            />
          </div>
        </div>
        <div className="mt-6 flex justify-end">
          <Button variant="primary" size="sm" disabled={busy} onClick={submit}>
            Create API key
          </Button>
        </div>
      </div>
    </Section>
  );
}
