#!/bin/bash
set -e
BASE="${1:-http://localhost:8080}"
echo "Testing $BASE"
curl -sf "$BASE/health" | grep -q '"status":"ok"' && echo "PASS /health" || echo "FAIL /health"
curl -sf "$BASE/license" | grep -q '"license"' && echo "PASS /license" || echo "FAIL /license"
curl -so /dev/null -w "%{http_code}" "$BASE/api/v1/health" | grep -q "401" && echo "PASS /api/v1 (auth required)" || echo "FAIL /api/v1"
