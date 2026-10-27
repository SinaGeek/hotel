// Placeholder until ts-rs / OpenAPI generation fills src/types/generated/ (Phase 4)
export interface ApiResponse<T> {
  success: boolean
  data: T
  error?: string | null
}

export interface LoginRequest {
  hotel_id: number
  username: string
  password: string
}
