# Desktop Build Prerequisites & Guide

This document describes the prerequisites and steps to build the HMS Windows desktop application with SQLCipher encrypted local database and Tauri 2 shell.

## 1. Prerequisites

### 1.1 MSVC C++ Build Tools
- Visual Studio Build Tools 2022+ (v143 / v18+ toolset).
- Includes MSVC compiler (`cl.exe`), linker (`link.exe`), and Windows 10/11 SDK.
- Location on this host: `C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat`.

### 1.2 Strawberry Perl
- Required by `openssl-src` to configure OpenSSL for Windows MSVC during the `bundled-sqlcipher-vendored-openssl` build.
- Installed at: `%LOCALAPPDATA%\Tools\perl\perl\bin` (added to user PATH).

### 1.3 NASM (Netwide Assembler)
- Required for optimized assembly routines during OpenSSL compilation.
- Installed at: `%LOCALAPPDATA%\Tools\nasm-2.16.03` (added to user PATH).

### 1.4 Rust & Tauri CLI
- Rust toolchain: `x86_64-pc-windows-msvc`.
- Tauri CLI: `pnpm tauri` (Tauri 2).
- Node.js & pnpm.

## 2. Building

### 2.1 Webview assets
Before building or bundling the desktop app, build the static web application:
```powershell
pnpm --dir apps/web build
```

### 2.2 Desktop Application
To build the debug desktop application:
```powershell
cargo build -p hms-desktop
```

To build the release binary / MSI package:
```powershell
pnpm tauri build
```
The installer output is generated under `apps/desktop/src-tauri/target/release/bundle/msi/`.

## 3. Database Encryption Architecture (SQLCipher)
- Engine: SQLCipher 4 via `rusqlite` feature `bundled-sqlcipher-vendored-openssl`.
- Key Generation: Cryptographically secure 32-byte key generated per installation using OS RNG (`getrandom`).
- Key Storage: Stored DPAPI-protected in Windows credential store / DPAPI key blob in `hms.key` inside `%APPDATA%\com.hms.desktop`.
- Cipher Security:
  - `PRAGMA key = "x'...'"` issued immediately upon connection open.
  - `PRAGMA cipher_memory_security = ON` enabled.
- Plaintext Migration: Existing plaintext SQLite files are automatically upgraded to encrypted files via `sqlcipher_export()`.
