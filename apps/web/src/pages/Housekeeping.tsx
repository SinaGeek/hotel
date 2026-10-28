// Facility > Housekeeping (Phase 5b.5, reference parity per REA capture
// facility-housekeeping.ax.txt): hotel-name heading with record count, Floor
// + Cleaning filters (the reference filters by building; our model has
// floors), Download CSV, and one flat table Room / Check-in / Check-out /
// Status (cleanliness). Occupancy comes from the board API over [today,
// tomorrow) — half-open dates, so a stay covers today when
// checkin <= today < checkout).
import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { api, ApiError, sessionHasPermission } from '../api/client'
import { useAuth } from '../api/auth'
import { FilterBar, PageHeader, StatusPill } from '../components/ui'
import { iso } from '../components/reservation-modals'
import type { BoardResponse } from '../types/generated/api'

interface HkRow {
  id: string
  number: string
  floor: number
  cleanliness: string
  checkin: string | null
  checkout: string | null
}

function downloadCsv(rows: HkRow[]) {
  const esc = (v: string) => (/[",\n;]/.test(v) ? `"${v.replace(/"/g, '""')}"` : v)
  const header = 'Room,Floor,Check-in,Check-out,Status'
  const lines = rows.map((r) => [
    esc(r.number), String(r.floor), esc(r.checkin ?? ''), esc(r.checkout ?? ''), esc(r.cleanliness),
  ].join(','))
  const blob = new Blob(['\ufeff' + header + '\n' + lines.join('\n')], { type: 'text/csv;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `housekeeping-${iso(new Date())}.csv`
  a.click()
  URL.revokeObjectURL(url)
}

export function Housekeeping() {
  const { t } = useTranslation()
  const { session } = useAuth()
  const hotelId = session?.user.hotel_id
  const [hotelName, setHotelName] = useState<string | null>(null)
  const [board, setBoard] = useState<BoardResponse | null>(null)
  const [floor, setFloor] = useState('')
  const [cleanliness, setCleanliness] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  // housekeeping_update_status permission gates server-side PATCHes; the
  // page itself is read-only, so only the visibility of the filters matters
  void sessionHasPermission(session, 'housekeeping_update_status')

  const load = useCallback(async () => {
    try {
      setBusy(true)
      setError(null)
      const today = iso(new Date())
      const tomorrow = iso(new Date(Date.now() + 86_400_000))
      setBoard(await api.board(today, tomorrow))
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }, [])
  useEffect(() => { void load() }, [load])

  useEffect(() => {
    if (hotelId === undefined) return
    let alive = true
    api.listHotels()
      .then((hs) => { if (alive) setHotelName(hs.find((h) => h.id === hotelId)?.name ?? null) })
      .catch(() => { /* falls back to the code only */ })
    return () => { alive = false }
  }, [hotelId])

  const today = iso(new Date())
  const rows = useMemo<HkRow[]>(() => {
    return (board?.rooms ?? [])
      .filter((r) => floor === '' || r.floor === Number(floor))
      .filter((r) => cleanliness === '' || r.cleanliness === cleanliness)
      .map((r) => {
        const stay = r.stays.find((s) => (s.status === 'checked_in' || s.status === 'confirmed') &&
          s.checkin_date <= today && s.checkout_date > today)
        return {
          id: r.id,
          number: r.room_number,
          floor: r.floor,
          cleanliness: r.cleanliness,
          checkin: stay?.checkin_date ?? null,
          checkout: stay?.checkout_date ?? null,
        }
      })
  }, [board, floor, cleanliness, today])

  const floors = useMemo(
    () => [...new Set((board?.rooms ?? []).map((r) => r.floor))].sort((a, b) => a - b),
    [board],
  )

  const states = ['dirty', 'cleaning', 'clean', 'inspected', 'out_of_order']

  return (
    <div className="page">
      <PageHeader
        title={t('nav.housekeeping')}
        subtitle={busy ? t('common.loading') : t('bk.records', { n: rows.length })}
      />
      {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}

      <FilterBar>
        <select value={floor} onChange={(e) => setFloor(e.target.value)}>
          <option value="">{t('fac.filterFloor')}</option>
          {floors.map((f) => <option key={f} value={f}>{t('fac.floorN', { n: f })}</option>)}
        </select>
        <select value={cleanliness} onChange={(e) => setCleanliness(e.target.value)}>
          <option value="">{t('fac.filterCleaning')}</option>
          {states.map((s) => <option key={s} value={s}>{t(`rooms.${s}`)}</option>)}
        </select>
        <button className="ghost" disabled={floor === '' && cleanliness === ''} onClick={() => { setFloor(''); setCleanliness('') }}>
          {t('bk.clear')}
        </button>
        <button
          className="ghost"
          onClick={() => downloadCsv(rows)}
          title={t('fac.downloadTip')}
        >
          {t('gu.download')}
        </button>
      </FilterBar>

      <h3 style={{ margin: '4px 0 8px' }}>
        {hotelName ?? t('fac.hotelFallback')} <span className="muted">{rows.length}</span>
      </h3>

      <table className="hms">
        <thead>
          <tr>
            <th>{t('rooms.number')}</th><th>{t('res.checkin')}</th>
            <th>{t('res.checkout')}</th><th>{t('fac.cleaningStatus')}</th>
          </tr>
        </thead>
        <tbody>
          {rows.length === 0 && !busy && (
            <tr><td colSpan={4} className="muted" style={{ textAlign: 'center' }}>{t('bk.noRows')}</td></tr>
          )}
          {rows.map((r) => (
            <tr key={r.id}>
              <td style={{ fontWeight: 600 }}>{r.number}</td>
              <td>{r.checkin ?? <span className="muted">—</span>}</td>
              <td>{r.checkout ?? <span className="muted">—</span>}</td>
              <td><StatusPill status={r.cleanliness} label={t(`rooms.${r.cleanliness}`)} /></td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
