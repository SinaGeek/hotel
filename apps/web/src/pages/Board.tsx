// Room board (README §4 centerpiece): rooms as rows, dates as columns,
// stays as colored blocks; click empty cell → create reservation, click a
// stay → detail with check-in/out/cancel, payments and folio.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { api, sessionHasPermission, ApiError } from '../api/client'
import { useAuth } from '../api/auth'
import type { BoardResponse, BoardRoom, ChannelDto, GuestDto, ReservationDto } from '../types/generated/api'

const DAY_MS = 86_400_000
const iso = (d: Date) => d.toISOString().slice(0, 10)
const fmtMoney = (cents: number, currency: string) =>
  new Intl.NumberFormat(undefined, { style: 'currency', currency, maximumFractionDigits: 0 }).format(cents / 100)

export function Board() {
  const { t } = useTranslation()
  const { session } = useAuth()
  const isHk = session?.user.role === 'housekeeping'

  const [from, setFrom] = useState(() => iso(new Date()))
  const [to, setTo] = useState(() => iso(new Date(Date.now() + 6 * DAY_MS)))
  const [board, setBoard] = useState<BoardResponse | null>(null)
  const [channels, setChannels] = useState<ChannelDto[]>([])
  const [guests, setGuests] = useState<GuestDto[]>([])
  const [error, setError] = useState<string | null>(null)

  const [creating, setCreating] = useState<{ roomId: string; date: string } | null>(null)
  const [detailId, setDetailId] = useState<string | null>(null)
  const [reloadKey, setReloadKey] = useState(0)

  const sessionRef = useRef(session)
  sessionRef.current = session

  const load = useCallback(async () => {
    try {
      setError(null)
      setBoard(await api.board(from, to))
      setChannels(await api.listChannels())
      if (sessionHasPermission(sessionRef.current, 'reservation_view') && !isHk) setGuests(await api.listGuests(''))
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    }
  }, [from, to, isHk])

  useEffect(() => { if (sessionRef.current) void load() }, [load, reloadKey])

  const days = useMemo(() => {
    const out: string[] = []
    const start = new Date(from + 'T00:00:00Z').getTime()
    const end = new Date(to + 'T00:00:00Z').getTime()
    for (let d = start; d < end; d += DAY_MS) out.push(iso(new Date(d)))
    return out
  }, [from, to])

  if (!session?.user) return null
  const occByDate = new Map(board?.occupancy.map((o) => [o.date, o]))

  return (
    <div className="page">
      <div className="row">
        <h2 style={{ margin: 0, flexGrow: 1 }}>{t('nav.board')}</h2>
        <input type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
        <input type="date" value={to} onChange={(e) => setTo(e.target.value)} />
        <button className="ghost" onClick={() => { setFrom(iso(new Date())); setTo(iso(new Date(Date.now() + 6 * DAY_MS))) }}>{t('board.week')}</button>
        <button className="ghost" onClick={() => void load()}>⟳</button>
      </div>
      {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}

      <div className="board-grid" style={{ gridTemplateColumns: `200px repeat(${days.length}, 48px)` }}>
        {/* header row: date + occupancy + cheapest price (explicit placement) */}
        <div className="board-cell board-head board-roomname" style={{ gridRow: 1 }}>{t('rooms.number')}</div>
        {days.map((d, di) => {
          const o = occByDate.get(d)
          return (
            <div key={d} className="board-cell board-head" style={{ gridRow: 1, gridColumn: di + 2 }}>
              <div>{d.slice(5)}</div>
              {!isHk && o && (
                <div className="muted" style={{ fontSize: 10 }}>
                  {o.available}/{o.available + o.occupied}
                  {o.from_price_cents ? <span> · {fmtMoney(o.from_price_cents, 'TRY')}</span> : null}
                </div>
              )}
            </div>
          )
        })}

        {/* one row per room; stay blocks span their nights (grid-column span) */}
        {(board?.rooms ?? []).map((room, ri) => {
          const rowIdx = ri + 2
          const active = room.stays.filter(
            (s) => s.status !== 'cancelled' && s.status !== 'conflict' && s.checkout_date > (from ?? '') && s.checkin_date < (to ?? ''),
          )
          const blockedDays = new Set<string>()
          for (const s of active) {
            for (const d of days) if (s.checkin_date <= d && s.checkout_date > d) blockedDays.add(d)
          }
          return (
            <div key={room.id} style={{ display: 'contents' }}>
              <div className="board-cell board-roomname" style={{ gridRow: rowIdx }}>
                <div>{room.room_number}</div>
                <div className="muted" style={{ fontSize: 10 }}>{room.room_type_name} · {room.cleanliness}</div>
              </div>
              {days.map((d, di) => (
                <div
                  key={d}
                  className="board-cell board-day"
                  style={{ gridRow: rowIdx, gridColumn: di + 2 }}
                  title={blockedDays.has(d) ? t('board.noStays') : t('board.clickToBook')}
                  onClick={() => { if (!blockedDays.has(d)) setCreating({ roomId: room.id, date: d }) }}
                />
              ))}
              {active.map((s) => {
                let ci = days.indexOf(s.checkin_date)
                if (ci < 0) ci = 0
                const endEx = days.indexOf(s.checkout_date) < 0 ? days.length : days.indexOf(s.checkout_date)
                const span = Math.max(endEx - ci, 1)
                return (
                  <div
                    key={s.reservation_id}
                    className={`stay ${s.status}`}
                    style={{ gridColumn: `${ci + 2} / span ${span}`, gridRow: rowIdx }}
                    onClick={(e) => { e.stopPropagation(); setDetailId(s.reservation_id) }}
                  >
                    {isHk ? '●' : (s.guest_name ?? `#${s.reservation_id.slice(0, 6)}`)}
                    {!isHk && ` · ${fmtMoney(s.total_price_cents, s.currency)}`}
                  </div>
                )
              })}
            </div>
          )
        })}
      </div>

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

// --------------------------------------------------------------- create modal
function NewReservationModal(props: {
  prefill: { roomId: string; date: string }
  rooms: BoardRoom[]
  channels: ChannelDto[]
  guests: GuestDto[]
  onClose: () => void
  onCreated: (r: ReservationDto) => void
}) {
  const { t } = useTranslation()
  const { session } = useAuth()
  const [roomId, setRoomId] = useState(props.prefill.roomId)
  const [checkin, setCheckin] = useState(props.prefill.date)
  const [checkout, setCheckout] = useState(iso(new Date(new Date(props.prefill.date + 'T00:00:00Z').getTime() + DAY_MS)))
  const [channelId, setChannelId] = useState(props.channels[0]?.id ?? '')
  const [guestId, setGuestId] = useState('')
  const [adults, setAdults] = useState(1)
  const [children, setChildren] = useState(0)
  const [voucher, setVoucher] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function submit(e: React.FormEvent) {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      const r = await api.createReservation({
        room_id: roomId,
        channel_id: channelId,
        guest_id: guestId || null,
        checkin_date: checkin,
        checkout_date: checkout,
        adults,
        children,
        voucher_number: voucher || undefined,
      })
      props.onCreated(r)
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
      void session
    }
  }

  return (
    <div className="modal-back" onClick={props.onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>{t('res.create')}</h3>
        <form onSubmit={submit} style={{ display: 'grid', gap: 10 }}>
          <label>Room
            <select value={roomId} onChange={(e) => setRoomId(e.target.value)} style={{ width: '100%' }}>
              {props.rooms.map((r) => <option key={r.id} value={r.id}>{r.room_number} ({r.room_type_name})</option>)}
            </select>
          </label>
          <div className="row">
            <label>{t('res.checkin')}<input type="date" value={checkin} onChange={(e) => setCheckin(e.target.value)} required style={{ width: '100%' }} /></label>
            <label>{t('res.checkout')}<input type="date" value={checkout} onChange={(e) => setCheckout(e.target.value)} required style={{ width: '100%' }} /></label>
          </div>
          <label>{t('res.channel')}
            <select value={channelId} onChange={(e) => setChannelId(e.target.value)} style={{ width: '100%' }} required>
              {props.channels.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
            </select>
          </label>
          <label>{t('res.guest')}
            <select value={guestId} onChange={(e) => setGuestId(e.target.value)} style={{ width: '100%' }}>
              <option value="">— {t('res.guest')} —</option>
              {props.guests.map((g) => <option key={g.id} value={g.id}>{g.full_name}</option>)}
            </select>
          </label>
          <div className="row">
            <label>{t('res.adults')}<input type="number" min={1} value={adults} onChange={(e) => setAdults(Number(e.target.value))} style={{ width: 80 }} /></label>
            <label>{t('res.children')}<input type="number" min={0} value={children} onChange={(e) => setChildren(Number(e.target.value))} style={{ width: 80 }} /></label>
            <label>{t('res.voucher')}<input value={voucher} onChange={(e) => setVoucher(e.target.value)} /></label>
          </div>
          {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}
          <div className="row">
            <button className="primary" disabled={busy} type="submit">{busy ? t('common.loading') : t('res.create')}</button>
            <button type="button" onClick={props.onClose}>{t('common.cancel')}</button>
          </div>
        </form>
      </div>
    </div>
  )
}

// --------------------------------------------------------------- detail modal
function ReservationDetailModal(props: { id: string; onClose: () => void; onChanged: () => void }) {
  const { t } = useTranslation()
  const { session } = useAuth()
  const can = (perm: string) => sessionHasPermission(session, perm)
  const [res, setRes] = useState<ReservationDto | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [payAmount, setPayAmount] = useState('')
  const [payType, setPayType] = useState('cash')
  const [folioCat, setFolioCat] = useState('minibar')
  const [folioDesc, setFolioDesc] = useState('')
  const [folioPrice, setFolioPrice] = useState('')
  const [folioQty, setFolioQty] = useState(1)
  const [payments, setPayments] = useState<{ id: string; amount_cents: number; payment_type: string; is_reversal: boolean }[]>([])
  const [folio, setFolio] = useState<{ id: string; category: string; description: string; total_cents: number }[]>([])

  async function refresh() {
    try {
      setRes(await api.getReservation(props.id))
      setPayments(await api.listPayments(props.id))
      setFolio(await api.listFolio(props.id))
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    }
  }
  useEffect(() => { void refresh() }, [props.id])

  async function status(action: 'checkIn' | 'checkOut' | 'cancelReservation') {
    if (action === 'cancelReservation' && !confirm(t('res.cancel') + '?')) return
    try {
      setError(null)
      await api[action](res!.id)
      await refresh()
      props.onChanged()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    }
  }

  if (!res) {
    return (
      <div className="modal-back" onClick={props.onClose}>
        <div className="modal">
          {error ? <p style={{ color: 'red' }}>{t('common.error')}: {error}</p> : <p>{t('common.loading')}</p>}
          <button onClick={props.onClose}>{t('common.back')}</button>
        </div>
      </div>
    )
  }

  return (
    <div className="modal-back" onClick={props.onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>{t('res.list')} #{res.id.slice(0, 8)}</h3>
        <p>
          <b>{t('res.status')}:</b> {t(`res.${res.status}`)} · <b>{t('res.checkin')}:</b> {res.checkin_date} · <b>{t('res.checkout')}:</b> {res.checkout_date}
          {' '}· <b>{t('res.total')}:</b> {fmtMoney(res.total_price_cents, res.currency)}
        </p>
        {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}

        <div className="row" style={{ marginBottom: 12 }}>
          {can('reservation_check_in') && res.status === 'confirmed' && <button className="primary" onClick={() => void status('checkIn')}>{t('res.checkin')}</button>}

          {can('reservation_check_out') && res.status === 'checked_in' && <button className="primary" onClick={() => void status('checkOut')}>{t('res.checkout')}</button>}
          {can('reservation_cancel') && (res.status === 'confirmed' || res.status === 'conflict') && <button className="danger" onClick={() => void status('cancelReservation')}>{t('res.cancel')}</button>}
        </div>

        {/* payments */}
        {can('financial_view') || can('payment_record') ? (
          <div className="card" style={{ marginBottom: 10 }}>
            <h4 style={{ margin: '0 0 8px' }}>{t('pay.title')}</h4>
            {payments.length === 0 && <p className="muted">—</p>}
            {payments.map((p) => (
              <div key={p.id} className="row" style={{ justifyContent: 'space-between' }}>
                <span>{t(`pay.${p.payment_type === 'credit_card' ? 'creditCard' : p.payment_type}`)} {p.is_reversal ? `(${t('pay.refund')})` : ''}</span>
                <span style={{ color: p.is_reversal ? '#e5484d' : undefined }}>{fmtMoney(p.amount_cents, res.currency)}</span>
                {!p.is_reversal && can('payment_refund') && (
                  <button className="ghost" onClick={async () => { await api.reversePayment(p.id).then(refresh) }}>{t('pay.refund')}</button>
                )}
              </div>
            ))}
            {can('payment_record') && (res.status === 'confirmed' || res.status === 'checked_in') && (
              <form className="row" style={{ marginTop: 8 }} onSubmit={async (e) => {
                e.preventDefault()
                await api.addPayment(res.id, { amount_cents: Math.round(Number(payAmount) * 100), payment_type: payType }).then(async () => { setPayAmount(''); await refresh() })
              }}>
                <input type="number" placeholder={t('pay.amount')} value={payAmount} onChange={(e) => setPayAmount(e.target.value)} required style={{ width: 110 }} />
                <select value={payType} onChange={(e) => setPayType(e.target.value)}>
                  <option value="cash">{t('pay.cash')}</option>
                  <option value="credit_card">{t('pay.creditCard')}</option>
                  <option value="bank_transfer">{t('pay.bank')}</option>
                  <option value="agency">{t('pay.agency')}</option>
                </select>
                <button className="primary" type="submit">{t('pay.record')}</button>
              </form>
            )}
          </div>
        ) : null}

        {/* folio */}
        {can('folio_manage') ? (
          <div className="card">
            <h4 style={{ margin: '0 0 8px' }}>{t('folio.title')}</h4>
            {folio.length === 0 && <p className="muted">—</p>}
            {folio.map((f) => (
              <div key={f.id} className="row" style={{ justifyContent: 'space-between' }}>
                <span>{f.category} · {f.description}</span>
                <b>{fmtMoney(f.total_cents, res.currency)}</b>
              </div>
            ))}
            <form className="row" style={{ marginTop: 8 }} onSubmit={async (e) => {
              e.preventDefault()
              await api.addFolio(res.id, { category: folioCat, description: folioDesc, unit_price_cents: Math.round(Number(folioPrice) * 100), quantity: folioQty })
                .then(async () => { setFolioDesc(''); setFolioPrice(''); await refresh() })
            }}>
              <select value={folioCat} onChange={(e) => setFolioCat(e.target.value)}>
                <option value="room">{t('folio.category_room')}</option>
                <option value="minibar">{t('folio.category_minibar')}</option>
                <option value="spa">{t('folio.category_spa')}</option>
                <option value="restaurant">{t('folio.category_restaurant')}</option>
                <option value="extra">{t('folio.category_extra')}</option>
              </select>
              <input placeholder={t('folio.desc')} value={folioDesc} onChange={(e) => setFolioDesc(e.target.value)} required />
              <input type="number" step="0.01" placeholder={t('folio.unitPrice')} value={folioPrice} onChange={(e) => setFolioPrice(e.target.value)} required style={{ width: 100 }} />
              <input type="number" min={1} value={folioQty} onChange={(e) => setFolioQty(Number(e.target.value))} style={{ width: 60 }} />
              <button className="primary" type="submit">{t('folio.add')}</button>
            </form>
          </div>
        ) : null}

        <div className="row" style={{ marginTop: 12 }}>
          <button onClick={props.onClose}>{t('common.back')}</button>
        </div>
      </div>
    </div>
  )
}
