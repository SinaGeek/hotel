// Desktop bridge (README §11): thin, typed wrappers over the Tauri IPC
// surface exposed by apps/desktop/src-tauri (sync_status, set_auth_token,
// remember_offline_credentials, offline_login, offline_reports, sync_now). Everything is a
// no-op throwing a clear Error in a plain browser, so the same web build
// serves both the REST web app and the offline desktop shell.

export interface SyncSnapshot {
  state: 'Synced' | 'Failing' | 'Locked' | 'Unknown' | 'UpdateRequired'
  /** present for Synced { synced_at_ms, pending, conflicts } and Failing { reason, pending } */
  detail?:
    | { synced_at_ms: number; pending: number; conflicts: number }
    | { reason: string; pending: number }
}

/** D3/D4: what `update_check` reports; `mandatory` blocks syncing. */
export interface UpdateInfo {
  available: boolean
  current_version: string
  version: string | null
  notes: string | null
  mandatory: boolean
  raw_json?: unknown
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

export interface OfflineReports {
  summary: {
    from: string
    to: string
    room_revenue_cents: number
    extra_revenue_cents: number
    payments_collected_cents: number
    occupied_nights: number
    available_room_nights: number
    occupancy_pct: number
    adr_cents: number | null
    revpar_cents: number | null
    currency?: string
  }
  by_day: Array<{
    date: string
    room_revenue_cents: number
    extra_revenue_cents: number
    payments_collected_cents: number
    occupied_nights: number
  }>
  by_channel: Array<{
    channel_id: string
    channel_name: string
    kind: string
    room_revenue_cents: number
    extra_revenue_cents: number
    payments_collected_cents: number
    occupied_nights: number
  }>
  by_room: Array<{
    room_id: string
    room_number: string
    room_type_name: string
    room_revenue_cents: number
    extra_revenue_cents: number
    occupied_nights: number
  }>
}

export const desktop = {
  // status pill source of truth (background worker keeps it fresh)
  syncStatus: () => invoke<SyncSnapshot>('sync_status'),
  setReportCurrency: (currency: string) =>
    invoke<void>('set_report_currency', { currency }),
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
  offlineReports: (from: string, to: string) =>
    invoke<OfflineReports>('offline_reports', { from, to }),
  // manual sync trigger
  syncNow: () => invoke<unknown>('sync_now'),

  // ------------------------------------------------ local-first writes (A1)
  /** STAGE locally (mirror + outbox in one transaction) then push+pull
   * immediately when online. `entity_id: null` on creates (the shell
   * generates a UUIDv7 and adopts it server-side). */
  localWrite: (
    entity: string,
    operation: string,
    entityId: string | null,
    payload: unknown,
  ) => invoke<WriteOutcome>('local_write', { entity, operation, entityId, payload }),
  /** Original payment from the mirror — used to build a reversal row
   * offline; the mirror must already hold the payment (pulled by sync). */
  paymentLookup: (paymentId: string) =>
    invoke<PaymentMirror>('local_payment_lookup', { paymentId }),

  // ------------------------------------------------------------- updater
  /** Ask the update endpoint whether a newer build exists (no download). */
  updateCheck: () => invoke<UpdateInfo>('update_check'),
  /** Download, verify and install the pending release. A final sync runs
   * first; its failure aborts the install so no write is stranded. */
  updateInstall: () => invoke<string>('update_install'),
  /** True while the server refuses this build (426) — syncing is blocked. */
  updateRequired: () => invoke<boolean>('update_required'),
}

export interface WriteOutcome {
  status: 'confirmed' | 'queued'
  operation_id: string
  entity_id: string
  /** present on queued: why the write is waiting for the next sync cycle */
  reason?: string
  /** A4: local availability warning (offline "room looks taken" estimate);
   * a warning never blocks — the server resolves overlaps/exclusions later. */
  warning?: string
}

export interface PaymentMirror {
  reservation_id: string
  amount_cents: number
  currency: string
}
