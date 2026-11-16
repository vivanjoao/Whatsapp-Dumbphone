@echo off
setlocal
cd /d "%~dp0"
if not exist .env (
    copy .env.example .env >nul
    echo Created .env. Set a unique PASSWORD and random SESSION_SECRET, then run again.
    exit /b 1
)
docker compose up -d --build
if errorlevel 1 exit /b 1
echo Gateway HTTP is bound to localhost:3000. Configure HTTPS before remote access.
echo Pair via the QR below. Do not share terminal logs.
docker compose logs -f
