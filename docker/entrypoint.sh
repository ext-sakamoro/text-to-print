#!/bin/sh
set -e

echo "Starting Next.js frontend..."
cd /app/frontend
PORT=3000 HOSTNAME=0.0.0.0 node server.js &
FRONTEND_PID=$!

echo "Waiting for frontend (pid=$FRONTEND_PID)..."
for i in $(seq 1 30); do
  if curl -sf http://127.0.0.1:3000/ > /dev/null 2>&1; then
    echo "Frontend ready"
    break
  fi
  sleep 1
done

echo "Starting API Gateway..."
exec api-gateway
