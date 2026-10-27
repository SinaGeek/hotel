// Dashboard: today's KPIs from the board (README §4 landing numbers).
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { api, ApiError } from '../api/client'
import { useAuth } from '../api/auth'
import type { BoardResponse } from '../types/generated/api'

const fmtMoney = (cents: number, currency: string) =>
  new Intl.NumberFormat(undefined, { style: 'currency', currency, maximumFractionDigits: 0 }).format(cents / 100)

export function Dashboard() {
  const { t } = useTranslation()
  const { session } = useAuth()
  const isHk = session?.user.role === 'housekeeping'
  const [board, setBoard] = useState<BoardResponse | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    void (async () => {
      try {
        const today = new Date().toISOString().slice(0, 10)
        const week = new Date(Date.now() + 6 * 86_400_000).toISOString().slice(0, 10)
        setBoard(await api.board(today, week))
      } catch (e) {
        setError(e instanceof ApiError ? e.message : String(e))
      }
    })()
  }, [])

  const today = board?.occupancy[0]
  const totalRooms = board?.rooms.length ?? 0
  const inHouse = board?.rooms.flatMap((r) => r.stays.filter((s) => s.status === 'checked_in').length) ?? 0
  const currency = board?.rooms[0]?.stays[0]?.currency ?? 'TRY'

  return (
    <div className="page">
      <h2>{t('nav.dashboard')}</h2>
      {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}
      <div className="kpis">
        <div className="kpi"><b>{today ? `${today.occupied}/${totalRooms}` : '—'}</b><span>{t('nav.board')} · {t('board.occupancy')}</span></div>
        <div className="kpi"><b>{today ? `${Math.round((today.occupied / Math.max(totalRooms, 1)) * 100)}%` : '—'}</b><span>%</span></div>
        {!isHk && <div className="kpi"><b>{inHouse}</b><span>{t('res.checked_in')}</span></div>}
        {!isHk && today?.from_price_cents && (
          <div className="kpi"><b>{fmtMoney(today.from_price_cents, currency)}</b><span>{t('board.cheapest')}</span></div>
        )}
      </div>
      {board && !isHk && (
        <div>
          {board.rooms.map((r) => (
            <div key={r.id} className="card row" style={{ padding: '8px 12px', marginBottom: 6, justifyContent: 'space-between' }}>
              <b>{r.room_number}</b>
              <span>{r.room_type_name}</span>
              <span className="muted">{t(`rooms.${r.cleanliness}`)}</span>
              {!isHk && <span>{r.stays.length > 0 ? `${r.stays.length} stays` : '—'}</span>}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
