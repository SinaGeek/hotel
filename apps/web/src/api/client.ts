// API client. Tomorrow these types come from src/types/generated (ts-rs / OpenAPI),
// until then hand-written to match hms-core::api_types.
import type { ApiResponse, LoginRequest } from '../types/generated/api'

const BASE = '/api/v1'

const AUTH_KEY = 'hms.session'

export interface SessionUser {
  userId: string
  hotelId: number
  username: string
  fullName: string
  role: string
}

export function loadSession(): SessionUser | null {
  const raw = localStorage.getItem(AUTH_KEY)
  return raw ? (JSON.parse(raw) as SessionUser) : null
}

export function saveSession(user: SessionUser) {
  localStorage.setItem(AUTH_KEY, JSON.stringify(user))
}

export function clearSession() {
  localStorage.removeItem(AUTH_KEY)
}

export class ApiError extends Error {
  constructor(message: string, public status: number) {
    super(message)
  }
}

type ReqInit = Omit<RequestInit, 'body'> & { body?: unknown; headers?: HeadersInit }

async function request<T>(path: string, init?: ReqInit): Promise<T> {
  const { body, ...rest } = init ?? {}
  const res = await fetch(BASE + path, {
    ...rest,
    headers: { 'Content-Type': 'application/json', ...(rest.headers ?? {}) },
    body: body != null ? JSON.stringify(body) : undefined,
  })
  const payload: ApiResponse<T> = await res.json().catch(() => ({ success: false, data: null as T, error: `HTTP ${res.status}` }))
  if (!res.ok || !payload.success) {
    throw new ApiError(payload.error ?? `HTTP ${res.status}`, res.status)
  }
  return payload.data
}

export const api = {
  login: (hotelId: number, username: string, password: string) =>
    request<SessionUser>('/auth/login', { method: 'POST', body: { hotel_id: hotelId, username, password } satisfies LoginRequest }),
}
