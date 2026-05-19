#!/usr/bin/env bash
set -euo pipefail

# Test script that simulates two mobile devices syncing with the server.
# Exercises: device registration, status updates, approval flow, two-tenant isolation.
#
# Prerequisites: orch8-server running on localhost:8080 with ORCH8_MOBILE_SYNC_ENABLED=true

BASE="${ORCH8_API_URL:-http://localhost:8080}"
echo "Server: $BASE"

# Two tenants, two devices
DEVICE_A="device-alpha-$(date +%s)"
DEVICE_B="device-beta-$(date +%s)"
INST_A="$(uuidgen | tr '[:upper:]' '[:lower:]')"
INST_B="$(uuidgen | tr '[:upper:]' '[:lower:]')"

echo ""
echo "=== Step 1: Register devices ==="
curl -s -X POST "$BASE/mobile/devices/register" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: tenant-alpha" \
  -d "{\"device_id\":\"$DEVICE_A\",\"platform\":\"ios\",\"app_version\":\"0.4.0\"}" \
  -o /dev/null -w "  Device A: HTTP %{http_code}\n"

curl -s -X POST "$BASE/mobile/devices/register" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: tenant-beta" \
  -d "{\"device_id\":\"$DEVICE_B\",\"platform\":\"android\",\"app_version\":\"0.4.0\"}" \
  -o /dev/null -w "  Device B: HTTP %{http_code}\n"

echo ""
echo "=== Step 2: Sync status updates (workflow executing) ==="
# Device A: payment-verification workflow, Running state
SYNC_A=$(curl -s -X POST "$BASE/mobile/sync" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: tenant-alpha" \
  -H "x-api-key: test" \
  -H "x-device-id: $DEVICE_A" \
  -d "{
    \"device_id\": \"$DEVICE_A\",
    \"status_updates\": [
      {\"instance_id\":\"$INST_A\",\"sequence_name\":\"payment-verification\",\"state\":\"Running\",\"current_step\":\"fraud_check\",\"handler\":\"fraud_check\",\"timestamp\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"steps\":[{\"block_id\":\"verify_identity\",\"block_type\":\"step\",\"state\":\"completed\",\"handler\":\"verify_identity\",\"started_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"completed_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"},{\"block_id\":\"fraud_check\",\"block_type\":\"step\",\"state\":\"running\",\"handler\":\"fraud_check\",\"started_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"completed_at\":null},{\"block_id\":\"payment_approval\",\"block_type\":\"step\",\"state\":\"pending\",\"handler\":\"request_approval\",\"started_at\":null,\"completed_at\":null},{\"block_id\":\"process_payment\",\"block_type\":\"step\",\"state\":\"pending\",\"handler\":\"process_payment\",\"started_at\":null,\"completed_at\":null},{\"block_id\":\"send_receipt\",\"block_type\":\"step\",\"state\":\"pending\",\"handler\":\"send_receipt\",\"started_at\":null,\"completed_at\":null}]}
    ],
    \"approval_requests\": [],
    \"command_acks\": []
  }")
echo "  Sync A: $SYNC_A"

# Device B: onboarding-flow workflow, Running state
SYNC_B=$(curl -s -X POST "$BASE/mobile/sync" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: tenant-beta" \
  -H "x-api-key: test" \
  -H "x-device-id: $DEVICE_B" \
  -d "{
    \"device_id\": \"$DEVICE_B\",
    \"status_updates\": [
      {\"instance_id\":\"$INST_B\",\"sequence_name\":\"onboarding-flow\",\"state\":\"Running\",\"current_step\":\"validate_email\",\"handler\":\"validate_email\",\"timestamp\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"steps\":[{\"block_id\":\"collect_info\",\"block_type\":\"step\",\"state\":\"completed\",\"handler\":\"collect_info\",\"started_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"completed_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"},{\"block_id\":\"validate_email\",\"block_type\":\"step\",\"state\":\"running\",\"handler\":\"validate_email\",\"started_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"completed_at\":null},{\"block_id\":\"create_account\",\"block_type\":\"step\",\"state\":\"pending\",\"handler\":\"create_account\",\"started_at\":null,\"completed_at\":null}]}
    ],
    \"approval_requests\": [],
    \"command_acks\": []
  }")
echo "  Sync B: $SYNC_B"

echo ""
echo "=== Step 3: Verify status visible in dashboard API ==="
STATUS_ALL=$(curl -s "$BASE/mobile/status?limit=100")
echo "  All statuses: $STATUS_ALL"

echo ""
echo "=== Step 4: Verify tenant isolation ==="
STATUS_A=$(curl -s "$BASE/mobile/status?limit=100" -H "X-Tenant-Id: tenant-alpha")
STATUS_B=$(curl -s "$BASE/mobile/status?limit=100" -H "X-Tenant-Id: tenant-beta")
echo "  Tenant Alpha: $STATUS_A"
echo "  Tenant Beta: $STATUS_B"

echo ""
echo "=== Step 5: Sync with approval request (human-in-the-loop) ==="
# Device A reaches wait_for_input step
SYNC_A2=$(curl -s -X POST "$BASE/mobile/sync" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: tenant-alpha" \
  -H "x-api-key: test" \
  -H "x-device-id: $DEVICE_A" \
  -d "{
    \"device_id\": \"$DEVICE_A\",
    \"status_updates\": [
      {\"instance_id\":\"$INST_A\",\"sequence_name\":\"payment-verification\",\"state\":\"Waiting\",\"current_step\":\"payment_approval\",\"handler\":\"request_approval\",\"timestamp\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"steps\":[{\"block_id\":\"verify_identity\",\"block_type\":\"step\",\"state\":\"completed\",\"handler\":\"verify_identity\",\"started_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"completed_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"},{\"block_id\":\"fraud_check\",\"block_type\":\"step\",\"state\":\"completed\",\"handler\":\"fraud_check\",\"started_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"completed_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"},{\"block_id\":\"payment_approval\",\"block_type\":\"step\",\"state\":\"waiting\",\"handler\":\"request_approval\",\"started_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"completed_at\":null},{\"block_id\":\"process_payment\",\"block_type\":\"step\",\"state\":\"pending\",\"handler\":\"process_payment\",\"started_at\":null,\"completed_at\":null},{\"block_id\":\"send_receipt\",\"block_type\":\"step\",\"state\":\"pending\",\"handler\":\"send_receipt\",\"started_at\":null,\"completed_at\":null}]}
    ],
    \"approval_requests\": [
      {\"instance_id\":\"$INST_A\",\"block_id\":\"payment_approval\",\"sequence_name\":\"payment-verification\",\"prompt\":\"Payment requires authorization\",\"choices\":[{\"label\":\"Approve\",\"value\":\"approved\"},{\"label\":\"Reject\",\"value\":\"rejected\"}],\"store_as\":\"payment_decision\",\"timeout_seconds\":86400}
    ],
    \"command_acks\": []
  }")
echo "  Sync A (with approval): $SYNC_A2"

echo ""
echo "=== Step 6: Verify approval visible in dashboard ==="
APPROVALS=$(curl -s "$BASE/mobile/approvals?state=pending")
echo "  Pending approvals: $APPROVALS"

# Extract approval ID
APPROVAL_ID=$(echo "$APPROVALS" | python3 -c "import json,sys; items=json.load(sys.stdin)['items']; print(items[0]['id'] if items else 'NONE')" 2>/dev/null || echo "NONE")
echo "  Approval ID: $APPROVAL_ID"

if [ "$APPROVAL_ID" = "NONE" ]; then
  echo "  ERROR: No approval found!"
  exit 1
fi

echo ""
echo "=== Step 7: Admin resolves approval ==="
RESOLVE=$(curl -s -X POST "$BASE/mobile/approvals/$APPROVAL_ID/resolve" \
  -H "Content-Type: application/json" \
  -d '{"output":{"value":"approved"}}' \
  -o /dev/null -w "HTTP %{http_code}")
echo "  Resolve: $RESOLVE"

echo ""
echo "=== Step 8: Device syncs and receives command ==="
SYNC_A3=$(curl -s -X POST "$BASE/mobile/sync" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: tenant-alpha" \
  -H "x-api-key: test" \
  -H "x-device-id: $DEVICE_A" \
  -d "{
    \"device_id\": \"$DEVICE_A\",
    \"status_updates\": [],
    \"approval_requests\": [],
    \"command_acks\": []
  }")
echo "  Sync A (expect command): $SYNC_A3"

echo ""
echo "=== Step 9: Device sends completion status ==="
SYNC_A4=$(curl -s -X POST "$BASE/mobile/sync" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: tenant-alpha" \
  -H "x-api-key: test" \
  -H "x-device-id: $DEVICE_A" \
  -d "{
    \"device_id\": \"$DEVICE_A\",
    \"status_updates\": [
      {\"instance_id\":\"$INST_A\",\"sequence_name\":\"payment-verification\",\"state\":\"Completed\",\"current_step\":null,\"handler\":null,\"timestamp\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"steps\":[{\"block_id\":\"verify_identity\",\"block_type\":\"step\",\"state\":\"completed\",\"handler\":\"verify_identity\",\"started_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"completed_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"},{\"block_id\":\"fraud_check\",\"block_type\":\"step\",\"state\":\"completed\",\"handler\":\"fraud_check\",\"started_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"completed_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"},{\"block_id\":\"payment_approval\",\"block_type\":\"step\",\"state\":\"completed\",\"handler\":\"request_approval\",\"started_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"completed_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"},{\"block_id\":\"process_payment\",\"block_type\":\"step\",\"state\":\"completed\",\"handler\":\"process_payment\",\"started_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"completed_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"},{\"block_id\":\"send_receipt\",\"block_type\":\"step\",\"state\":\"completed\",\"handler\":\"send_receipt\",\"started_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"completed_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}]}
    ],
    \"approval_requests\": [],
    \"command_acks\": []
  }")
echo "  Sync A (completed): $SYNC_A4"

echo ""
echo "=== Step 10: Final dashboard state ==="
FINAL_STATUS=$(curl -s "$BASE/mobile/status?limit=100")
FINAL_APPROVALS=$(curl -s "$BASE/mobile/approvals?limit=100")
echo "  Statuses: $FINAL_STATUS"
echo "  Approvals: $FINAL_APPROVALS"

echo ""
echo "=== Step 11: List registered devices ==="
DEVICES=$(curl -s "$BASE/mobile/devices?limit=10")
echo "  Devices: $DEVICES"

echo ""
echo "=== Step 12: Send start_workflow command to device ==="
START_CMD=$(curl -s -X POST "$BASE/mobile/commands" \
  -H "Content-Type: application/json" \
  -d "{
    \"device_id\": \"$DEVICE_A\",
    \"command_type\": \"start_workflow\",
    \"payload\": {\"sequence_name\":\"payment-verification\",\"input\":{\"amount\":100,\"currency\":\"USD\",\"customer_id\":\"cust-42\"}}
  }" \
  -o /dev/null -w "HTTP %{http_code}")
echo "  Start command: $START_CMD"

echo ""
echo "=== Step 13: Device syncs and receives start_workflow command ==="
SYNC_CMD=$(curl -s -X POST "$BASE/mobile/sync" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: tenant-alpha" \
  -H "x-api-key: test" \
  -H "x-device-id: $DEVICE_A" \
  -d "{
    \"device_id\": \"$DEVICE_A\",
    \"status_updates\": [],
    \"approval_requests\": [],
    \"command_acks\": []
  }")
echo "  Sync (expect start_workflow): $SYNC_CMD"

echo ""
echo "=== Step 14: Verify steps data in final status response ==="
HAS_STEPS=$(echo "$FINAL_STATUS" | python3 -c "
import json, sys
data = json.load(sys.stdin)
for item in data.get('items', []):
    steps = item.get('steps')
    if steps:
        parsed = json.loads(steps) if isinstance(steps, str) else steps
        if isinstance(parsed, list) and len(parsed) > 0:
            print(f\"  {item['instance_id'][:8]}... has {len(parsed)} steps: {', '.join(s['block_id'] + '=' + s['state'] for s in parsed)}\")
" 2>/dev/null)
if [ -n "$HAS_STEPS" ]; then
  echo "$HAS_STEPS"
else
  echo "  WARNING: No steps data found in status response"
fi

echo ""
echo "=== Step 15: Send update_sequence command (restart policy) ==="
UPDATE_RESTART=$(curl -s -X POST "$BASE/mobile/commands" \
  -H "Content-Type: application/json" \
  -d "{
    \"device_id\": \"$DEVICE_A\",
    \"command_type\": \"update_sequence\",
    \"payload\": {\"instance_id\":\"$INST_A\",\"sequence_name\":\"payment-verification\",\"policy\":\"restart\",\"input\":{\"amount\":200}}
  }" \
  -o /dev/null -w "HTTP %{http_code}")
echo "  update_sequence(restart): $UPDATE_RESTART"

echo ""
echo "=== Step 16: Send update_sequence command (fail policy) ==="
UPDATE_FAIL=$(curl -s -X POST "$BASE/mobile/commands" \
  -H "Content-Type: application/json" \
  -d "{
    \"device_id\": \"$DEVICE_A\",
    \"command_type\": \"update_sequence\",
    \"payload\": {\"instance_id\":\"$INST_A\",\"policy\":\"fail\"}
  }" \
  -o /dev/null -w "HTTP %{http_code}")
echo "  update_sequence(fail): $UPDATE_FAIL"

echo ""
echo "=== Step 17: Send update_sequence command (cancel policy) ==="
UPDATE_CANCEL=$(curl -s -X POST "$BASE/mobile/commands" \
  -H "Content-Type: application/json" \
  -d "{
    \"device_id\": \"$DEVICE_A\",
    \"command_type\": \"update_sequence\",
    \"payload\": {\"instance_id\":\"$INST_A\",\"policy\":\"cancel\"}
  }" \
  -o /dev/null -w "HTTP %{http_code}")
echo "  update_sequence(cancel): $UPDATE_CANCEL"

echo ""
echo "=== Step 18: Send update_sequence command (graceful policy) ==="
UPDATE_GRACEFUL=$(curl -s -X POST "$BASE/mobile/commands" \
  -H "Content-Type: application/json" \
  -d "{
    \"device_id\": \"$DEVICE_A\",
    \"command_type\": \"update_sequence\",
    \"payload\": {\"instance_id\":\"$INST_A\",\"policy\":\"graceful\"}
  }" \
  -o /dev/null -w "HTTP %{http_code}")
echo "  update_sequence(graceful): $UPDATE_GRACEFUL"

echo ""
echo "=== Step 19: Send update_sequence command (skip_executed policy) ==="
UPDATE_SKIP=$(curl -s -X POST "$BASE/mobile/commands" \
  -H "Content-Type: application/json" \
  -d "{
    \"device_id\": \"$DEVICE_A\",
    \"command_type\": \"update_sequence\",
    \"payload\": {\"instance_id\":\"$INST_A\",\"sequence_name\":\"payment-verification\",\"policy\":\"skip_executed\",\"input\":{\"amount\":300}}
  }" \
  -o /dev/null -w "HTTP %{http_code}")
echo "  update_sequence(skip_executed): $UPDATE_SKIP"

echo ""
echo "=== Step 20: Device syncs and receives update_sequence commands ==="
SYNC_UPDATE=$(curl -s -X POST "$BASE/mobile/sync" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: tenant-alpha" \
  -H "x-api-key: test" \
  -H "x-device-id: $DEVICE_A" \
  -d "{
    \"device_id\": \"$DEVICE_A\",
    \"status_updates\": [],
    \"approval_requests\": [],
    \"command_acks\": []
  }")
echo "  Sync (expect update_sequence commands): $SYNC_UPDATE"

echo ""
echo "=== Step 21: Create a test credential ==="
CRED_CREATE=$(curl -s -X POST "$BASE/credentials" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "test-api-key",
    "name": "Test API Key",
    "kind": "api_key",
    "value": "{\"token\":\"sk_test_secret_12345\"}"
  }' \
  -o /dev/null -w "HTTP %{http_code}")
echo "  Credential create: $CRED_CREATE"

echo ""
echo "=== Step 22: List credentials (verify it exists, secret redacted) ==="
CREDS=$(curl -s "$BASE/credentials")
echo "  Credentials: $CREDS"

echo ""
echo "=== Step 23: Step delegation — device sends step with credentials:// ref ==="
SYNC_DELEG=$(curl -s -X POST "$BASE/mobile/sync" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: tenant-alpha" \
  -H "x-api-key: test" \
  -H "x-device-id: $DEVICE_A" \
  -d "{
    \"device_id\": \"$DEVICE_A\",
    \"status_updates\": [],
    \"approval_requests\": [],
    \"step_delegations\": [
      {\"request_id\":\"deleg-001\",\"instance_id\":\"$INST_A\",\"block_id\":\"llm_call\",\"handler\":\"call_llm\",\"params\":{\"model\":\"gpt-4\",\"auth\":\"credentials://test-api-key\"}}
    ],
    \"command_acks\": []
  }")
echo "  Sync (delegation sent): $SYNC_DELEG"

echo ""
echo "=== Step 24: Device syncs to receive step_result with resolved credentials ==="
SYNC_RESULT=$(curl -s -X POST "$BASE/mobile/sync" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: tenant-alpha" \
  -H "x-api-key: test" \
  -H "x-device-id: $DEVICE_A" \
  -d "{
    \"device_id\": \"$DEVICE_A\",
    \"status_updates\": [],
    \"approval_requests\": [],
    \"step_delegations\": [],
    \"command_acks\": []
  }")
echo "  Sync (expect step_result): $SYNC_RESULT"

# Verify the step_result contains the resolved secret
HAS_RESOLVED=$(echo "$SYNC_RESULT" | python3 -c "
import json, sys
data = json.load(sys.stdin)
for cmd in data.get('commands', []):
    if cmd.get('type') == 'step_result':
        payload = cmd.get('payload', {})
        if payload.get('success'):
            params = payload.get('resolved_params', {})
            auth = params.get('auth', {})
            if isinstance(auth, dict) and 'token' in auth:
                print(f\"  PASS: Credential resolved — token starts with: {auth['token'][:10]}...\")
            else:
                print(f\"  FAIL: Credential not resolved, got: {auth}\")
        else:
            print(f\"  FAIL: step_result not successful: {payload.get('error')}\")
" 2>/dev/null)
if [ -n "$HAS_RESOLVED" ]; then
  echo "$HAS_RESOLVED"
else
  echo "  WARNING: No step_result command found"
fi

echo ""
echo "=== Step 25: Step delegation with non-existent credential ==="
SYNC_BAD=$(curl -s -X POST "$BASE/mobile/sync" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: tenant-alpha" \
  -H "x-api-key: test" \
  -H "x-device-id: $DEVICE_A" \
  -d "{
    \"device_id\": \"$DEVICE_A\",
    \"status_updates\": [],
    \"approval_requests\": [],
    \"step_delegations\": [
      {\"request_id\":\"deleg-002\",\"instance_id\":\"$INST_A\",\"block_id\":\"bad_step\",\"handler\":\"call_llm\",\"params\":{\"auth\":\"credentials://nonexistent-key\"}}
    ],
    \"command_acks\": []
  }")
echo "  Sync (bad credential): $SYNC_BAD"

# Verify error response
SYNC_ERR_RESULT=$(curl -s -X POST "$BASE/mobile/sync" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: tenant-alpha" \
  -H "x-api-key: test" \
  -H "x-device-id: $DEVICE_A" \
  -d "{
    \"device_id\": \"$DEVICE_A\",
    \"status_updates\": [],
    \"approval_requests\": [],
    \"step_delegations\": [],
    \"command_acks\": []
  }")
HAS_ERROR=$(echo "$SYNC_ERR_RESULT" | python3 -c "
import json, sys
data = json.load(sys.stdin)
for cmd in data.get('commands', []):
    if cmd.get('type') == 'step_result':
        payload = cmd.get('payload', {})
        if not payload.get('success'):
            print(f\"  PASS: Credential error correctly returned: {payload.get('error', 'none')[:60]}...\")
" 2>/dev/null)
if [ -n "$HAS_ERROR" ]; then
  echo "$HAS_ERROR"
else
  echo "  WARNING: No error step_result found"
fi

echo ""
echo "=== Done ==="
echo "All scenarios tested. Open dashboard at http://localhost:5173/mobile to verify visually."
