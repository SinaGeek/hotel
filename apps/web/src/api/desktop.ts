// Desktop bridge (README §11): thin, typed wrappers over the Tauri IPC
// surface exposed by apps/desktop/src-tauri (sync_status, set_auth_token,
// remember_offline_credentials, offline_login, sync_now). Everything is a
// no-op throwing a clear Error in a plain browser, so the same web build
// serves both the REST web app and the offline desktop shell.

export interface SyncSnapshot {
  state: 'Synced' | 'Failing' | 'Locked' | 'Unknown'
  /** present for Synced { synced_at_ms, pending, conflicts } and Failing { reason, pending } */
  detail?:
    | { synced_at_ms: number; pending: number; conflicts: number }
    | { reason: string; pending: number }
}

/** True only inside the Tauri webview (injected internals). */
export const isDesktop =
  typeof window !== 'undefined' &&
  !!(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const w = window as unknown as { __TAURI_INTERNALS__: { invoke: (c: string, a?: Record<string, unknown>) => Promise<T> } }
  if (!isDesktop) return Promise.reject(new Error('not running in the desktop shell'))
  return w.__TAURI_INTERNALS__.invoke(cmd, args)
}

export const desktop = {
  // status pill source of truth (background worker keeps it fresh)
  syncStatus: () => invoke<SyncSnapshot>('sync_status'),
  // the webview posts the token after each online login so the worker can sync
  setAuthToken: (token: string, hotelId: number) =>
    invoke<void>('set_auth_token', { token, hotelId }),
  // caches the argon2 verifier for offline verification (README §11.7)
  rememberOfflineCredentials: (
    userId: string,
    hotelId: number,
    username: string,
    passwordHash: string,
    role: string,
  ) =>
    invoke<void>('remember_offline_credentials', {
      userId,
      hotelId,
      username,
      passwordHash,
      role,
    }),
  // manual sync trigger
  syncNow: () => invoke<unknown>('sync_now'),
}
