// API client for the hms-server REST API (README §12). Token persisted in
// localStorage; types mirror the server DTOs in src/types/generated/api.ts
// (hand-written until ts-rs/OpenAPI generation, see todo.md).
import type {
  ApiResponse,
  BoardResponse,
  ChannelDto,
  ChannelRevenue,
  CreateReservationBody,
  DayReport,
  FolioDto,
  GuestDto,
  HotelDto,
  LoginResponse,
  PaymentDto,
  PriceRuleDto,
  ReservationDto,
  ReservationListQuery,
  RoleMatrix,
  RoomDto,
  RoomRevenue,
  RoomTypeDto,
  SessionUser,
  ForecastRow,
  InhouseReport,
  MainCouranteReport,
  SummaryReport,
  UserDto,
} from '../types/generated/api'
import { desktop, isDesktop } from './desktop'

// API base. The browser build talks to its own origin (`/api/v1`, proxied to the
// Rust server by Vite in dev/preview and by nginx/Caddy in production). The
// packaged desktop app is served from `http://tauri.localhost`, so it must build
// with an absolute base: `VITE_API_BASE=http://<server>:8080/api/v1`. The value is
// baked in at build time; leave unset for the web build.
const BASE = (import.meta.env.VITE_API_BASE as string | undefined)?.replace(/\/+$/, '') ||
  (isDesktop ? 'http://localhost:8080/api/v1' : '/api/v1')

const AUTH_KEY = 'hms.session'

export interface Session {
  token: string
  user: SessionUser
  permissions: string[]
  offline?: boolean
}

export function loadSession(): Session | null {
  const raw = localStorage.getItem(AUTH_KEY)
  return raw ? (JSON.parse(raw) as Session) : null
}

export function saveSession(s: Session) {
  localStorage.setItem(AUTH_KEY, JSON.stringify(s))
}

export function clearSession() {
  localStorage.removeItem(AUTH_KEY)
}

export class ApiError extends Error {
  constructor(message: string, public status: number) {
    super(message)
  }
}

const effectiveOfflineHotelId = (session: Session): number => {
  const activeHotel = localStorage.getItem('hms.active_hotel')
  return (session.user.role === 'admin' && activeHotel) ? Number(activeHotel) : session.user.hotel_id
}

type ReqInit = Omit<RequestInit, 'body'> & { body?: unknown; query?: Record<string, string | number | undefined> }

type LocalRow = Record<string, unknown>
const asNum = (v: unknown, d = 0) => (typeof v === 'number' ? v : Number(v) || d)
const asStr = (v: unknown, d = '') => (v == null ? d : String(v))
const DAY_MS = 86_400_000

/**
 * Offline board: builds the BoardResponse from the encrypted local SQLite
 * domain tables. Rooms come grouped under their room types (the board groups
 * by room_type_id), stays come from local reservations, per-night prices are
 * resolved through the layered fee model (calendar override > hotel weekend
 * days > room type mid fee).
 */
async function loadOfflineBoard(from: string, to: string, hotelId: number): Promise<BoardResponse> {
  const [roomRows, typeRows, resRows, ruleRows, guestRows, payRows] = await Promise.all([
    desktop.listDomainRecords('rooms', hotelId),
    desktop.listDomainRecords('room_types', hotelId),
    desktop.listDomainRecords('reservations', hotelId),
    desktop.listDomainRecords('price_rules', hotelId).catch(() => [] as LocalRow[]),
    desktop.listDomainRecords('guests', hotelId).catch(() => [] as LocalRow[]),
    desktop.listDomainRecords('payments', hotelId).catch(() => [] as LocalRow[]),
  ])
  // soft-deleted rooms are trashed, not active inventory
  const activeRoomRows = (roomRows as LocalRow[]).filter((r) => r.deleted_at == null)
  // per-stay balance: stay total minus recorded payments (reversals subtract)
  const paid = new Map<string, number>()
  for (const p of payRows as LocalRow[]) {
    const rid = String(p.reservation_id)
    const amt = asNum(p.amount_cents)
    paid.set(rid, (paid.get(rid) ?? 0) + (asNum(p.is_reversal) === 1 ? -amt : amt))
  }
  const typeById = new Map(typeRows.map((t) => [String(t.id), t]))
  const guestName = new Map(guestRows.map((g) => [String(g.id), String(g.full_name ?? '')]))
  const profile = await desktop.getHotelProfile(hotelId).catch(() => null as unknown)
  const weekendDays: number[] = Array.isArray((profile as LocalRow)?.weekendDays)
    ? ((profile as LocalRow).weekendDays as number[])
    : [6, 0]

  // Layered nightly price for a room type on a given date.
  const nightPrice = (t: LocalRow, date: string): number => {
    const rule = (ruleRows as LocalRow[]).find((r) =>
      String(r.room_type_id) === String(t.id) &&
      asStr(r.date_from, '9999') <= date && date <= asStr(r.date_to))
    if (rule) return asNum(rule.price_cents)
    if (weekendDays.includes(new Date(date + 'T00:00:00Z').getUTCDay())) {
      const weekend = asNum(t.weekend_price_cents)
      if (weekend > 0) return weekend
    }
    return asNum(t.base_price_cents)
  }

  const ACTIVE = new Set(['confirmed', 'checked_in'])
  const rooms: BoardResponse['rooms'] = activeRoomRows.map((r) => {
    const t = typeById.get(String(r.room_type_id))
    return {
      id: String(r.id),
      room_number: asStr(r.room_number),
      room_type_id: String(r.room_type_id ?? ''),
      room_type_name: t ? asStr(t.name) : '',
      cleanliness: asStr(r.cleanliness, 'clean'),
      floor: asNum(r.floor),
      seasonal_price_cents: t ? nightPrice(t, from) : 0,
      stays: (resRows as LocalRow[])
        .filter((s) =>
          String(s.room_id) === String(r.id) && ACTIVE.has(asStr(s.status)) &&
          asStr(s.checkout_date) > from && asStr(s.checkin_date) < to)
        .map((s) => ({
          reservation_id: String(s.id),
          checkin_date: asStr(s.checkin_date),
          checkout_date: asStr(s.checkout_date),
          status: asStr(s.status),
          guest_name: s.guest_id ? guestName.get(String(s.guest_id)) ?? null : null,
          total_price_cents: asNum(s.total_price_cents),
          currency: asStr(s.currency, 'TRY'),
          balance_cents: asNum(s.total_price_cents) - (paid.get(String(s.id)) ?? 0),
        })),
    }
  })

  const days: string[] = []
  const start = new Date(from + 'T00:00:00Z').getTime()
  const end = new Date(to + 'T00:00:00Z').getTime()
  for (let d = start; d < end; d += DAY_MS) days.push(new Date(d).toISOString().slice(0, 10))

  const occupancy: BoardResponse['occupancy'] = days.map((date) => {
    let occupied = 0
    for (const r of rooms) {
      if (r.stays.some((s) => s.checkin_date <= date && date < s.checkout_date)) occupied++
    }
    const prices = typeRows.map((t) => nightPrice(t as LocalRow, date))
    return {
      date,
      available: Math.max(0, rooms.length - occupied),
      occupied,
      from_price_cents: prices.length ? Math.min(...prices) : null,
    }
  })

  return { from, to, rooms, occupancy }
}

async function request<T>(path: string, init?: ReqInit): Promise<T> {
  const session = loadSession()
  if (isDesktop && session?.offline) {
    // Admin's header switcher (localStorage hms.active_hotel) also applies to
    // the offline session: hotel 0 has no rooms, switched context reads 4442 etc.
    const hid = effectiveOfflineHotelId(session)
    if (path.startsWith('/reservations/') && init?.method === undefined) {
      // detail + subresources (payments / folio / guests) in one offline surface
      const parts = path.split('/')
      const id = parts[2]
      const sub = parts[3]
      if (sub === 'payments') {
        const rows = (await desktop.listDomainRecords('payments', hid)) as LocalRow[]
        return rows.filter((p) => String(p.reservation_id) === id) as T
      }
      if (sub === 'folio') {
        const rows = (await desktop.listDomainRecords('folio', hid)) as LocalRow[]
        return rows.filter((f) => String(f.reservation_id) === id) as T
      }
      if (sub === 'guests') {
        return await desktop.reservationGuests(hid, id) as T
      }
      const local = await desktop.offlineReservations()
      const row = local.find((r) => String(r.id) === id)
      if (row) return row as T
      return {} as T
    }
    if (path === '/reservations' || path.startsWith('/reservations?')) {
      let rows = (await desktop.listDomainRecords('reservations', hid)) as LocalRow[]
      const q = (init?.query ?? {}) as Record<string, string | number | undefined>
      if (q.status != null && q.status !== '') rows = rows.filter((r) => asStr(r.status) === String(q.status))
      if (q.from != null || q.to != null) {
        // stay-overlap window, matching server semantics: checkout > from and checkin < to
        rows = rows.filter((r) =>
          (q.to == null || asStr(r.checkin_date) < String(q.to)) &&
          (q.from == null || asStr(r.checkout_date) > String(q.from)))
      }
      if (q.limit != null) rows = rows.slice(0, Number(q.limit))
      return rows as T
    }
    if (path.startsWith('/reservations/count')) {
      const rows = (await desktop.listDomainRecords('reservations', hid)) as LocalRow[]
      return rows.length as T
    }
    if (path.startsWith('/board')) {
      const q = (init?.query ?? {}) as Record<string, string | number | undefined>
      const today = new Date().toISOString().slice(0, 10)
      const bFrom = String(q.from ?? today)
      const bTo = String(q.to ?? new Date(new Date(bFrom + 'T00:00:00Z').getTime() + DAY_MS).toISOString().slice(0, 10))
      return await loadOfflineBoard(bFrom, bTo, hid) as T
    }
    if (path === '/hotels' || path.startsWith('/hotels/')) {
      const localHotels = await desktop.listLocalHotels().catch(() => [])
      if (localHotels.length) return localHotels as T
      return [
        { id: 0, name: 'Platform (all hotels)', is_active: true },
        { id: 4442, name: 'Hotel 4442', is_active: true },
      ] as T
    }
    if (path === '/users' || path.startsWith('/users/')) {
      const localUsers = await desktop.listLocalUsers().catch(() => [])
      return localUsers.map((u) => ({
        id: u.user_id,
        hotel_id: u.hotel_id,
        username: u.username,
        full_name: u.full_name || u.username,
        email: null,
        role: u.role,
        is_active: true,
        force_password_change: false,
      })) as T
    }
    if (path === '/rooms' || path.startsWith('/rooms/')) {
      const rows = await desktop.listDomainRecords('rooms', hid) as LocalRow[]
      return rows.filter((r) => r.deleted_at == null) as T
    }
    if (path === '/room-types' || path.startsWith('/room-types/')) {
      const rows = await desktop.listDomainRecords('room_types', hid) as LocalRow[]
      return rows.map((t) => ({ amenities: [], ...t })) as T
    }
    if (path === '/guests' || path.startsWith('/guests/')) {
      const rows = await desktop.listDomainRecords('guests', hid) as LocalRow[]
      return rows.map((g) => ({ is_primary: false, kbs_reported: false, ...g })) as T
    }
    if (path === '/channels' || path.startsWith('/channels/')) {
      return await desktop.listDomainRecords('channels', hid) as T
    }
    if (path === '/restaurants' || path.startsWith('/restaurants/')) {
      return await desktop.listDomainRecords('restaurants', hid) as T
    }
    if (path === '/price-rules' || path.startsWith('/price-rules/')) {
      return await desktop.listDomainRecords('price_rules', hid) as T
    }
    if (path.startsWith('/roles/')) {
      return { role: path.split('/')[2] || 'reception', permissions: [] } as T
    }
    if (path.startsWith('/reports')) {
      return [] as T
    }
    if (init?.method === undefined || init.method === 'GET') {
      return [] as T
    }
    return {} as T
  }
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...((init?.headers as Record<string, string>) ?? {}),
  }
  if (session) headers['Authorization'] = `Bearer ${session.token}`
  const activeHotel = localStorage.getItem('hms.active_hotel')
  if (activeHotel && session?.user.role === 'admin') {
    headers['x-hotel-id'] = activeHotel
  }
  const qs = init?.query
    ? '?' + Object.entries(init.query).filter(([, v]) => v != null).map(([k, v]) => `${k}=${encodeURIComponent(String(v))}`).join('&')
    : ''
  const res = await fetch(BASE + path + qs, {
    ...init,
    headers,
    body: init?.body != null ? JSON.stringify(init.body) : undefined,
  })
  if (res.status === 401) {
    // expired/invalid session — drop it and force re-login (README §8)
    clearSession()
  }
  const payload: ApiResponse<T> = await res.json().catch(() => ({ success: false, data: null as T, error: `HTTP ${res.status}` }))
  if (!res.ok || !payload.success) {
    throw new ApiError(payload.error ?? `HTTP ${res.status}`, res.status)
  }
  return payload.data
}

export function sessionHasPermission(session: Session | null, perm: string): boolean {
  if (!session) return false
  const roles = session.user.role.split(',').map((r) => r.trim().toLowerCase())
  if (roles.includes('admin') || session.user.hotel_id === 0) return true
  return session.permissions.includes(perm)
}

// ------------------------------------------------ desktop local-first writes
// (todo A1): inside the Tauri webview every write goes through the native
// shell. The command stages the mirror row + outbox op in ONE SQLite
// transaction and, when a session exists, runs one push+pull cycle so the
// server's canonical state replaces the optimistic values. Queued (offline)
// writes return the optimistic DTO; canonical values arrive with the next
// pull and a re-fetch.
async function localWrite<T>(
  entity: string,
  operation: string,
  payload: unknown,
  entityId: string | null,
  canonical?: (id: string) => Promise<T>,
): Promise<T> {
  const outcome = await desktop.localWrite(entity, operation, entityId, payload)
  if (outcome.status === 'confirmed' && canonical) {
    try {
      return await canonical(outcome.entity_id)
    } catch {
      // push confirmed but the canonical refetch failed — optimistic fallthrough
    }
  }
  return { ...(payload as Record<string, unknown>), id: outcome.entity_id } as T
}

/** First matching element of a list, else an opt-time stub when the mirror is
 * ahead of the server list call (queued writes). */
async function firstOrStub<T extends { id: string }>(p: Promise<T[]>, id: string, stub: Record<string, unknown>): Promise<T> {
  const rows = await p
  return rows.find((r) => r.id === id) ?? ({ ...stub, id } as unknown as T)
}

export const api = {
  // OTA channel configuration endpoints. These manage settings only; live
  // provider operations remain disabled until certified client adapters exist.
  otaConfigs: () => request<unknown[]>('/ota-configs'),
  createOtaConfig: (body: unknown) => request<unknown>('/ota-configs', { method: 'POST', body }),
  updateOtaConfig: (id: string, body: unknown) => request<unknown>(`/ota-configs/${id}`, { method: 'PUT', body }),
  deleteOtaConfig: (id: string) => request<null>(`/ota-configs/${id}`, { method: 'DELETE' }),

  // ---------------------------------------------------------------- auth
  // `offline` is set by the desktop webview (README §11.7) so the response
  // carries the argon2 verifier for the native shell's offline login cache.
  login: (hotelId: number, username: string, password: string, offline = false) =>
    request<LoginResponse>('/auth/login', { method: 'POST', body: { hotel_id: hotelId, username, password, ...(offline ? { offline: true } : {}) } }),
  changePassword: (oldPassword: string, newPassword: string) =>
    request<null>('/auth/change-password', { method: 'POST', body: { old_password: oldPassword, new_password: newPassword } }),
  logout: () => request<null>('/auth/logout', { method: 'POST' }),

  // ---------------------------------------------------------------- hotels
  listHotels: () => request<HotelDto[]>('/hotels'),
  createHotel: (b: { id: number; name: string; default_currency?: string; time_zone?: string }) =>
    request<HotelDto>('/hotels', { method: 'POST', body: b }),
  updateHotel: (id: string | number, b: { id?: number; name?: string; is_active?: boolean }) =>
    request<HotelDto>(`/hotels/${id}`, { method: 'PATCH', body: b }),

  // ---------------------------------------------------------------- users/roles
  listUsers: () => request<UserDto[]>('/users'),
  createUser: (b: {
    hotel_id?: number
    username: string
    full_name?: string
    email?: string
    role: string
    password: string
  }) => request<UserDto>('/users', { method: 'POST', body: b }),
  updateUser: (
    id: string,
    b: { full_name?: string; email?: string; role?: string; is_active?: boolean; password?: string },
  ) => request<UserDto>(`/users/${id}`, { method: 'PATCH', body: b }),
  roleMatrix: (role: string) => request<RoleMatrix>(`/roles/${role}/permissions`),

  // ---------------------------------------------------------------- rooms
  listRoomTypes: () => request<RoomTypeDto[]>('/room-types'),
  listRooms: () => request<RoomDto[]>('/rooms'),
  saveDomainRecord: (entity: 'rooms' | 'room_types' | 'guests' | 'reservations' | 'channels' | 'restaurants' | 'price_rules', id: string, value: Record<string, unknown>) => {
    if (isDesktop) {
      const session = loadSession()
      if (!session) return Promise.reject(new ApiError('Sign in required', 401))
      return desktop.saveDomainRecord(entity, effectiveOfflineHotelId(session), id, value).then(() => value)
    }
    return request<Record<string, unknown>>(`/${entity.replace('_', '-')}/${id}`, { method: 'PATCH', body: value })
  },
  deleteDomainRecord: (entity: 'rooms' | 'room_types' | 'guests' | 'reservations' | 'channels' | 'restaurants' | 'price_rules', id: string) => {
    if (isDesktop) {
      const session = loadSession()
      if (!session) return Promise.reject(new ApiError('Sign in required', 401))
      return desktop.deleteDomainRecord(entity, effectiveOfflineHotelId(session), id)
    }
    return request<null>(`/${entity.replace('_', '-')}/${id}`, { method: 'DELETE' })
  },
  setCleanliness: (roomId: string, cleanliness: string, notes?: string) => {
    if (!isDesktop) return request<RoomDto>(`/rooms/${roomId}/cleanliness`, { method: 'PATCH', body: { cleanliness, notes } })
    // local-first (todo A1): mirror op + optimistic row; refetch canonical list
    return localWrite<RoomDto>('cleanliness', 'update', { cleanliness }, roomId, (id) =>
      firstOrStub(request<RoomDto[]>('/rooms'), id, { room_number: '', cleanliness }),
    )
  },

  // ---------------------------------------------------------------- guests
  listGuests: (search?: string) => request<GuestDto[]>('/guests', { query: { search } }),
  createGuest: (b: { full_name: string; nationality?: string; phone?: string; email?: string; is_primary?: boolean }) => {
    if (!isDesktop) return request<GuestDto>('/guests', { method: 'POST', body: b })
    // local-first (todo A1): the shell adopts the client UUID server-side
    return localWrite<GuestDto>('guest', 'insert', { is_primary: true, ...b }, null)
  },

  // ---------------------------------------------------------------- channels/price rules
  listChannels: () => request<ChannelDto[]>('/channels'),
  listRestaurants: () => request<Array<Record<string, unknown>>>('/restaurants'),
  listPriceRules: () => request<PriceRuleDto[]>('/price-rules'),
  createPriceRule: (b: { room_type_id: string; date_from: string; date_to: string; price_cents: number; currency?: string }) =>
    request<PriceRuleDto>('/price-rules', { method: 'POST', body: b }),

  // ---------------------------------------------------------------- reservations
  listReservations: (q?: ReservationListQuery) => request<ReservationDto[]>('/reservations', { query: q as ReqInit['query'] }),
  reservationsCount: (q?: ReservationListQuery) => request<number>('/reservations/count', { query: q as ReqInit['query'] }),
  getReservation: (id: string) => request<ReservationDto>(`/reservations/${id}`),
  listReservationGuests: (id: string) => request<Array<Record<string, unknown>>>(`/reservations/${id}/guests`),
  suggestedRooms: (id: string) => request<RoomDto[]>(`/reservations/${id}/suggested-rooms`),
  createReservation: (b: CreateReservationBody) => {
    if (!isDesktop) return request<ReservationDto>('/reservations', { method: 'POST', body: b })
    // local-first (todo A1): staged offline-capable create; canonical values
    // (number, status incl. `conflict`, priced total) arrive with the pull
    return localWrite<ReservationDto>('reservation', 'insert', b, null, (id) =>
      request<ReservationDto>(`/reservations/${id}`),
    )
  },
  editReservation: (id: string, patch: Partial<CreateReservationBody>) => {
    if (!isDesktop) return request<ReservationDto>(`/reservations/${id}`, { method: 'PATCH', body: patch })
    return localWrite<ReservationDto>('reservation', 'update', patch, id, () =>
      request<ReservationDto>(`/reservations/${id}`),
    )
  },
  checkIn: (id: string) => {
    if (!isDesktop) return request<ReservationDto>(`/reservations/${id}/check-in`, { method: 'POST' })
    return localWrite<ReservationDto>('reservation', 'update', { status: 'checked_in' }, id, () =>
      request<ReservationDto>(`/reservations/${id}`),
    )
  },
  checkOut: (id: string) => {
    if (!isDesktop) return request<ReservationDto>(`/reservations/${id}/check-out`, { method: 'POST' })
    return localWrite<ReservationDto>('reservation', 'update', { status: 'checked_out' }, id, () =>
      request<ReservationDto>(`/reservations/${id}`),
    )
  },
  cancelReservation: (id: string) => {
    if (!isDesktop) return request<ReservationDto>(`/reservations/${id}/cancel`, { method: 'POST' })
    return localWrite<ReservationDto>('reservation', 'update', { status: 'cancelled' }, id, () =>
      request<ReservationDto>(`/reservations/${id}`),
    )
  },

  // ---------------------------------------------------------------- payments/folio
  listPayments: (reservationId: string) => request<PaymentDto[]>(`/reservations/${reservationId}/payments`),
  addPayment: (reservationId: string, b: { amount_cents: number; payment_type: string; currency?: string; reference?: string }) => {
    if (!isDesktop) return request<PaymentDto>(`/reservations/${reservationId}/payments`, { method: 'POST', body: b })
    // append-only local write (todo A1): reversals are negative amounts
    return localWrite<PaymentDto>('payment', 'insert', { ...b, reservation_id: reservationId }, null, (id) =>
      firstOrStub(api.listPayments(reservationId), id, { ...b, reservation_id: reservationId, currency: b.currency ?? 'TRY', is_reversal: b.amount_cents < 0, created_at: '', reference: b.reference ?? null } as Record<string, unknown>),
    )
  },
  reversePayment: (paymentId: string) => {
    if (!isDesktop) return request<PaymentDto>(`/payments/${paymentId}/reverse`, { method: 'POST' })
    // offline reversal (README §7): read the original from the mirror, then
    // stage a NEGATIVE insert — payments are append-only, never edited
    return (async () => {
      const original = await desktop.paymentLookup(paymentId)
      const body = {
        reservation_id: original.reservation_id,
        amount_cents: -original.amount_cents,
        currency: original.currency,
        payment_type: 'refund',
        reference: null as string | null,
      }
      return localWrite<PaymentDto>('payment', 'insert', body, null, (id) =>
        firstOrStub(api.listPayments(original.reservation_id), id, { ...body, is_reversal: true, created_at: '' } as Record<string, unknown>),
      )
    })()
  },
  listFolio: (reservationId: string) => request<FolioDto[]>(`/reservations/${reservationId}/folio`),
  addFolio: (reservationId: string, b: { category: string; description: string; unit_price_cents: number; quantity: number }) => {
    if (!isDesktop) return request<FolioDto>(`/reservations/${reservationId}/folio`, { method: 'POST', body: b })
    return localWrite<FolioDto>('folio', 'insert', { ...b, reservation_id: reservationId }, null, (id) =>
      firstOrStub(api.listFolio(reservationId), id, { ...b, reservation_id: reservationId, total_cents: b.unit_price_cents * b.quantity, currency: 'TRY', created_at: '' } as Record<string, unknown>),
    )
  },

  // ---------------------------------------------------------------- board
  board: (from: string, to: string) => request<BoardResponse>('/board', { query: { from, to } }),

  // ---------------------------------------------------------------- reports
  reportSummary: (from: string, to: string) => request<SummaryReport>('/reports/summary', { query: { from, to } }),
  reportsByChannel: (from: string, to: string) => request<ChannelRevenue[]>('/reports/by-channel', { query: { from, to } }),
  reportsByDay: (from: string, to: string) => request<DayReport[]>('/reports/by-day', { query: { from, to } }),
  reportsByRoom: (from: string, to: string) => request<RoomRevenue[]>('/reports/by-room', { query: { from, to } }),
  // Daily set (5b.6): per-date occupancy forecast and per-date stays/balances
  reportForecast: (start: string, days?: number) =>
    request<ForecastRow[]>('/reports/forecast', { query: days == null ? { start } : { start, days } }),
  reportInhouse: (date: string) => request<InhouseReport>('/reports/inhouse', { query: { date } }),
  reportMaincourante: (date: string) => request<MainCouranteReport>('/reports/maincourante', { query: { date } }),
}
