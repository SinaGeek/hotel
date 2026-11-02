# Desktop auto-updater (todo D1–D6)

The Windows desktop app updates itself: it checks a signed manifest at start
and every 4 hours, downloads in the background, and installs only when the
user confirms — except when the server forces it.

## 1. Keys

- Generated once with `pnpm exec tauri signer generate`.
- **Private key**: `hms.key` — lives outside the repository
  (`%USERPROFILE%\.hms-signing\hms.key` locally, GitHub secret
  `TAURI_SIGNING_PRIVATE_KEY` in CI) together with its password
  (`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`).
- **Public key**: embedded in `apps/desktop/src-tauri/tauri.conf.json`
  (`plugins.updater.pubkey`). It is safe to commit; only holders of the
  private key can produce an installable package, so a tampered MSI is
  rejected during `download()`.

Rotating the key means every installed build must be replaced by hand once
(an app only trusts the key baked into it).

## 2. Update host

`plugins.updater.endpoints` points at `…/desktop/latest.json`. The manifest
the release workflow produces looks like:

```json
{
  "version": "0.2.0",
  "notes": "HMS Desktop 0.2.0",
  "pub_date": "2026-11-02T00:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "<contents of the .msi.sig file>",
      "url": "https://updates.example/desktop/HMS Desktop_0.2.0_x64_en-US.msi"
    }
  }
}
```

Serve it over HTTPS with the MSI beside it. Point the real host into
`tauri.conf.json` before the first production build (the placeholder is
`updates.hms.example`).

## 3. Behaviour in the client

| Concern | Where | What happens |
|---|---|---|
| Background check | `updater::spawn_checker` | on start + every 4 h; failures are logged, never surfaced as app errors |
| Pill / prompt | `apps/web/src/components/UpdatePill.tsx` | "Update X available" + explicit **Install** button |
| Never during a form | `UpdatePill` | nothing installs without a click |
| Sync before install | `update_install` | one final cycle drains the outbox; a failed sync **aborts** the install |
| Forced update | `SyncFailure::UpdateRequired` → `SyncState::UpdateRequired` | server `426` flips a sticky flag; the pill turns red and says syncing is blocked until the install |

## 4. Local data across an update

`hms.sqlite` and `hms.key` live in `%APPDATA%\com.hms.desktop`, outside the
install directory, so an MSI upgrade never touches them. Schema migrations
(`desktop-migrations/*.sql`, guarded by `PRAGMA user_version`) run on the first
open of the new build. Back up both files before a manual upgrade — the
database is useless without its DPAPI key blob.

## 5. Release checklist

1. Bump `version` in `apps/desktop/src-tauri/tauri.conf.json`.
2. Tag `vX.Y.Z` and push — `.github/workflows/release.yml` builds the web
   bundle, signs the MSI (`TAURI_SIGNING_PRIVATE_KEY`), writes `latest.json`,
   uploads the artifacts, and publishes when `UPDATE_PUBLISH_CMD` is set.
3. Keep an older install running, publish the new version, and confirm the app
   offers it, installs it, and keeps its local data (accept criterion of D6).
4. If the release raises `SYNC_PROTOCOL_MIN_VERSION`, older builds get `426` at
   their next sync and require the update — verify that path before announcing.
