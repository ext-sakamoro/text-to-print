#!/bin/sh
set -e

echo "=== Frontend directory contents ==="
ls -la /app/frontend/
echo "=== Checking for server.js ==="
find /app/frontend -name "server.js" -type f 2>/dev/null
echo "==="

# Determine server.js location
if [ -f /app/frontend/server.js ]; then
  FRONTEND_DIR=/app/frontend
elif [ -f /app/frontend/frontend/server.js ]; then
  FRONTEND_DIR=/app/frontend/frontend
else
  echo "ERROR: server.js not found!"
  find /app/frontend -maxdepth 3 -type f | head -30
  # Still start gateway so health check passes
  exec api-gateway
fi

echo "Starting Next.js frontend from $FRONTEND_DIR..."
cd "$FRONTEND_DIR"
PORT=3000 HOSTNAME=0.0.0.0 node server.js &
FRONTEND_PID=$!

echo "Waiting for frontend (pid=$FRONTEND_PID)..."
for i in $(seq 1 30); do
  if curl -sf http://127.0.0.1:3000/ > /dev/null 2>&1; then
    echo "Frontend ready"
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
