#!/usr/bin/env bash
# Public-contract boundary lint. The published plugin may reference only what
# an external developer with an API key can see. Fails (strict) or counts
# (report) any reference to backend internals.
#   check-public-boundary.sh [--strict|--report] <file-or-dir>...
set -euo pipefail
mode=strict
if [ "${1:-}" = "--strict" ] || [ "${1:-}" = "--report" ]; then mode="${1#--}"; shift; fi
[ $# -gt 0 ] || { echo "usage: $0 [--strict|--report] <path>..." >&2; exit 2; }
command -v rg >/dev/null 2>&1 || { echo "error: ripgrep (rg) is required" >&2; exit 2; }

patterns=(
  # backend source paths and service/crate names
  '\b(src|crates|packages|apps)/[A-Za-z0-9_./-]+\.(rs|ts|tsx|js|py|go)\b'
  '\b(proto-engine|app-builder-api|erpai-agent-v2|erpai-gateway|erpai-permission-api|erpai-onboarding|erpai-ingest|injest|herbert|config-server)\b'
  '\b(AGENTS|CLAUDE)\.md\b'
  # internal storage, queues, ops
  '\b(autobuilder_[a-z_]+|v2_engine|perm_engine|app_builder_v2|auto_builder_db|erpai_onboarding_db)\b'
  '\bv2\.(app-builder|auto-builder)[a-z-]*\.[a-z-]+\b'
  '\b(ReplacingMergeTree|Kafka|kafka|Redis Streams|MongoDB|mongo)\b'
  '\b(psql|kubectl|docker|ssh|scp|rsync)\b'
  # internal auth and routes
  '/internal/'
  '/token/(user|org|app)/'
  '\b(INTERNAL_JWT_SECRET|x-internal-token|x-gateway-[a-z-]+|ERPAI_ONBOARDING_API_INTERNAL_BASE_URL|PROTO_ENGINE)\b'
  # hosts other than the public ones
  '\b(make-api\.erpai\.dev|erpai\.dev|erpai\.studio|new\.erp\.ai|pw\.erp\.ai|deskera\.com|hetzner|erpai-v2-(app|db))\b'
  '\b([0-9]{1,3}\.){3}[0-9]{1,3}\b'
  # real identifiers in examples (placeholders only)
  '\b[0-9a-f]{24}\b'
  '\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b'
)
allow=(
  '127\.0\.0\.1'
  'https://apps\.erp\.ai'
  'https://github\.com/erphq/'
)

args=(-n --no-heading --color never --hidden --glob '!*.local.md' --glob '!node_modules' --glob '!.git')
for p in "${patterns[@]}"; do args+=(-e "$p"); done
hits="$(rg "${args[@]}" "$@" 2>/dev/null || true)"
for a in "${allow[@]}"; do hits="$(printf '%s\n' "$hits" | rg -v -e "$a" || true)"; done
hits="$(printf '%s\n' "$hits" | sed '/^$/d')"
count="$(printf '%s' "$hits" | grep -c . || true)"

if [ "$count" -eq 0 ]; then echo "public boundary: ok"; exit 0; fi
printf '%s\n' "$hits"
echo "public boundary: violations: $count"
[ "$mode" = report ] && exit 0
echo "error: published content must not reference backend internals (see docs/superpowers/specs/2026-09-08-agent-plugins-and-erpai-cli-design.md §3.4)" >&2
exit 1
