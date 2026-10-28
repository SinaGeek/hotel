// Monthly Room board (Phase 5b.2, reference parity per docs/ui-reference.md):
// Calendar/List views, 7 days / 15 days / Month range with period pager
// (Today disabled on the current period), per-room-type price rows fed by
// price rules, grouped room rows, stay bars, legend, drag across empty days
// to create, click a bar for the reservation detail.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { api, sessionHasPermission, ApiError } from '../api/client'
import { useAuth } from '../api/auth'
import { PageHeader, StatusPill } from '../components/ui'
import { NewReservationModal, ReservationDetailModal, fmtMoney } from '../components/reservation-modals'
import type { BoardResponse, BoardRoom, ChannelDto, GuestDto, PriceRuleDto } from '../types/generated/api'

const DAY_MS = 86_400_000
const iso = (d: Date) => d.toISOString().slice(0, 10)
const addDays = (date: string, n: number) =>
  iso(new Date(new Date(date + 'T00:00:00Z').getTime() + n * DAY_MS))
const DOW = ['SUN', 'MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT']
const fmtNum = (n: number) => n.toLocaleString('en-US')
const fmtMD = (d: string) =>
  new Date(d + 'T00:00:00Z').toLocaleDateString('en-US', { month: 'short', day: 'numeric' })

type Mode = '7' | '15' | 'month'
const MODE_LABEL: Record<Mode, string> = { '7': 'board.seven', '15': 'board.fifteen', month: 'board.month' }

// [from, to) like the server board API
function periodFor(anchor: string, mode: Mode): { from: string; to: string } {
  if (mode === '7') return { from: anchor, to: addDays(anchor, 7) }
  if (mode === '15') return { from: anchor, to: addDays(anchor, 15) }
  const y = Number(anchor.slice(0, 4))
  const m = Number(anchor.slice(5, 7))
  const next = m === 12 ? `${y + 1}-01-01` : `${y}-${String(m + 1).padStart(2, '0')}-01`
  return { from: anchor.slice(0, 8) + '01', to: next }
}
function shiftAnchor(anchor: string, mode: Mode, dir: 1 | -1): string {
  if (mode === 'month') {
    const y = Number(anchor.slice(0, 4))
    const nm = Number(anchor.slice(5, 7)) - 1 + dir
    const ny = y + Math.floor(nm / 12)
    const mm = (((nm % 12) + 12) % 12) + 1
    return `${ny}-${String(mm).padStart(2, '0')}-01`
  }
  return addDays(anchor, dir * (mode === '7' ? 7 : 15))
}
function rangeLabel(from: string, to: string): string {
  const end = addDays(to, -1)
  return `${fmtMD(from)} - ${fmtMD(end)}, ${end.slice(0, 4)}`
}

type Section = { typeId: string; name: string; seasonal: number; rooms: BoardRoom[] }
type LayoutItem =
  | { kind: 'type'; name: string; typeId: string; row: number }
  | { kind: 'room'; room: BoardRoom; row: number }
const CLEAN_DOT: Record<string, string> = {
  clean: 'ok', inspected: 'ok', cleaning: 'warn', dirty: 'bad', out_of_order: 'na',
}

export function Board() {
  const { t } = useTranslation()
  const { session } = useAuth()
  const isHk = session?.user.role === 'housekeeping'

  const [mode, setMode] = useState<Mode>('15')
  const [anchor, setAnchor] = useState(() => iso(new Date()))
  const [view, setView] = useState<'calendar' | 'list'>('calendar')
  const { from, to } = useMemo(() => periodFor(anchor, mode), [anchor, mode])
  const today = iso(new Date())
  const onCurrentPeriod = from <= today && today < to

  const [board, setBoard] = useState<BoardResponse | null>(null)
  const [channels, setChannels] = useState<ChannelDto[]>([])
  const [guests, setGuests] = useState<GuestDto[]>([])
  const [rules, setRules] = useState<PriceRuleDto[]>([])
  const [hotelName, setHotelName] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [drag, setDrag] = useState<{ roomId: string; start: number; end: number } | null>(null)
  const [creating, setCreating] = useState<{ roomId: string; date: string; nights: number } | null>(null)
  const [detailId, setDetailId] = useState<string | null>(null)
  const [reloadKey, setReloadKey] = useState(0)

  const sessionRef = useRef(session)
  sessionRef.current = session
  const isHkRef = useRef(isHk)
  isHkRef.current = isHk

  const hotelId = session?.user.hotel_id
  useEffect(() => {
    if (hotelId === undefined) return
    let alive = true
    api.listHotels()
      .then((hs) => { if (alive) setHotelName(hs.find((h) => h.id === hotelId)?.name ?? null) })
      .catch(() => { /* hotel row falls back to the app name */ })
    return () => { alive = false }
  }, [hotelId])

  const load = useCallback(async () => {
    try {
      setError(null)
      setBoard(await api.board(from, to))
      setChannels(await api.listChannels())
      if (sessionHasPermission(sessionRef.current, 'reservation_view') && !isHkRef.current) setGuests(await api.listGuests(''))
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    }
  }, [from, to])

  useEffect(() => { if (sessionRef.current) void load() }, [load, reloadKey])

  // per-type daily prices (seasonal price override); non-fatal if not permitted
  useEffect(() => {
    if (sessionRef.current == null) return
    let alive = true
    api.listPriceRules()
      .then((rs) => { if (alive) setRules(rs) })
      .catch(() => { /* type header falls back to the seasonal price */ })
    return () => { alive = false }
  }, [])

  const days = useMemo(() => {
    const out: string[] = []
    const start = new Date(from + 'T00:00:00Z').getTime()
    const end = new Date(to + 'T00:00:00Z').getTime()
    for (let d = start; d < end; d += DAY_MS) out.push(iso(new Date(d)))
    return out
  }, [from, to])
  const todayIdx = days.indexOf(today)

  // drag across empty days -> create modal on mouseup.
  // Geometry lives in a ref updated synchronously on mousedown/mousemove so
  // the mouseup commit never sees a stale React batch; `drag` state only
  // drives the highlight. Hit-tested against day cells so floating stay bars
  // cannot interrupt the drag.
  const dragRef = useRef<{ roomId: string; start: number; end: number } | null>(null)
  const dragActive = drag != null
  useEffect(() => {
    if (!dragActive) return
    const onMove = (e: MouseEvent) => {
      const cell = e.target instanceof Element ? e.target.closest('.bd-day') : null
      if (cell instanceof HTMLElement && dragRef.current != null) {
        const roomId = cell.dataset['roomId']
        const di = Number(cell.dataset['dayIdx'])
        if (roomId != null && roomId === dragRef.current.roomId) {
          dragRef.current = { ...dragRef.current, end: di }
          setDrag((cur) => (cur != null && cur.roomId === roomId ? { ...cur, end: di } : cur))
        }
      }
    }
    const commit = () => {
      const d = dragRef.current
      if (d == null) return
      const a = Math.min(d.start, d.end)
      const b = Math.max(d.start, d.end)
      dragRef.current = null
      setDrag(null)
      setCreating({ roomId: d.roomId, date: days[a], nights: b - a + 1 })
    }
    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', commit)
    return () => {
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', commit)
    }
  }, [dragActive, days])

  const typeSections = useMemo<Section[]>(() => {
    const out: Section[] = []
    for (const r of board?.rooms ?? []) {
      let s = out.find((x) => x.typeId === r.room_type_id)
      if (!s) {
        s = { typeId: r.room_type_id, name: r.room_type_name, seasonal: r.seasonal_price_cents, rooms: [] }
        out.push(s)
      }
      s.rooms.push(r)
    }
    return out
  }, [board])

  const priceFor = useCallback((typeId: string, date: string): number | null => {
    const matches = rules.filter((r) => r.room_type_id === typeId && r.date_from <= date && r.date_to >= date)
    const latest = matches.sort((a, b) => (a.date_from < b.date_from ? 1 : -1))[0]
    if (latest) return Math.round(latest.price_cents / 100)
    const sec = typeSections.find((s) => s.typeId === typeId)
    return sec && sec.seasonal > 0 ? Math.round(sec.seasonal / 100) : null
  }, [rules, typeSections])

  // flat grid layout: 1 = day header, 2 = hotel row, 3+ = type/room rows
  const layout = useMemo<LayoutItem[]>(() => {
    const out: LayoutItem[] = []
    let row = 2
    for (const sec of typeSections) {
      row += 1
      out.push({ kind: 'type', name: sec.name, typeId: sec.typeId, row })
      for (const room of sec.rooms) {
        row += 1
        out.push({ kind: 'room', room, row })
      }
    }
    return out
  }, [typeSections])

  const listRows = useMemo(() => {
    const out: Array<{
      key: string; resId: string; room: string; type: string; guest: string; checkin: string
      checkout: string; nights: number; status: string; total: string
    }> = []
    for (const room of board?.rooms ?? []) {
      for (const s of room.stays) {
        if (s.checkout_date <= from || s.checkin_date >= to) continue
        out.push({
          key: room.id + s.reservation_id + s.checkin_date,
          resId: s.reservation_id,
          room: room.room_number,
          type: room.room_type_name,
          guest: s.guest_name ?? `#${s.reservation_id.slice(0, 6)}`,
          checkin: s.checkin_date,
          checkout: s.checkout_date,
          nights: Math.round((new Date(s.checkout_date + 'T00:00:00Z').getTime() - new Date(s.checkin_date + 'T00:00:00Z').getTime()) / DAY_MS),
          status: s.status,
          total: fmtMoney(s.total_price_cents, s.currency),
        })
      }
    }
    return out
  }, [board, from, to])

  if (!session?.user) return null

  const toolbar = (
    <div className="row" style={{ gap: 10, flexWrap: 'wrap' }}>
      <div className="seg" role="tablist" aria-label="View">
        <button className={view === 'calendar' ? 'on' : ''} onClick={() => setView('calendar')}>{t('board.calendar')}</button>
        <button className={view === 'list' ? 'on' : ''} onClick={() => setView('list')}>{t('board.list')}</button>
      </div>
      <div className="seg" aria-label="Range">
        {(['7', '15', 'month'] as const).map((m) => (
          <button key={m} className={mode === m ? 'on' : ''} onClick={() => setMode(m)}>{t(MODE_LABEL[m])}</button>
        ))}
      </div>
      <div className="seg" style={{ gap: 0 }}>
        <button style={{ padding: '6px 10px' }} onClick={() => setAnchor(shiftAnchor(anchor, mode, -1))} aria-label="Previous period">&#8249;</button>
        <span style={{ padding: '6px 10px', minWidth: 175, textAlign: 'center', fontSize: 13 }}>{rangeLabel(from, to)}</span>
        <button style={{ padding: '6px 10px' }} onClick={() => setAnchor(shiftAnchor(anchor, mode, 1))} aria-label="Next period">&#8250;</button>
      </div>
      <button className="ghost" disabled={onCurrentPeriod} onClick={() => setAnchor(iso(new Date()))}>{t('board.today')}</button>
      <button className="ghost" onClick={() => void load()} aria-label="Refresh">&#10227;</button>
    </div>
  )

  return (
    <div className="page">
      <PageHeader title={t('nav.board')} actions={toolbar} />
      {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}

      {view === 'list' ? (
        <table className="hms">
          <thead>
            <tr>
              <th>{t('rooms.number')}</th><th>{t('rooms.type')}</th><th>{t('res.guest')}</th>
              <th>{t('res.checkin')}</th><th>{t('res.checkout')}</th><th>{t('board.nights')}</th>
              <th>{t('res.status')}</th><th>{t('res.total')}</th>
            </tr>
          </thead>
          <tbody>
            {listRows.length === 0 && (
              <tr><td colSpan={8} className="muted" style={{ textAlign: 'center' }}>{t('reports.noData')}</td></tr>
            )}
            {listRows.map((r) => (
              <tr
                key={r.key}
                style={{ cursor: 'pointer' }}
                onClick={() => setDetailId(r.resId)}
              >
                <td>{r.room}</td>
                <td className="muted">{r.type}</td>
                <td>{r.guest}</td>
                <td>{r.checkin}</td>
                <td>{r.checkout}</td>
                <td>{r.nights}</td>
                <td><StatusPill status={r.status} label={t(`res.${r.status}`)} /></td>
                <td>{r.total}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <>
          <div className="board-scroll">
            <div className="board-grid" style={{ gridTemplateColumns: `205px repeat(${days.length}, 48px)` }}>
              {/* day header row */}
              <div className="bd-cell bd-rail bd-rail-top" style={{ gridRow: 1, gridColumn: 1 }}>
                <div className="bd-range">{rangeLabel(from, to)}</div>
                <div className="muted" style={{ fontSize: 11 }}>{t('board.roomsCount', { n: board?.rooms.length ?? 0 })}</div>
              </div>
              {days.map((d, di) => {
                const wd = new Date(d + 'T00:00:00Z').getUTCDay()
                return (
                  <div
                    key={d}
                    className={`bd-cell bd-head${wd === 0 || wd === 6 ? ' weekend' : ''}`}
                    style={{ gridRow: 1, gridColumn: di + 2 }}
                  >
                    <div className="bd-dow">{DOW[wd]}</div>
                    <div className="bd-num">{d === today
                      ? <span className="bd-todaynum">{Number(d.slice(8))}</span>
                      : Number(d.slice(8))}</div>
                  </div>
                )
              })}

              {/* hotel row */}
              <div className="bd-cell bd-rail bd-hotel" style={{ gridRow: 2, gridColumn: 1 }}>
                <span className="bd-accent" />
                <span className="bd-hotelname">{hotelName ?? t('app.title')}</span>
                <span className="muted" style={{ marginLeft: 'auto', fontSize: 11 }}>{board?.rooms.length ?? 0}</span>
              </div>
              {days.map((d, di) => (
                <div key={d} className={`bd-cell${di === todayIdx ? ' bd-tint' : ''}`} style={{ gridRow: 2, gridColumn: di + 2, minHeight: 32 }} />
              ))}

              {/* type header + room rows */}
              {layout.map((item) => item.kind === 'type' ? (
                <div key={'t' + item.typeId} style={{ display: 'contents' }}>
                  <div className="bd-cell bd-rail bd-typerail" style={{ gridRow: item.row, gridColumn: 1 }}>{item.name}</div>
                  {days.map((d, di) => {
                    const p = priceFor(item.typeId, d)
                    return (
                      <div
                        key={d}
                        className={`bd-cell bd-price${di === todayIdx ? ' bd-tint' : ''}`}
                        style={{ gridRow: item.row, gridColumn: di + 2, minHeight: 30 }}
                      >
                        {p != null ? fmtNum(p) : ''}
                      </div>
                    )
                  })}
                </div>
              ) : (
                <div key={item.room.id} style={{ display: 'contents' }}>
                  <div className="bd-cell bd-rail" style={{ gridRow: item.row, gridColumn: 1 }}>
                    <span className={`dot ${CLEAN_DOT[item.room.cleanliness] ?? 'na'}`} />
                    <span className="bd-roomnum">{item.room.room_number}</span>
                  </div>
                  {days.map((d, di) => {
                    const dragging = drag != null && drag.roomId === item.room.id &&
                      di >= Math.min(drag.start, drag.end) && di <= Math.max(drag.start, drag.end)
                    return (
                      <div
                        key={d}
                        className={`bd-cell bd-day${di === todayIdx ? ' bd-tint' : ''}${dragging ? ' bd-dragging' : ''}`}
                        style={{ gridRow: item.row, gridColumn: di + 2 }}
                        title={t('board.clickToBook')}
                        data-room-id={item.room.id}
                        data-day-idx={di}
                        onMouseDown={(e) => { e.preventDefault(); dragRef.current = { roomId: item.room.id, start: di, end: di }; setDrag({ roomId: item.room.id, start: di, end: di }) }}
                      />
                    )
                  })}
                  {item.room.stays
                    .filter((s) => s.status !== 'cancelled' && s.status !== 'conflict' && s.checkout_date > from && s.checkin_date < to)
                    .map((s) => {
                      let ci = days.indexOf(s.checkin_date)
                      if (ci < 0) ci = 0
                      const ce = days.indexOf(s.checkout_date)
                      const span = Math.max((ce < 0 ? days.length : ce) - ci, 1)
                      const nights = Math.round(
                        (new Date(s.checkout_date + 'T00:00:00Z').getTime() - new Date(s.checkin_date + 'T00:00:00Z').getTime()) / DAY_MS,
                      )
                      return (
                        <div
                          key={s.reservation_id}
                          className={`stay ${s.status}`}
                          style={{ gridColumn: `${ci + 2} / span ${span}`, gridRow: item.row }}
                          onClick={(e) => { e.stopPropagation(); setDetailId(s.reservation_id) }}
                        >
                          {isHk ? '●' : (
                            <>
                              <b>{s.guest_name ?? `#${s.reservation_id.slice(0, 6)}`}</b>
                              <span>{nights} {t('board.nights')}</span>
                            </>
                          )}
                        </div>
                      )
                    })}
                </div>
              ))}
            </div>
          </div>

          {/* legend / hint footer */}
          <div className="board-foot">
            <span className="lg"><span className="dotb ok" />{t('board.legend.reservation')}</span>
            <span className="lg"><span className="dotb gold" />{t('board.legend.conflict')}</span>
            <span className="lg"><span className="dotb na" />{t('board.legend.cancelled')}</span>
            <span className="muted">{t('board.legend.dragHint')}</span>
            <span className="row" style={{ marginLeft: 'auto', gap: 14 }}>
              <span className="lg"><span className="dot ok" />{t('rooms.clean')}</span>
              <span className="lg"><span className="dot warn" />{t('rooms.dirty')}</span>
            </span>
          </div>
        </>
      )}

      {creating && (
        <NewReservationModal
          prefill={creating}
          rooms={board?.rooms ?? []}
          channels={channels}
          guests={guests}
          onClose={() => setCreating(null)}
          onCreated={(r) => { setCreating(null); setDetailId(r.id); setReloadKey((k) => k + 1) }}
        />
      )}
      {detailId && (
        <ReservationDetailModal id={detailId} onClose={() => setDetailId(null)} onChanged={() => setReloadKey((k) => k + 1)} />
      )}
    </div>
  )
}

