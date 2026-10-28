// Reservations list — keyset pagination (README §12: never OFFSET).
import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { api, ApiError } from '../api/client'
import { StatusPill } from '../components/ui'
import type { ReservationDto } from '../types/generated/api'

const fmtMoney = (cents: number, currency: string) =>
  new Intl.NumberFormat(undefined, { style: 'currency', currency, maximumFractionDigits: 0 }).format(cents / 100)

export function Reservations() {
  const { t } = useTranslation()
  const [items, setItems] = useState<ReservationDto[]>([])
  const [status, setStatus] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const load = useCallback(async (after?: string, append?: boolean) => {
    setBusy(true)
    try {
      setError(null)
      const rows = await api.listReservations({ limit: 50, status: status || undefined, after })
      setItems((prev) => (append ? [...prev, ...rows] : rows))
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }, [status])

  useEffect(() => { void load() }, [load])

  const lastId = items.length > 0 ? items[items.length - 1].id : undefined

  return (
    <div className="page">
      <div className="row">
        <h2 style={{ margin: 0, flexGrow: 1 }}>{t('res.list')}</h2>
        <select value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">— {t('res.status')} —</option>
          <option value="confirmed">{t('res.confirmed')}</option>
          <option value="checked_in">{t('res.checked_in')}</option>
          <option value="checked_out">{t('res.checked_out')}</option>
          <option value="cancelled">{t('res.cancelled')}</option>
          <option value="conflict">{t('res.conflict')}</option>
        </select>
      </div>
      {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}
      <table className="hms">
        <thead>
          <tr><th>#</th><th>{t('res.checkin')}</th><th>{t('res.checkout')}</th><th>{t('res.status')}</th><th>{t('res.adults')}</th><th>{t('res.total')}</th><th>{t('res.voucher')}</th></tr>
        </thead>
        <tbody>
          {items.map((r) => (
            <tr key={r.id}>
              <td className="muted">{r.id.slice(0, 8)}</td>
              <td>{r.checkin_date}</td>
              <td>{r.checkout_date}</td>
              <td><StatusPill status={r.status} label={t(`res.${r.status}`)} /></td>
              <td>{r.adults}+{r.children}</td>
              <td>{fmtMoney(r.total_price_cents, r.currency)}</td>
              <td>{r.voucher_number ?? ''}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="row" style={{ marginTop: 10 }}>
        <button className="ghost" disabled={busy || !lastId} onClick={() => void load(lastId, true)}>{busy ? t('common.loading') : '↓ more'}</button>
      </div>
    </div>
  )
}
