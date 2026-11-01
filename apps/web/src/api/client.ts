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

// API base. The browser build talks to its own origin (`/api/v1`, proxied to the
// Rust server by Vite in dev/preview and by nginx/Caddy in production). The
// packaged desktop app is served from `http://tauri.localhost`, so it must build
// with an absolute base: `VITE_API_BASE=http://<server>:8080/api/v1`. The value is
// baked in at build time; leave unset for the web build.
const BASE = (import.meta.env.VITE_API_BASE as string | undefined)?.replace(/\/+$/, '') || '/api/v1'
const AUTH_KEY = 'hms.session'

export interface Session {
  token: string
  user: SessionUser
  permissions: string[]
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

type ReqInit = Omit<RequestInit, 'body'> & { body?: unknown; query?: Record<string, string | number | undefined> }

async function request<T>(path: string, init?: ReqInit): Promise<T> {
  const session = loadSession()
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...((init?.headers as Record<string, string>) ?? {}),
  }
  if (session) headers['Authorization'] = `Bearer ${session.token}`
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
  if (session.user.role === 'admin') return true
  return session.permissions.includes(perm)
}

export const api = {
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
  updateHotel: (id: string | number, b: { name?: string; is_active?: boolean }) =>
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
  setCleanliness: (roomId: string, cleanliness: string, notes?: string) =>
    request<RoomDto>(`/rooms/${roomId}/cleanliness`, { method: 'PATCH', body: { cleanliness, notes } }),

  // ---------------------------------------------------------------- guests
  listGuests: (search?: string) => request<GuestDto[]>('/guests', { query: { search } }),
  createGuest: (b: { full_name: string; nationality?: string; phone?: string; email?: string; is_primary?: boolean }) =>
    request<GuestDto>('/guests', { method: 'POST', body: b }),

  // ---------------------------------------------------------------- channels/price rules
  listChannels: () => request<ChannelDto[]>('/channels'),
  listPriceRules: () => request<PriceRuleDto[]>('/price-rules'),
  createPriceRule: (b: { room_type_id: string; date_from: string; date_to: string; price_cents: number; currency?: string }) =>
    request<PriceRuleDto>('/price-rules', { method: 'POST', body: b }),

  // ---------------------------------------------------------------- reservations
  listReservations: (q?: ReservationListQuery) => request<ReservationDto[]>('/reservations', { query: q as ReqInit['query'] }),
  reservationsCount: (q?: ReservationListQuery) => request<number>('/reservations/count', { query: q as ReqInit['query'] }),
  getReservation: (id: string) => request<ReservationDto>(`/reservations/${id}`),
  createReservation: (b: CreateReservationBody) => request<ReservationDto>('/reservations', { method: 'POST', body: b }),
  editReservation: (id: string, patch: Partial<CreateReservationBody>) =>
    request<ReservationDto>(`/reservations/${id}`, { method: 'PATCH', body: patch }),
  checkIn: (id: string) => request<ReservationDto>(`/reservations/${id}/check-in`, { method: 'POST' }),
  checkOut: (id: string) => request<ReservationDto>(`/reservations/${id}/check-out`, { method: 'POST' }),
  cancelReservation: (id: string) => request<ReservationDto>(`/reservations/${id}/cancel`, { method: 'POST' }),

  // ---------------------------------------------------------------- payments/folio
  listPayments: (reservationId: string) => request<PaymentDto[]>(`/reservations/${reservationId}/payments`),
  addPayment: (reservationId: string, b: { amount_cents: number; payment_type: string; currency?: string; reference?: string }) =>
    request<PaymentDto>(`/reservations/${reservationId}/payments`, { method: 'POST', body: b }),
  reversePayment: (paymentId: string) => request<PaymentDto>(`/payments/${paymentId}/reverse`, { method: 'POST' }),
  listFolio: (reservationId: string) => request<FolioDto[]>(`/reservations/${reservationId}/folio`),
  addFolio: (reservationId: string, b: { category: string; description: string; unit_price_cents: number; quantity: number }) =>
    request<FolioDto>(`/reservations/${reservationId}/folio`, { method: 'POST', body: b }),

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
