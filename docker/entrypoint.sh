#!/bin/sh
set -e

echo "Starting Next.js frontend..."
cd /app/frontend
PORT=3000 HOSTNAME=0.0.0.0 node server.js &
FRONTEND_PID=$!

echo "Waiting for frontend (pid=$FRONTEND_PID)..."
READY=0
for i in $(seq 1 30); do
  if curl -sf http://127.0.0.1:3000/ > /dev/null 2>&1; then
    echo "Frontend ready on 127.0.0.1:3000"
    READY=1
    break
  fi
  if ! kill -0 $FRONTEND_PID 2>/dev/null; then
    echo "Frontend process died!"
    break
  fi
  sleep 1
done

if [ "$READY" = "0" ]; then
  echo "Frontend not ready, testing localhost..."
  curl -v http://127.0.0.1:3000/ 2>&1 || true
  curl -v http://localhost:3000/ 2>&1 || true
  echo "Checking network..."
  ss -tlnp 2>/dev/null || netstat -tlnp 2>/dev/null || true
fi

echo "Starting API Gateway..."
exec api-gateway
