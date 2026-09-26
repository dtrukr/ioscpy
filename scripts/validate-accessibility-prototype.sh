#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IOSCPY_BIN="${IOSCPY_BIN:-$ROOT/host/target/debug/ioscpy}"
DEVICE_UDID="${IOSCPY_DEVICE:-}"
OUT_DIR=""
TAP_FIRST=0
SELECTOR_READY=0
SELECTOR_PROOF=""
SELECTOR_PROOF_TEMPLATE=""

usage() {
  cat <<'USAGE'
usage: scripts/validate-accessibility-prototype.sh [--device UDID] [--out-dir DIR] [--tap-first]
                                                    [--selector-ready --selector-proof FILE]
                                                    [--selector-proof-template FILE]

Validates the prototype accessibility path against an attached jailbroken iPhone
with the matching ioscpy device package installed.

The default run checks:
  - the host binary builds/runs
  - the phone advertises accessibility support
  - --accessibility-tree matches protocol/accessibility.schema.json
  - validation-summary.json matches protocol/accessibility-validation.schema.json

--tap-first also sends an accessibility tap action to the first enabled visible
node with a non-empty frame. Use it only on a safe screen.

--selector-ready records that this run validated committed MCP selector
semantics, not only the prototype tree/action transport. It requires
--tap-first, --selector-proof FILE, and either --device UDID or exactly one
listed device. It should not be used for the SpringBoard-only UIKit prototype.

--selector-proof-template FILE writes a draft selector proof from the captured
tree. It starts every case as ok=false and is not accepted by --selector-ready
until the cases are exercised on the device and filled with real evidence.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --device)
      DEVICE_UDID="${2:-}"
      shift 2
      ;;
    --out-dir)
      OUT_DIR="${2:-}"
      shift 2
      ;;
    --tap-first)
      TAP_FIRST=1
      shift
      ;;
    --selector-ready)
      SELECTOR_READY=1
      shift
      ;;
    --selector-proof)
      SELECTOR_PROOF="${2:-}"
      shift 2
      ;;
    --selector-proof-template)
      SELECTOR_PROOF_TEMPLATE="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "$SELECTOR_READY" -eq 1 && "$TAP_FIRST" -ne 1 ]]; then
  echo "--selector-ready requires --tap-first" >&2
  exit 2
fi

if [[ -n "$SELECTOR_PROOF_TEMPLATE" && "$TAP_FIRST" -ne 1 ]]; then
  echo "--selector-proof-template requires --tap-first" >&2
  exit 2
fi

if [[ "$SELECTOR_READY" -eq 1 ]]; then
  if [[ -z "$SELECTOR_PROOF" ]]; then
    echo "--selector-ready requires --selector-proof FILE" >&2
    exit 2
  fi
  if [[ ! -f "$SELECTOR_PROOF" ]]; then
    echo "selector proof file not found: $SELECTOR_PROOF" >&2
    exit 2
  fi
fi

if [[ -z "$OUT_DIR" ]]; then
  OUT_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ioscpy-ax-validate.XXXXXX")"
else
  mkdir -p "$OUT_DIR"
fi

if [[ ! -x "$IOSCPY_BIN" ]]; then
  (cd "$ROOT/host" && cargo build)
fi

DEVICE_ARGS=()
if [[ -n "$DEVICE_UDID" ]]; then
  DEVICE_ARGS=(--device "$DEVICE_UDID")
fi

run_ioscpy() {
  if [[ ${#DEVICE_ARGS[@]} -gt 0 ]]; then
    "$IOSCPY_BIN" "${DEVICE_ARGS[@]}" "$@"
  else
    "$IOSCPY_BIN" "$@"
  fi
}

echo "writing validation artifacts to $OUT_DIR"

run_ioscpy --list > "$OUT_DIR/devices.txt"
if grep -q "No devices attached" "$OUT_DIR/devices.txt"; then
  echo "no iPhone found over USB; see $OUT_DIR/devices.txt" >&2
  exit 1
fi
SUMMARY_DEVICE_UDID="$DEVICE_UDID"
if [[ -z "$SUMMARY_DEVICE_UDID" ]]; then
  SUMMARY_DEVICE_UDID="$(awk 'NF { print $1 }' "$OUT_DIR/devices.txt" | sed -n '1p')"
  if [[ "$(awk 'NF { count++ } END { print count + 0 }' "$OUT_DIR/devices.txt")" -ne 1 ]]; then
    SUMMARY_DEVICE_UDID=""
  fi
fi
if [[ "$SELECTOR_READY" -eq 1 && -z "$SUMMARY_DEVICE_UDID" ]]; then
  echo "--selector-ready requires --device UDID or exactly one listed device" >&2
  exit 2
fi
if [[ -n "$SELECTOR_PROOF_TEMPLATE" && -z "$SUMMARY_DEVICE_UDID" ]]; then
  echo "--selector-proof-template requires --device UDID or exactly one listed device" >&2
  exit 2
fi

run_ioscpy --debug > "$OUT_DIR/debug.txt"
run_ioscpy --accessibility-tree > "$OUT_DIR/tree.json"
node "$ROOT/scripts/validate-accessibility-artifact.js" \
  tree \
  "$ROOT/protocol/accessibility.schema.json" \
  "$OUT_DIR/tree.json"

if [[ "$TAP_FIRST" -eq 1 ]]; then
  ACTION_JSON="$(node - "$OUT_DIR/tree.json" <<'NODE'
const fs = require("fs");
const tree = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
const node = tree.nodes.find((candidate) => {
  const frame = candidate && candidate.frame;
  return candidate.enabled !== false
    && candidate.hidden !== true
    && frame
    && Number(frame.width) > 1
    && Number(frame.height) > 1;
});
if (!node) {
  throw new Error("no enabled visible node with a non-empty frame");
}
process.stdout.write(JSON.stringify({
  schema: "ioscpy.accessibility.action.v1",
  action: "tap",
  target_id: node.id,
  node: {
    id: node.id,
    frame: node.frame,
  },
}));
NODE
)"
  printf '%s\n' "$ACTION_JSON" > "$OUT_DIR/action-request.json"
  node "$ROOT/scripts/validate-accessibility-artifact.js" \
    action-request \
    "$ROOT/protocol/accessibility-action.schema.json" \
    "$OUT_DIR/action-request.json"
  run_ioscpy --accessibility-action "$ACTION_JSON" > "$OUT_DIR/action-result.json"
  node "$ROOT/scripts/validate-accessibility-artifact.js" \
    action-result \
    "$ROOT/protocol/accessibility-action.schema.json" \
    "$OUT_DIR/action-result.json"
fi

if [[ "$SELECTOR_READY" -eq 1 ]]; then
  cp "$SELECTOR_PROOF" "$OUT_DIR/selector-proof.json"
  node "$ROOT/scripts/validate-accessibility-selector-proof.js" \
    "$ROOT/protocol/accessibility-selector-proof.schema.json" \
    "$OUT_DIR/selector-proof.json" \
    "$OUT_DIR/tree.json" \
    "$SUMMARY_DEVICE_UDID" \
    "$OUT_DIR/action-result.json"
fi

if [[ -n "$SELECTOR_PROOF_TEMPLATE" ]]; then
  node "$ROOT/scripts/create-accessibility-selector-proof-template.js" \
    "$OUT_DIR/tree.json" \
    "$SUMMARY_DEVICE_UDID" \
    "$SELECTOR_PROOF_TEMPLATE"
  echo "selector proof draft written to $SELECTOR_PROOF_TEMPLATE"
fi

node - "$OUT_DIR" "$TAP_FIRST" "$SUMMARY_DEVICE_UDID" "$SELECTOR_READY" <<'NODE'
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const outDir = process.argv[2];
const tapFirst = process.argv[3] === "1";
const deviceUdid = process.argv[4] || null;
const selectorReady = process.argv[5] === "1";
const tree = JSON.parse(fs.readFileSync(path.join(outDir, "tree.json"), "utf8"));
let action = null;
if (tapFirst) {
  action = JSON.parse(fs.readFileSync(path.join(outDir, "action-result.json"), "utf8"));
}
let selectorProof = null;
if (selectorReady) {
  const proof = fs.readFileSync(path.join(outDir, "selector-proof.json"));
  const proofJson = JSON.parse(proof.toString("utf8"));
  selectorProof = {
    file: "selector-proof.json",
    schema: proofJson.schema,
    sha256: crypto.createHash("sha256").update(proof).digest("hex"),
    bytes: proof.length,
  };
}
const summary = {
  schema: "ioscpy.accessibility.validation.v1",
  ok: true,
  validated_at: new Date().toISOString(),
  tap_first: tapFirst,
  selector_ready: selectorReady,
  selector_proof: selectorProof,
  device_udid: deviceUdid,
  tree: {
    schema: tree.schema,
    source: tree.source,
    snapshot_id: tree.snapshot_id ?? null,
    node_count: Array.isArray(tree.nodes) ? tree.nodes.length : 0,
    truncated: tree.truncated === true,
    host_application: tree.host_application ?? null,
    screen: tree.screen ?? null,
  },
  action: action ? {
    schema: action.schema,
    ok: action.ok === true,
    action: action.action ?? null,
    target_id: action.target_id ?? null,
    point: action.point ?? null,
  } : null,
  artifacts: {
    devices: "devices.txt",
    debug: "debug.txt",
    tree: "tree.json",
    action_request: tapFirst ? "action-request.json" : null,
    action_result: tapFirst ? "action-result.json" : null,
    selector_proof: selectorProof ? selectorProof.file : null,
  },
};
fs.writeFileSync(path.join(outDir, "validation-summary.json"), `${JSON.stringify(summary, null, 2)}\n`);
NODE

node "$ROOT/scripts/validate-accessibility-summary.js" \
  "$ROOT/protocol/accessibility-validation.schema.json" \
  "$OUT_DIR/validation-summary.json"

echo "accessibility prototype validation complete; summary: $OUT_DIR/validation-summary.json"
