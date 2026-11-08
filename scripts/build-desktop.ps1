# Build the desktop app with SQLCipher (todo C1): enters the MSVC environment
# and prepends the portable Perl + NASM that OpenSSL's build needs.
# Usage (from anywhere): powershell -ExecutionPolicy Bypass -File scripts\build-desktop.ps1 [cargo args...]
# The client workspace lives at client\ — cargo is always run from there.
$ErrorActionPreference = "Stop"

$vcvars = "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if (-not (Test-Path $vcvars)) {
    throw "MSVC build tools not found at $vcvars (install 'Desktop development with C++')"
}
$perlBin = Join-Path $env:LOCALAPPDATA "Tools\perl\perl\bin"
$nasmBin = Join-Path $env:LOCALAPPDATA "Tools\nasm-2.16.03"
if (-not (Test-Path (Join-Path $perlBin "perl.exe"))) { throw "Strawberry Perl not found at $perlBin" }
if (-not (Test-Path (Join-Path $nasmBin "nasm.exe"))) { throw "NASM not found at $nasmBin" }

$extraArgs = if ($args.Count -gt 0) { $args -join " " } else { "check -p hms-desktop --all-targets" }
# Run inside cmd after vcvars so cl.exe/link.exe and the Win SDK are visible.
$clientDir = Join-Path (Split-Path -Parent $PSScriptRoot) "client"
$cmd = 'call "{0}" >nul && set "PATH={1};{2};%PATH%" && cd /d "{3}" && cargo {4}' -f $vcvars, $perlBin, $nasmBin, $clientDir, $extraArgs
cmd.exe /c $cmd
exit $LASTEXITCODE
