// Desktop bridge (README §11): thin, typed wrappers over the Tauri IPC
// surface exposed by client/desktop/src-tauri (sync_status, set_auth_token,
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
  getAppPreferences: () => invoke<{ first_run: boolean; mode: 'local' | 'cloud'; api_base: string | null; storage_dir: string | null; theme: 'light' | 'dark'; sync_mode: 'auto' | 'manual' }>('get_app_preferences'),
  completeFirstRun: (mode: 'local' | 'cloud', apiBase: string | null, storageDir: string, theme: 'light' | 'dark') =>
    invoke<void>('complete_first_run', { mode, apiBase, storageDir, theme }),
  // Settings ▸ Remote server: persist + apply a new API base (no server contact)
  setApiBase: (apiBase: string) => invoke<void>('set_api_base', { apiBase }),
  // Settings ▸ Sync mode: auto (background worker) | manual (Sync now only)
  setSyncMode: (mode: 'auto' | 'manual') => invoke<void>('set_sync_mode', { mode }),
  // "Local only" first-run entry point: opens the workspace with no server
  localModeSession: () => invoke<{ username: string; role: string } | null>('local_mode_session'),
  getOfflineMode: () => invoke<boolean>('get_offline_mode'),
  setOfflineMode: (enabled: boolean) => invoke<void>('set_offline_mode', { enabled }),
  offlineLoginIdentity: () => invoke<{ hotel_id: number; username: string } | null>('offline_login_identity'),
  offlineLogin: (hotelId: number, username: string, password: string) =>
    invoke<{ offline: boolean; user_id: string; role: string }>('offline_login', { hotelId, username, password }),
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
  offlineReservations: () => invoke<Array<Record<string, unknown>>>('offline_reservations'),
  reservationGuests: (hotelId: number, reservationId: string) =>
    invoke<Array<Record<string, unknown>>>('reservation_guests', { hotelId, reservationId }),
  reservationGuestIds: (reservationId: string) =>
    invoke<Array<string>>('reservation_guest_ids', { reservationId }),
  // manual sync trigger
  syncNow: () => invoke<unknown>('sync_now'),

  // G4: LAN hub configuration
  setHubConfig: (hubUrl: string | null, hubToken: string | null) =>
    invoke<void>('set_hub_config', { hubUrl, hubToken }),
  getHubConfig: () =>
    invoke<{ hub_url: string | null; has_token: boolean }>('get_hub_config'),

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

  // ------------------------------------------------------------- bulk import
  /** Settings ▸ Bulk import: load legacy data files into the local database.
   * Returns summary with inserted/updated/skipped/error counts. */
  bulkImport: (kind: 'guests' | 'collections' | 'reservations' | 'users', path: string, anchor?: string) =>
    invoke<{ dataset: string; inserted: number; updated: number; skipped: number; error_count: number; errors: string[] }>(
      'bulk_import',
      { kind, path, anchor }
    ),
  /** List locally provisioned users (always available offline). */
  /** Persist/read desktop hotel profiles and shift handovers in encrypted SQLite. */
  listLocalHotels: () => invoke<Array<Record<string, unknown>>>('list_local_hotels'),
  listDomainRecords: (entity: string, hotelId: number) =>
    invoke<Array<Record<string, unknown>>>('list_domain_records', { entity, hotelId }),
  saveDomainRecord: (entity: string, hotelId: number, id: string, value: unknown) =>
    invoke<void>('save_domain_record', { entity, hotelId, id, value }),
  deleteDomainRecord: (entity: string, hotelId: number, id: string) =>
    invoke<void>('delete_domain_record', { entity, hotelId, id }),
  restoreDomainRecord: (entity: string, hotelId: number, id: string) =>
    invoke<void>('restore_domain_record', { entity, hotelId, id }),
  saveLocalHotel: (oldId: number, id: number, code: string, name: string, profile: unknown) =>
    invoke<void>('save_local_hotel', { oldId, id, code, name, profile }),
  saveHotelProfile: (hotelId: number, profile: unknown) =>
    invoke<void>('save_hotel_profile', { hotelId, profile }),
  getHotelProfile: (hotelId: number) =>
    invoke<Record<string, unknown> | null>('get_hotel_profile', { hotelId }),
  createShiftTransfer: (transfer: {
    id: string; hotelId: number; fromUser: string; toUser: string;
    amountCents: number; currency: string; notes?: string;
  }) => invoke<void>('create_shift_transfer', {
    id: transfer.id, hotelId: transfer.hotelId, fromUser: transfer.fromUser,
    toUser: transfer.toUser, amountCents: transfer.amountCents,
    currency: transfer.currency, notes: transfer.notes ?? null,
  }),
  pendingShiftTransfer: (hotelId: number, toUser: string) =>
    invoke<{ id: string; from_user: string; to_user: string; amount_cents: number; currency: string; notes?: string; created_ms: number } | null>(
      'pending_shift_transfer', { hotelId, toUser },
    ),
  decideShiftTransfer: (id: string, decidedBy: string, approved: boolean) =>
    invoke<void>('decide_shift_transfer', { id, decidedBy, approved }),
  updateLocalUser: (userId: string, body: { password: string }) =>
    invoke<void>('update_local_user', { userId, password: body.password }),
  listLocalUsers: () =>
    invoke<Array<{ user_id: string; username: string; full_name: string | null; role: string; hotel_id: number }>>(
      'list_local_users'
    ),
  /** Open a native file picker dialog and return the selected path. */
  pickFile: (fileTypes: Array<[string, string[]]>) =>
    invoke<string | null>('pick_file', { fileTypes }),
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
