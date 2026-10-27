// Wire types mirroring crates/hms-server DTOs exactly.
// HAND-WRITTEN for now: replace by ts-rs/OpenAPI generation in Phase 12 when
// the API stabilizes further (tracked in todo.md). Keep in sync with server!
export interface ApiResponse<T> {
  success: boolean
  data: T
  error?: string | null
}

// ---------------------------------------------------------------- auth
export interface LoginRequest {
  hotel_id: number
  username: string
  password: string
}

export interface SessionUser {
  user_id: string
  hotel_id: number
  username: string
  full_name: string
  role: string
  force_password_change: boolean
}

export interface LoginResponse {
  token: string
  user: SessionUser
  permissions: string[]
}

// ---------------------------------------------------------------- hotels
export interface HotelDto {
  id: number
  name: string
  default_currency: string
  time_zone: string
  is_active: boolean
  next_change_seq: number
}

// ---------------------------------------------------------------- users/roles
export interface UserDto {
  id: string
  username: string
  full_name: string | null
  email: string | null
  role: string
  is_active: boolean
  force_password_change: boolean
}

export interface PermissionState {
  permission: string
  default_allowed: boolean
  hotel_override: boolean | null
  effective: boolean
}

export interface RoleMatrix {
  role: string
  permissions: PermissionState[]
}

// ---------------------------------------------------------------- rooms
export interface RoomTypeDto {
  id: string
  name: string
  capacity: number
  base_price_cents: number
  amenities: string[]
}

export interface RoomDto {
  id: string
  room_type_id: string
  room_number: string
  floor: number
  cleanliness: string
}

// ---------------------------------------------------------------- guests
export interface GuestDto {
  id: string
  full_name: string
  nationality: string | null
  license_plate: string | null
  birth_date: string | null
  is_primary: boolean
  kbs_reported: boolean
}

// ---------------------------------------------------------------- channels/price rules
export interface ChannelDto {
  id: string
  kind: string
  name: string
  is_active: boolean
}

export interface PriceRuleDto {
  id: string
  room_type_id: string
  date_from: string
  date_to: string
  price_cents: number
  currency: string
}

// ---------------------------------------------------------------- reservations
export interface DailyRate {
  date: string
  price_cents: number
}

export interface ReservationDto {
  id: string
  hotel_id: number
  room_id: string
  channel_id: string
  guest_id: string | null
  checkin_date: string
  checkout_date: string
  status: string
  daily_rates: DailyRate[] | { unknown: [] }
  total_price_cents: number
  currency: string
  adults: number
  children: number
  notes: string | null
  voucher_number: string | null
  created_at: string
}

export interface CreateReservationBody {
  room_id: string
  channel_id: string
  guest_id?: string | null
  checkin_date: string
  checkout_date: string
  adults: number
  children: number
  currency?: string
  notes?: string
  voucher_number?: string
}

export interface ReservationListQuery {
  from?: string
  to?: string
  status?: string
  limit?: number
  after?: string
}

// ---------------------------------------------------------------- payments/folio
export interface PaymentDto {
  id: string
  reservation_id: string
  amount_cents: number
  currency: string
  payment_type: string
  reference: string | null
  is_reversal: boolean
  created_by: string
  created_at: string
}

export interface FolioDto {
  id: string
  reservation_id: string
  category: string
  description: string
  unit_price_cents: number
  quantity: number
  total_cents: number
  currency: string
  created_at: string
}

// ---------------------------------------------------------------- reports
export interface SummaryReport {
  from: string
  to: string
  currency: string
  room_revenue_cents: number
  extra_revenue_cents: number
  payments_collected_cents: number
  occupied_nights: number
  available_room_nights: number
  occupancy_pct: number
  adr_cents: number | null
  revpar_cents: number | null
}

export interface ChannelRevenue {
  channel_id: string
  channel_name: string
  kind: string
  room_revenue_cents: number
  extra_revenue_cents: number
  payments_collected_cents: number
  occupied_nights: number
}

export interface DayReport {
  date: string
  room_revenue_cents: number
  extra_revenue_cents: number
  payments_collected_cents: number
  occupied_nights: number
}

export interface RoomRevenue {
  room_id: string
  room_number: string
  room_type_name: string
  room_revenue_cents: number
  extra_revenue_cents: number
  occupied_nights: number
}

// ---------------------------------------------------------------- board
export interface BoardStay {
  reservation_id: string
  checkin_date: string
  checkout_date: string
  status: string
  guest_name: string | null
  total_price_cents: number
  currency: string
}

export interface BoardRoom {
  id: string
  room_number: string
  room_type_id: string
  room_type_name: string
  cleanliness: string
  stays: BoardStay[]
  seasonal_price_cents: number
}

export interface BoardDay {
  date: string
  available: number
  occupied: number
  from_price_cents: number | null
}

export interface BoardResponse {
  from: string
  to: string
  rooms: BoardRoom[]
  occupancy: BoardDay[]
}
