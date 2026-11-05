# scripts/dev-stack.ps1 â€” native (no-Docker) replacement for `docker compose up -d --build`.
#
# Runs the same production-stack pieces directly on Windows:
#   1. PostgreSQL 18 (started via pg_ctl if not already listening on 5432)
#   2. hms-server   (cargo run; applies SQL migrations automatically at boot)
#   3. optional: Caddy TLS proxy + static SPA + update host (-WithCaddy,
#      config: deploy/Caddyfile.native; needs apps/web/dist and hosts entries)
#
# Usage (PowerShell):
#   .\scripts\dev-stack.ps1                 # start everything, wait for /health
#   .\scripts\dev-stack.ps1 -WithCaddy      # also start the local TLS proxy
#   .\scripts\dev-stack.ps1 -Stop           # stop server (+ caddy); Postgres stays up
#   .\scripts\dev-stack.ps1 -Stop -StopPostgres
#
# State (pids/logs/secret) lives in .stack/ and is gitignored.

param(
    [switch]$Stop,
    [switch]$StopPostgres,
    [switch]$WithCaddy,
    [string]$DatabaseUrl = "postgres://postgres:1234@localhost:5432/hms",
    [int]$ServerPort = 8080
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
$PgBin    = "C:\Program Files\PostgreSQL\18\bin"
$PgData   = "C:\Program Files\PostgreSQL\18\data"
$StackDir = Join-Path $RepoRoot ".stack"
New-Item -ItemType Directory -Force -Path $StackDir | Out-Null
$ServerOutLog = Join-Path $StackDir "server.out.log"
$ServerErrLog = Join-Path $StackDir "server.err.log"
$CaddyLog     = Join-Path $StackDir "caddy.out.log"
$CaddyErrLog  = Join-Path $StackDir "caddy.err.log"

function Test-PortListening {
    param([int]$Port)
    return [bool](Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue)
}

function Wait-PortListening {
    param([int]$Port, [int]$Seconds)
    $deadline = (Get-Date).AddSeconds($Seconds)
    while (-not (Test-PortListening $Port) -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 500
    }
    return (Test-PortListening $Port)
}

function Stop-PortProcess {
    param([int]$Port)
    $conns = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
    if ($conns) {
        $conns.OwningProcess | Sort-Object -Unique | ForEach-Object {
            if ($_ -and $_ -ne $PID) {
                Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue
            }
        }
        return $true
    }
    return $false
}

function Get-Health {
    param([string]$Url)
    try {
        $r = Invoke-WebRequest -UseBasicParsing -Uri $Url -TimeoutSec 3
        return $r.Content
    } catch {
        return $null
    }
}

if ($Stop) {
    $killedServer = Stop-PortProcess -Port $ServerPort
    if ($killedServer) { "server stopped (port $ServerPort released)" } else { "no server listening on port $ServerPort" }

    $caddyPidFile = Join-Path $StackDir "caddy.pid"
    if (Test-Path $caddyPidFile) {
        $cpid = Get-Content $caddyPidFile -ErrorAction SilentlyContinue
        if ($cpid) { Stop-Process -Id $cpid -Force -ErrorAction SilentlyContinue }
        Remove-Item $caddyPidFile -ErrorAction SilentlyContinue
        "caddy stopped"
    }

    if ($StopPostgres) {
        & "$PgBin\pg_ctl.exe" stop -D $PgData -m fast | Out-Null
        "postgres stopped"
    } else {
        "postgres left running (use -StopPostgres to stop it)"
    }
    exit 0
}

# 1) PostgreSQL ---------------------------------------------------------------
if (Test-PortListening 5432) {
    "postgres: already listening on 5432"
} else {
    "postgres: starting via pg_ctl (data dir: $PgData)"
    & "$PgBin\pg_ctl.exe" start -D $PgData -l "$PgData\hms-dev.log" -w | Out-Null
    if (-not (Wait-PortListening -Port 5432 -Seconds 60)) {
        throw "postgres did not start; see $PgData\hms-dev.log"
    }
}

# 2) hms-server ---------------------------------------------------------------
if (Test-PortListening $ServerPort) {
    "server: already listening on port $ServerPort - not starting another instance"
} else {
    # Stable session secret across restarts (stored once), so dev sessions survive.
    $secretFile = Join-Path $StackDir "secret.txt"
    if (Test-Path $secretFile) {
        $SessionSecret = Get-Content $secretFile -Raw
    } else {
        $SessionSecret = [Convert]::ToBase64String((1..30 | ForEach-Object { Get-Random -Maximum 256 }))
        Set-Content -Path $secretFile -Value $SessionSecret -NoNewline
    }

    "server: starting (cargo run -p hms-server, migrations auto-apply at boot)"
    $env:DATABASE_URL   = $DatabaseUrl
    $env:SESSION_SECRET = $SessionSecret
    $env:APP_ENV        = "development"
    $env:RUST_LOG       = "info"
    # cargo run stays alive as the server's wrapper process; logs go to .stack/.
    $proc = Start-Process -FilePath "cargo" `
        -ArgumentList @("run", "-q", "-p", "hms-server") `
        -WorkingDirectory $RepoRoot `
        -RedirectStandardOutput $ServerOutLog -RedirectStandardError $ServerErrLog `
        -PassThru -WindowStyle Hidden
    "server: cargo pid $($proc.Id), logs: $ServerErrLog"

    # First run compiles, so allow a generous wait.
    if (-not (Wait-PortListening -Port $ServerPort -Seconds 420)) {
        Get-Content $ServerErrLog -Tail 40 -ErrorAction SilentlyContinue
        throw "server did not bind port $ServerPort within 7 minutes; see $ServerErrLog"
    }
}

# 3) optional Caddy TLS proxy --------------------------------------------------
if ($WithCaddy) {
    $caddyExe = Join-Path $env:LOCALAPPDATA "Tools\caddy\caddy.exe"
    if (-not (Test-Path $caddyExe)) { throw "caddy.exe not found at $caddyExe (see docs in deploy/Caddyfile.native)" }
    $caddyPidFile = Join-Path $StackDir "caddy.pid"
    if ((Test-PortListening 8443) -or (Test-Path $caddyPidFile)) {
        "caddy: already running (pid file present)"
    } else {
        "caddy: starting with deploy\Caddyfile.native"
        $proc = Start-Process -FilePath $caddyExe `
            -ArgumentList @("run", "--config", (Join-Path $RepoRoot "deploy\Caddyfile.native")) `
            -WorkingDirectory $RepoRoot `
            -RedirectStandardOutput $CaddyLog -RedirectStandardError $CaddyErrLog `
            -PassThru -WindowStyle Hidden
        Set-Content -Path $caddyPidFile -Value $proc.Id
        if (-not (Wait-PortListening -Port 8443 -Seconds 30)) {
            Get-Content $CaddyLog -Tail 40 -ErrorAction SilentlyContinue
            throw "caddy did not bind 8443; see $CaddyLog"
        }
        if (-not (Wait-PortListening -Port 9443 -Seconds 15)) {
            Get-Content $CaddyLog -Tail 40 -ErrorAction SilentlyContinue
            throw "caddy did not bind 9443 (update host); see $CaddyLog"
        }
    }
}

# 4) status --------------------------------------------------------------------
$health = Get-Health -Url "http://127.0.0.1:$ServerPort/health"
if (-not $health) { throw "server is listening but /health failed" }
"stack ready:"
"  api      http://127.0.0.1:$ServerPort/health  -> $health"
if ($WithCaddy) { "  site     https://localhost:8443/  (SPA + /api via Caddy, local CA); update host https://localhost:9443/" }
"  logs     $ServerErrLog"

