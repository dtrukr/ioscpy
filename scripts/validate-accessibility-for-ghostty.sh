#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEFAULT_GHOSTTY_MCP_DIR="$(cd "$ROOT/.." 2>/dev/null && pwd)/ghostty-lean/webview-poc/mcp-server"

DEVICE_UDID="${IOSCPY_DEVICE:-}"
OUT_DIR="${TMPDIR:-/tmp}/ioscpy-ax-validation"
GHOSTTY_MCP_DIR="${GHOSTTY_MCP_DIR:-$DEFAULT_GHOSTTY_MCP_DIR}"
SELECTOR_READY=0
SELECTOR_PROOF=""
SELECTOR_PROOF_TEMPLATE=""
SKIP_GHOSTTY_CHECK=0

usage() {
  cat <<'USAGE'
usage: scripts/validate-accessibility-for-ghostty.sh [--device UDID] [--out-dir DIR]
                                                     [--selector-proof-template FILE]
                                                     [--selector-ready --selector-proof FILE]
                                                     [--ghostty-mcp-dir DIR]
                                                     [--skip-ghostty-check]

Runs the ioscpy jailbroken-iPhone accessibility validation with --tap-first and
then runs Ghostty MCP's validation-summary gate against the resulting summary.

Prototype runs prove transport only and run Ghostty's checker with --prototype-ok.
Selector-ready runs require a filled selector proof and run Ghostty's checker in
strict selector mode.
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
    --selector-proof-template)
      SELECTOR_PROOF_TEMPLATE="${2:-}"
      shift 2
      ;;
    --selector-ready)
      SELECTOR_READY=1
      shift
      ;;
    --selector-proof)
      SELECTOR_PROOF="${2:-}"
      shift 2
      ;;
    --ghostty-mcp-dir)
      GHOSTTY_MCP_DIR="${2:-}"
      shift 2
      ;;
    --skip-ghostty-check)
      SKIP_GHOSTTY_CHECK=1
      shift
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

if [[ "$SELECTOR_READY" -eq 1 && -z "$SELECTOR_PROOF" ]]; then
  echo "--selector-ready requires --selector-proof FILE" >&2
  exit 2
fi

validate_args=(--tap-first --out-dir "$OUT_DIR")
if [[ -n "$DEVICE_UDID" ]]; then
  validate_args+=(--device "$DEVICE_UDID")
fi
if [[ -n "$SELECTOR_PROOF_TEMPLATE" ]]; then
  validate_args+=(--selector-proof-template "$SELECTOR_PROOF_TEMPLATE")
fi
if [[ "$SELECTOR_READY" -eq 1 ]]; then
  validate_args+=(--selector-ready --selector-proof "$SELECTOR_PROOF")
fi

"$ROOT/scripts/validate-accessibility-prototype.sh" "${validate_args[@]}"

SUMMARY="$OUT_DIR/validation-summary.json"
if [[ ! -f "$SUMMARY" ]]; then
  echo "validation summary was not written: $SUMMARY" >&2
  exit 1
fi

if [[ "$SKIP_GHOSTTY_CHECK" -eq 1 ]]; then
  echo "skipping Ghostty validation-summary gate check"
  echo "validation summary: $SUMMARY"
  exit 0
fi

if [[ ! -d "$GHOSTTY_MCP_DIR" ]]; then
  echo "Ghostty MCP directory not found: $GHOSTTY_MCP_DIR" >&2
  echo "pass --ghostty-mcp-dir DIR or set GHOSTTY_MCP_DIR" >&2
  exit 2
fi

check_args=(--summary "$SUMMARY")
if [[ -n "$DEVICE_UDID" ]]; then
  check_args+=(--device "$DEVICE_UDID")
fi
if [[ "$SELECTOR_READY" -ne 1 ]]; then
  check_args+=(--prototype-ok)
fi

(cd "$GHOSTTY_MCP_DIR" && npm run check:iphone-accessibility-validation -- "${check_args[@]}")

echo "validation summary: $SUMMARY"
