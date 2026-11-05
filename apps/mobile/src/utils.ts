// G5: mobile app local helpers — date ranges, currency and formatting.
import type { DailyRate } from '@hms/shared-types'

export function todayISO(): string {
  return new Date().toISOString().slice(0, 10)
}

export function addDaysISO(date: string, days: number): string {
  const d = new Date(date + 'T00:00:00Z')
  d.setUTCDate(d.getUTCDate() + days)
  return d.toISOString().slice(0, 10)
}

export function formatMoney(cents: number, currency = 'TRY'): string {
  const negative = cents < 0
  const abs = Math.abs(cents)
  const whole = Math.floor(abs / 100)
  const frac = String(abs % 100).padStart(2, '0')
  const grouped = whole.toString().replace(/\B(?=(\d{3})+(?!\d))/g, '.')
  return `${negative ? '-' : ''}${grouped},${frac} ${currency}`
}

/** Sum the per-night rates of a reservation (server sends `[]` when hidden). */
export function totalFromRates(rates: DailyRate[] | { unknown: [] } | undefined): number {
  if (!Array.isArray(rates)) return 0
  return rates.reduce((sum, r) => sum + r.price_cents, 0)
}

export function shortDate(iso: string): string {
  const [, m, d] = iso.split('-')
  return `${d}.${m}`
}
