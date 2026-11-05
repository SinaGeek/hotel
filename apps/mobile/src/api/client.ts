// G5: mobile API client.
//
// Reuses the SAME wire types as the web app (`apps/web/src/types/generated/api.ts`
// via the `@hms/shared-types` path alias) and the SAME REST contract
// (`/api/v1/...`, README §12). Only the token *storage* differs: the mobile
// build keeps the session in AsyncStorage (Expo) instead of `localStorage`.
import AsyncStorage from '@react-native-async-storage/async-storage'
import type {
  ApiResponse,
  BoardResponse,
  GuestDto,
  LoginResponse,
  PaymentDto,
  ReservationDto,
  RoomDto,
  RoomTypeDto,
  SessionUser,
  SummaryReport,
} from '@hms/shared-types'

const AUTH_KEY = 'hms.session'

// Configured at build time: EXPO_PUBLIC_API_BASE=https://hms.example.com/api/v1
const BASE = (process.env.EXPO_PUBLIC_API_BASE ?? 'http://10.0.2.2:8080/api/v1').replace(/\/+$/, '')

export interface Session {
  token: string
  user: SessionUser
  permissions: string[]
}

let cached: Session | null = null

export async function loadSession(): Promise<Session | null> {
  if (cached) return cached
  const raw = await AsyncStorage.getItem(AUTH_KEY)
  cached = raw ? (JSON.parse(raw) as Session) : null
  return cached
}

export async function saveSession(s: Session): Promise<void> {
  cached = s
  await AsyncStorage.setItem(AUTH_KEY, JSON.stringify(s))
}

export async function clearSession(): Promise<void> {
  cached = null
  await AsyncStorage.removeItem(AUTH_KEY)
}

export class ApiError extends Error {
  constructor(
    message: string,
    public status: number,
  ) {
    super(message)
  }
}

export function hasPermission(session: Session | null, perm: string): boolean {
  if (!session) return false
  if (session.user.role === 'admin') return true
  return session.permissions.includes(perm)
}

interface ReqInit {
  method?: string
  body?: unknown
  query?: Record<string, string | number | undefined>
}

async function request<T>(path: string, init?: ReqInit): Promise<T> {
  const session = await loadSession()
  const headers: Record<string, string> = { 'Content-Type': 'application/json' }
  if (session) headers.Authorization = `Bearer ${session.token}`
  const qs = init?.query
    ? '?' +
      Object.entries(init.query)
        .filter(([, v]) => v != null)
        .map(([k, v]) => `${k}=${encodeURIComponent(String(v))}`)
        .join('&')
    : ''
  const res = await fetch(BASE + path + qs, {
    method: init?.method ?? 'GET',
    headers,
    body: init?.body != null ? JSON.stringify(init.body) : undefined,
  })
  if (res.status === 401) await clearSession()
  const payload: ApiResponse<T> = await res
    .json()
    .catch(() => ({ success: false, data: null as T, error: `HTTP ${res.status}` }))
  if (!res.ok || !payload.success) throw new ApiError(payload.error ?? `HTTP ${res.status}`, res.status)
  return payload.data
}

export const api = {
  baseUrl: BASE,
  login: (hotelId: number, username: string, password: string) =>
    request<LoginResponse>('/auth/login', { method: 'POST', body: { hotel_id: hotelId, username, password } }),
  logout: () => request<null>('/auth/logout', { method: 'POST' }),

  listRoomTypes: () => request<RoomTypeDto[]>('/room-types'),
  listRooms: () => request<RoomDto[]>('/rooms'),
  setCleanliness: (roomId: string, cleanliness: string, notes?: string) =>
    request<RoomDto>(`/rooms/${roomId}/cleanliness`, { method: 'PATCH', body: { cleanliness, notes } }),

  listGuests: (search?: string) => request<GuestDto[]>('/guests', { query: { search } }),
  board: (from: string, to: string) => request<BoardResponse>('/board', { query: { from, to } }),
  reportSummary: (from: string, to: string) => request<SummaryReport>('/reports/summary', { query: { from, to } }),
  listReservations: (q?: { from?: string; to?: string; status?: string; limit?: number }) =>
    request<ReservationDto[]>('/reservations', { query: q }),
  getReservation: (id: string) => request<ReservationDto>(`/reservations/${id}`),
  checkIn: (id: string) => request<ReservationDto>(`/reservations/${id}/check-in`, { method: 'POST' }),
  checkOut: (id: string) => request<ReservationDto>(`/reservations/${id}/check-out`, { method: 'POST' }),
  listPayments: (reservationId: string) => request<PaymentDto[]>(`/reservations/${reservationId}/payments`),
}
