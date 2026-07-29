#!/bin/sh
set -e

# Start Next.js frontend on port 3000 (background)
cd /app/frontend
PORT=3000 HOSTNAME=0.0.0.0 node server.js &

# Give Node.js a moment to bind
sleep 2

# Start Rust API Gateway on $PORT (foreground)
exec api-gateway
