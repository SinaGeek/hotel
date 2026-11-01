# PostgreSQL backup (README §17: the desktop copy is not a backup).
#
# Usage (PowerShell):
#   .\scripts\backup-postgres.ps1
#   .\scripts\backup-postgres.ps1 -Database hms -OutDir D:\backups -RetainDays 30
#
# Environment:
#   PGPASSWORD          password for the backup role (or use a .pgpass file)
#   HMS_BACKUP_DIR      default output directory (default: .\backups)
#
# Restore:
#   createdb -h localhost -U postgres hms_restore
#   pg_restore -h localhost -U postgres -d hms_restore --clean --if-exists <file>.dump
#
# Schedule it with Task Scheduler (daily) or a cron/systemd timer on Linux.
# Copy the dumps off the machine afterwards; a local dump does not survive disk loss.

[CmdletBinding()]
param(
    [string]$Database = $(if ($env:POSTGRES_DB) { $env:POSTGRES_DB } else { "hms" }),
    [string]$PgHost = $(if ($env:PGHOST) { $env:PGHOST } else { "localhost" }),
    [int]$Port = $(if ($env:PGPORT) { [int]$env:PGPORT } else { 5432 }),
    [string]$User = $(if ($env:POSTGRES_USER) { $env:POSTGRES_USER } else { "postgres" }),
    [string]$OutDir = $(if ($env:HMS_BACKUP_DIR) { $env:HMS_BACKUP_DIR } else { Join-Path (Get-Location) "backups" }),
    [int]$RetainDays = 30,
    [string]$PgBin = $(if ($env:PG_BIN) { $env:PG_BIN } else { "C:\Program Files\PostgreSQL\18\bin" })
)

$ErrorActionPreference = "Stop"

$pgDump = Join-Path $PgBin "pg_dump.exe"
if (-not (Test-Path $pgDump)) {
    throw "pg_dump.exe not found at $pgDump. Set PG_BIN to your PostgreSQL bin directory."
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$target = Join-Path $OutDir "$Database-$stamp.dump"

Write-Host "Backing up $Database from $PgHost`:$Port to $target"
& $pgDump --host $PgHost --port $Port --username $User --dbname $Database `
    --format=custom --compress=9 --file $target
if ($LASTEXITCODE -ne 0) { throw "pg_dump failed with exit code $LASTEXITCODE" }

# Verify the archive is readable before trusting it.
& (Join-Path $PgBin "pg_restore.exe") --list $target | Out-Null
if ($LASTEXITCODE -ne 0) { throw "pg_restore --list could not read the dump" }

$size = [math]::Round((Get-Item $target).Length / 1MB, 1)
Write-Host "OK: $target ($size MB)"

# Prune old dumps.
$cutoff = (Get-Date).AddDays(-$RetainDays)
Get-ChildItem -Path $OutDir -Filter "$Database-*.dump" |
    Where-Object { $_.LastWriteTime -lt $cutoff } |
    ForEach-Object {
        Write-Host "Pruning $($_.Name)"
        Remove-Item $_.FullName -Force
    }

Write-Host "Copy this dump off the machine (object storage, another host, or tape)."
