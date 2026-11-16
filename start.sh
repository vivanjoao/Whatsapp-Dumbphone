#!/bin/sh
set -eu
cd "$(dirname "$0")"
if [ ! -f .env ]; then
    cp .env.example .env
    chmod 600 .env
    echo 'Created .env. Set a unique PASSWORD and random SESSION_SECRET before running again.'
    exit 1
fi
docker compose up -d --build
echo 'Gateway HTTP is bound to localhost:3000. Configure HTTPS before remote access.'
echo 'Pair via the QR shown below; do not share terminal logs.'
exec docker compose logs -f
