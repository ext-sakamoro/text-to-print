#!/usr/bin/env bash
# smoke_worker.sh — Cloudflare Worker endpoint 疎通確認
#
# usage:
#   scripts/smoke_worker.sh                    # default endpoint
#   scripts/smoke_worker.sh -v                 # verbose (body 先頭も表示)
#   WORKER_BASE_URL=https://staging ... smoke_worker.sh
#
# checks:
#   1. GET  /api/presets              → 200
#   2. POST /api/share          {}    → 400 (validation reject = endpoint alive)
#   3. GET  /api/gallery/list         → 200 (GALLERY_DB provision 済判定)
#   4. POST /api/gallery/publish {}   → 400
#
# exit 0 = all pass, exit 1 = 1+ fail

set -euo pipefail
IFS=$'\n\t'

BASE="${WORKER_BASE_URL:-https://text-to-print.alicelaw.net}"
VERBOSE=0
CURL_TIMEOUT=10

while [[ $# -gt 0 ]]; do
  case "$1" in
    -v|--verbose) VERBOSE=1; shift ;;
    -h|--help)
      sed -n '2,15p' "$0"
      exit 0
      ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [[ -t 1 ]]; then
  GREEN=$'\033[0;32m'; RED=$'\033[0;31m'; YELLOW=$'\033[0;33m'; DIM=$'\033[2m'; RESET=$'\033[0m'
else
  GREEN=""; RED=""; YELLOW=""; DIM=""; RESET=""
fi

pass_count=0
fail_count=0

# args: name method path expected_status body
check() {
  local name="$1" method="$2" path="$3" expected="$4" body="${5:-}"
  local url="${BASE}${path}"
  local tmp_body; tmp_body=$(mktemp)
  local http_code curl_exit=0

  if [[ "$method" == "GET" ]]; then
    http_code=$(curl -s -o "$tmp_body" -w "%{http_code}" --max-time "$CURL_TIMEOUT" "$url") || curl_exit=$?
  else
    http_code=$(curl -s -o "$tmp_body" -w "%{http_code}" --max-time "$CURL_TIMEOUT" \
      -X "$method" -H "Content-Type: application/json" -d "$body" "$url") || curl_exit=$?
  fi

  if [[ $curl_exit -ne 0 ]]; then
    printf "  %s[FAIL]%s %-30s %s %s\n" "$RED" "$RESET" "$name" "$method" "$path"
    printf "         %scurl failed (exit=%d, network/DNS/TLS)%s\n" "$DIM" "$curl_exit" "$RESET"
    fail_count=$((fail_count + 1))
    rm -f "$tmp_body"
    return
  fi

  if [[ "$http_code" == "$expected" ]]; then
    printf "  %s[OK]%s   %-30s %s %s → %s\n" "$GREEN" "$RESET" "$name" "$method" "$path" "$http_code"
    pass_count=$((pass_count + 1))
  else
    printf "  %s[FAIL]%s %-30s %s %s → expected %s got %s\n" "$RED" "$RESET" "$name" "$method" "$path" "$expected" "$http_code"
    fail_count=$((fail_count + 1))

    # GALLERY_DB 未 provision hint (path は query string 含む可能性あり)
    if [[ "$path" == /api/gallery/* && ( "$http_code" == "500" || "$http_code" == "404" ) ]]; then
      printf "         %shint: GALLERY_DB 未 provision の可能性 (crates/worker で:%s\n" "$YELLOW" "$RESET"
      printf "         %s        wrangler d1 create text-to-print-gallery%s\n" "$YELLOW" "$RESET"
      printf "         %s        wrangler d1 execute text-to-print-gallery --file=migrations/0003_gallery.sql%s\n" "$YELLOW" "$RESET"
      printf "         %s        wrangler deploy)%s\n" "$YELLOW" "$RESET"
    fi
  fi

  if [[ $VERBOSE -eq 1 ]]; then
    local snippet; snippet=$(head -c 200 "$tmp_body" | tr -d '\n' || true)
    printf "         %sbody: %s%s\n" "$DIM" "$snippet" "$RESET"
  fi

  rm -f "$tmp_body"
}

echo "smoke_worker.sh — testing $BASE"
echo

check "presets list"      GET  "/api/presets"              200
check "share reject"      POST "/api/share"                400 "{}"
check "gallery list"      GET  "/api/gallery/list?limit=5" 200
check "gallery publish"   POST "/api/gallery/publish"      400 "{}"

echo
total=$((pass_count + fail_count))
if [[ $fail_count -eq 0 ]]; then
  printf "%sall %d checks passed%s\n" "$GREEN" "$total" "$RESET"
  exit 0
else
  printf "%s%d / %d checks failed%s\n" "$RED" "$fail_count" "$total" "$RESET"
  exit 1
fi
