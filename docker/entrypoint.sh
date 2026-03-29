#!/bin/sh
set -e

echo "Starting Next.js frontend..."
cd /app/frontend
PORT=3000 HOSTNAME=0.0.0.0 node server.js &
FRONTEND_PID=$!

echo "Waiting for frontend (pid=$FRONTEND_PID)..."
for i in $(seq 1 30); do
  # Use -o /dev/null to discard body, check any HTTP response (even 404)
  HTTP_CODE=$(curl -so /dev/null -w "%{http_code}" http://127.0.0.1:3000/auth/login 2>/dev/null || echo "000")
  if [ "$HTTP_CODE" != "000" ]; then
    echo "Frontend ready (HTTP $HTTP_CODE)"
    break
  fi
  if ! kill -0 $FRONTEND_PID 2>/dev/null; then
    echo "Frontend process died!"
    break
  fi
  sleep 1
done

echo "Starting API Gateway..."
exec api-gateway
