#!/bin/sh
set -e

# Start Next.js frontend on port 3000 (background)
cd /app/frontend
PORT=3000 node server.js &
FRONTEND_PID=$!

# Wait for frontend to be ready
for i in $(seq 1 30); do
  if wget -q -O /dev/null http://127.0.0.1:3000/auth/login 2>/dev/null; then
    break
  fi
  sleep 1
done

# Start Rust API Gateway on $PORT (foreground)
exec api-gateway
