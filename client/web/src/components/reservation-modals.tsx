// Shared reservation modals (5b.3): the board and the dashboard both need
// drag/click-to-create and a reservation detail view, so they live here.
import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { api, sessionHasPermission, ApiError } from '../api/client'
import { useAuth } from '../api/auth'
import type { BoardRoom, ChannelDto, GuestDto, ReservationDto } from '../types/generated/api'

export const DAY_MS = 86_400_000
export const iso = (d: Date) => d.toISOString().slice(0, 10)
export const addDays = (date: string, n: number) =>
  iso(new Date(new Date(date + 'T00:00:00Z').getTime() + n * DAY_MS))
export const fmtMoney = (cents: number, currency: string) =>
  new Intl.NumberFormat(undefined, { style: 'currency', currency, maximumFractionDigits: 0 }).format(cents / 100)

// --------------------------------------------------------------- create modal
export function NewReservationModal(props: {
  prefill: { roomId: string; date: string; nights: number }
  rooms: BoardRoom[]
  channels: ChannelDto[]
  guests: GuestDto[]
  onClose: () => void
  onCreated: (r: ReservationDto) => void
}) {
  const { t } = useTranslation()
  const [roomId, setRoomId] = useState(props.prefill.roomId)
  const [checkin, setCheckin] = useState(props.prefill.date)
  const [checkout, setCheckout] = useState(addDays(props.prefill.date, props.prefill.nights))
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
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
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
// Booking detail page parity (app.hmsflow reference): header with
// Check-In/Check-Out actions, "Booking Details" card on the left and the
// merged financial card on the right (Others menu / Add Payment / balance
// breakdown), then Account Transactions / Guests / Collection-Payment /
// Transactions / Day Rollover tabs.

type PayRow = {
  id: string; amount_cents: number; currency?: string; payment_type?: string
  is_reversal?: boolean | number
}
type FolioRow = {
  id: string; category: string; description: string
  total_cents: number; unit_price_cents?: number; quantity?: number; currency?: string
}
type GuestRow = {
  id: string; full_name: string; nationality?: string | null; license_plate?: string | null
}

const DAY_NAMES = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']
const fmtDate = (d: string) => {
  const dt = new Date(d + 'T00:00:00Z')
  return `${String(dt.getUTCDate()).padStart(2, '0')}.${String(dt.getUTCMonth() + 1).padStart(2, '0')}.${dt.getUTCFullYear()} ${DAY_NAMES[dt.getUTCDay()]}`
}
const fmtDateTime = (v: string | null | undefined) => {
  if (!v) return null
  const dt = new Date(v)
  if (Number.isNaN(dt.getTime())) return null
  const p = (n: number) => String(n).padStart(2, '0')
  return `${p(dt.getDate())}.${p(dt.getMonth() + 1)}.${dt.getFullYear()} ${p(dt.getHours())}:${p(dt.getMinutes())}`
}
const fmtMoney2 = (cents: number, currency: string) => {
  const v = new Intl.NumberFormat('tr-TR', { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(cents / 100)
  return `${v} ${currency}`
}

export function ReservationDetailModal(props: {
  id: string
  onClose: () => void
  onChanged: () => void
  onReassign?: (r: ReservationDto) => void
  onNewReservation?: () => void
}) {
  const { t } = useTranslation()
  const { session } = useAuth()
  const can = (perm: string) => sessionHasPermission(session, perm)
  const [res, setRes] = useState<ReservationDto | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [payments, setPayments] = useState<PayRow[]>([])
  const [folio, setFolio] = useState<FolioRow[]>([])
  const [guests, setGuests] = useState<GuestRow[]>([])
  const [roomInfo, setRoomInfo] = useState<{ number: string; type: string } | null>(null)
  const [channelName, setChannelName] = useState<string | null>(null)
  const [tab, setTab] = useState<'ledger' | 'guests' | 'payments' | 'charges'>('ledger')
  const [othersOpen, setOthersOpen] = useState(false)
  const [busy, setBusy] = useState(false)

  // payment / guest / folio form state
  const [payAmount, setPayAmount] = useState('')
  const [payType, setPayType] = useState('cash')
  const [folioCat, setFolioCat] = useState('room')
  const [folioDesc, setFolioDesc] = useState('')
  const [folioPrice, setFolioPrice] = useState('')
  const [folioQty, setFolioQty] = useState(1)
  const [gName, setGName] = useState('')
  const [gNat, setGNat] = useState('')
  const [editDates, setEditDates] = useState(false)
  const [editIn, setEditIn] = useState('')
  const [editOut, setEditOut] = useState('')

  const refresh = useCallback(async function refresh() {
    try {
      const r = await api.getReservation(props.id)
      setRes(r)
      const [pay, fol, gst, rooms, types] = await Promise.all([
        api.listPayments(props.id).catch(() => [] as PayRow[]),
        api.listFolio(props.id).catch(() => [] as FolioRow[]),
        api.listReservationGuests(props.id).catch(() => [] as Array<Record<string, unknown>>),
        api.listRooms().catch(() => [] as Array<Record<string, unknown>>),
        api.listRoomTypes().catch(() => [] as Array<Record<string, unknown>>),
      ])
      setPayments(pay)
      setFolio(fol)
      setGuests(gst.map((g) => ({
        id: String(g.id ?? ''),
        full_name: String(g.full_name ?? ''),
        nationality: g.nationality as string | null,
        license_plate: g.license_plate as string | null,
      })))
      const room = rooms.find((x) => String(x.id) === String(r.room_id))
      if (room) {
        const ty = types.find((x) => String(x.id) === String(room.room_type_id))
        setRoomInfo({
          number: String(room.room_number ?? ''),
          type: ty ? String(ty.name ?? '') : '',
        })
      } else {
        setRoomInfo(null)
      }
      void api.listChannels().then((cs) => {
        setChannelName(cs.find((c) => c.id === r.channel_id)?.name ?? null)
      }).catch(() => setChannelName(null))
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    }
  }, [props.id])
  useEffect(() => { void refresh() }, [refresh])
  useEffect(() => {
    if (!othersOpen) return
    const off = () => setOthersOpen(false)
    window.addEventListener('click', off)
    return () => window.removeEventListener('click', off)
  }, [othersOpen])

  const currency = res?.currency ?? 'TRY'
  const nights = res
    ? Math.max(1, Math.round((new Date(res.checkout_date + 'T00:00:00Z').getTime() - new Date(res.checkin_date + 'T00:00:00Z').getTime()) / DAY_MS))
    : 0
  const total = res?.total_price_cents ?? 0
  const pricePerDay = nights > 0 ? Math.round(total / nights) : total
  const collected = payments.reduce(
    (s, p) => s + ((p.is_reversal ? -1 : 1) * p.amount_cents), 0,
  )
  const balance = total - collected

  async function status(action: 'checkIn' | 'checkOut' | 'cancelReservation') {
    if (action === 'cancelReservation' && !confirm(t('res.cancel') + '?')) return
    try {
      setBusy(true); setError(null)
      await api[action](res!.id)
      await refresh()
      props.onChanged()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  async function saveDates() {
    try {
      setBusy(true); setError(null)
      await api.saveDomainRecord('reservations', res!.id, {
        room_id: res!.room_id, checkin_date: editIn, checkout_date: editOut, status: res!.status,
      })
      setEditDates(false)
      await refresh(); props.onChanged()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  async function addPayment() {
    const cents = Math.round(Number(payAmount) * 100)
    if (!cents) return
    try {
      setBusy(true); setError(null)
      await api.addPayment(res!.id, { amount_cents: cents, payment_type: payType, currency })
      setPayAmount('')
      await refresh(); props.onChanged()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  async function addFolioLine() {
    try {
      setBusy(true); setError(null)
      await api.addFolio(res!.id, {
        category: folioCat, description: folioDesc,
        unit_price_cents: Math.round(Number(folioPrice) * 100), quantity: folioQty,
      })
      setFolioDesc(''); setFolioPrice('')
      await refresh(); props.onChanged()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  async function addGuest() {
    if (!gName.trim()) return
    try {
      setBusy(true); setError(null)
      const g = await api.createGuest({ full_name: gName.trim(), nationality: gNat.trim() || undefined })
      // attach to the reservation (link list; primary only when none yet)
      const ids = [...new Set([res!.guest_id, ...guests.map((x) => x.id)].filter(Boolean))] as string[]
      const nextIds = ids.includes(g.id) ? ids : [...ids, g.id]
      await api.saveDomainRecord('reservations', res!.id, {
        room_id: res!.room_id, checkin_date: res!.checkin_date, checkout_date: res!.checkout_date,
        status: res!.status, guest_ids: nextIds,
      })
      setGName(''); setGNat('')
      await refresh(); props.onChanged()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  async function removeGuest(gid: string) {
    if (!confirm('Remove this guest from the booking?')) return
    try {
      setBusy(true); setError(null)
      const ids = guests.map((x) => x.id).filter((x) => x !== gid)
      await api.saveDomainRecord('reservations', res!.id, {
        room_id: res!.room_id, checkin_date: res!.checkin_date, checkout_date: res!.checkout_date,
        status: res!.status, guest_ids: ids, guest_id: ids[0] ?? null,
      })
      await refresh(); props.onChanged()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  async function reverseLastPayment() {
    const last = [...payments].reverse().find((p) => !p.is_reversal)
    if (!last) { setError('No payment to return'); return }
    try {
      setBusy(true); setError(null)
      await api.reversePayment(last.id)
      await refresh(); props.onChanged()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  if (!res) {
    return (
      <div className="modal-back bk-back" onClick={props.onClose}>
        <div className="modal bk-modal">
          {error ? <p style={{ color: 'red' }}>{t('common.error')}: {error}</p> : <p>{t('common.loading')}</p>}
          <button onClick={props.onClose}>{t('common.back')}</button>
        </div>
      </div>
    )
  }

  const statusLabel = t(`res.${res.status}`)
  const canPay = can('payment_record') && (res.status === 'confirmed' || res.status === 'checked_in')
  const canFolio = can('folio_manage')
  const others = [
    { key: 'newres', label: 'Add New Reservation', show: !!props.onNewReservation, act: () => props.onNewReservation!() },
    { key: 'invoice', label: 'Sale Invoice', show: true, act: () => { try { window.print() } catch { /* webview */ } } },
    { key: 'addguest', label: 'Add Guest', show: true, act: () => setTab('guests') },
    { key: 'addtx', label: 'Add Transaction', show: canFolio, act: () => setTab('charges') },
    { key: 'addpay', label: 'Add Payment', show: canPay, act: () => setTab('payments') },
    { key: 'returnpay', label: 'Payment (Return) Add', show: canPay, act: () => void reverseLastPayment() },
    { key: 'cancel', label: 'Cancel Booking', show: can('reservation_cancel') && (res.status === 'confirmed' || res.status === 'conflict'), act: () => void status('cancelReservation') },
  ].filter((o) => o.show)

  // combined ledger for the Account Transactions tab
  const ledgerRows = [
    ...payments.map((p) => ({
      id: p.id, kind: 'payment' as const, date: '', memo: paymentLabel(p), amount: (p.is_reversal ? -1 : 1) * p.amount_cents,
      note: `Payment${p.payment_type ? ` - ${payTypeName(p)}` : ''}`,
    })),
    ...folio.map((f) => ({ id: f.id, kind: 'charge' as const, date: '', memo: chargeLabel(f), amount: f.total_cents, note: `Charge${f.quantity && f.quantity !== 1 ? ` x${f.quantity}` : ''}` })),
  ]

  return (
    <div className="modal-back bk-back" onClick={props.onClose}>
      <div className="modal bk-modal" onClick={(e) => e.stopPropagation()}>
        <div className="bk-head">
          <button className="bk-backbtn" onClick={props.onClose} aria-label="Back">&#8592;</button>
          <h2 className="bk-title">Booking #{res.id.slice(0, 8)}</h2>
          <span className={`pill pill-${res.status}`}>{statusLabel}</span>
          <div className="bk-headbtns">
            <button className="bk-btn" onClick={() => { try { window.print() } catch { /* webview */ } }}>{t('print', 'Print')} &#9662;</button>
            <button className="bk-btn" onClick={() => { setEditDates((v) => !v); if (!editDates) { setEditIn(res.checkin_date); setEditOut(res.checkout_date) } }}>
              &#9998; Edit
            </button>
            {can('reservation_check_in') && res.status === 'confirmed' && (
              <button className="bk-btn bk-primary" disabled={busy} onClick={() => void status('checkIn')}>&#8594; Check-in</button>
            )}
            {can('reservation_check_out') && res.status === 'checked_in' && (
              <button className="bk-btn bk-primary" disabled={busy} onClick={() => void status('checkOut')}>&#8594; Check-out</button>
            )}
            {res.status === 'conflict' && props.onReassign && (
              <button className="bk-btn bk-warn" onClick={() => props.onReassign!(res)}>&#9888; Reassign</button>
            )}
          </div>
        </div>
        {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}

        <div className="bk-top">
          {/* Booking Details */}
          <div className="bk-card bk-details">
            <h4>Booking Details</h4>
            <div className="bk-grid">
              <span className="bk-label">Room</span><b className="bk-val">{roomInfo?.number ?? res.room_id.slice(0, 8)}</b>
              <span className="bk-label">Room Type</span><b className="bk-val">{roomInfo?.type || '—'}</b>
              <span className="bk-label">Accommodation Type</span><b className="bk-val">{res.notes || 'Room + Breakfast'}</b>
              <span className="bk-label">Booking Type</span><b className="bk-val">Reservation</b>
              <span className="bk-label">Number of Guests</span>
              <b className="bk-val">{res.adults} adults, {res.children} children</b>
              {editDates ? (
                <>
                  <span className="bk-label">{t('res.checkin')}</span>
                  <span className="bk-val"><input type="date" value={editIn} onChange={(e) => setEditIn(e.target.value)} /></span>
                  <span className="bk-label">{t('res.checkout')}</span>
                  <span className="bk-val row" style={{ gap: 6 }}>
                    <input type="date" value={editOut} onChange={(e) => setEditOut(e.target.value)} />
                    <button className="bk-btn bk-primary" disabled={busy} onClick={() => void saveDates()}>Save</button>
                  </span>
                  <span className="bk-label">Channel</span><b className="bk-val">{channelName ?? '—'}</b>
                </>
              ) : (
                <>
                  <span className="bk-label">{t('res.checkin')}</span>
                  <b className="bk-val">{fmtDate(res.checkin_date)} 14:00</b>
                  <span className="bk-label">{t('res.checkout')}</span>
                  <b className="bk-val">{fmtDate(res.checkout_date)} 12:00</b>
                  <span className="bk-label">Agency</span><b className="bk-val">{channelName ?? '—'}</b>
                </>
              )}
              <span className="bk-label">Voucher No</span><b className="bk-val">{res.voucher_number || '—'}</b>
              <span className="bk-label">Channel Booking No</span><b className="bk-val">—</b>
              <span className="bk-label">Registration Date</span>
              <b className="bk-val">{fmtDateTime(res.created_at) ?? '—'}</b>
              <span className="bk-label">Created By</span><b className="bk-val">—</b>
            </div>
          </div>

          {/* Amounts — merged with the built-in payment box */}
          <div className="bk-card bk-amounts">
            <h4>Amounts</h4>
            <div className="bk-ops">
              <div className="bk-dropdown" onClick={(e) => e.stopPropagation()}>
                <button className="bk-btn" onClick={() => setOthersOpen((v) => !v)}>Others &#9662;</button>
                {othersOpen && others.length > 0 && (
                  <div className="bk-menu">
                    {others.map((o) => (
                      <button key={o.key} className="bk-menuitem" onClick={() => { setOthersOpen(false); o.act() }}>{o.label}</button>
                    ))}
                  </div>
                )}
              </div>
              {canPay && (
                <button className="bk-btn bk-pay" onClick={() => setTab('payments')}>&#10148; Add Payment</button>
              )}
            </div>
            <div className="bk-balrow">
              <span className={balance > 0 ? 'bk-bal bk-bal-due' : 'bk-bal bk-bal-zero'}>Balance</span>
              <span className={balance > 0 ? 'bk-balnum bk-bal-due' : 'bk-balnum bk-bal-zero'}>{fmtMoney2(balance, currency)}</span>
            </div>
            <div className="bk-bar"><div className="bk-barfill" style={{ width: `${total > 0 ? Math.max(0, Math.min(100, (collected / total) * 100)) : 0}%` }} /></div>
            <div className="bk-fin">
              <span>Price per day</span><b>{fmtMoney2(pricePerDay, currency)}</b>
              <span>Night</span><b>{nights}</b>
              <span>Accommodation</span><b>{fmtMoney2(total, currency)}</b>
              <span>Collection</span><b>{fmtMoney2(collected, currency)}</b>
              <span>Grand Total</span><b>{fmtMoney2(total, currency)}</b>
              <span>Exchange Rate</span><b>1</b>
            </div>
          </div>
        </div>

        {/* tabs */}
        <div className="bk-tabs">
          {([
            ['ledger', 'Account Transactions', 0],
            ['guests', 'Guests', guests.length],
            ['payments', 'Collection / Payment', payments.length],
            ['charges', 'Transactions', folio.length],
          ] as const).map(([key, label, count]) => (
            <button key={key} className={`bk-tab${tab === key ? ' on' : ''}`} onClick={() => setTab(key)}>
              {label}{count ? <i>{count}</i> : null}
            </button>
          ))}
        </div>

        {tab === 'ledger' && <LedgerTab rows={ledgerRows} currency={currency} />}
        {tab === 'guests' && (
          <div className="bk-tabpane">
            <div className="row" style={{ justifyContent: 'flex-end', marginBottom: 8 }}>
              <button className="bk-btn bk-primary bk-addguest" onClick={() => void addGuest()} disabled={busy}>&#128101; Add Guest</button>
            </div>
            <form className="row" style={{ marginBottom: 10, flexWrap: 'wrap' }} onSubmit={(e) => { e.preventDefault(); void addGuest() }}>
              <input placeholder="Guest full name" value={gName} onChange={(e) => setGName(e.target.value)} required style={{ width: 220 }} />
              <input placeholder="Country / nationality" value={gNat} onChange={(e) => setGNat(e.target.value)} style={{ width: 180 }} />
              <button className="bk-btn" type="submit" disabled={busy}>Save guest</button>
            </form>
            <table className="hms bk-table">
              <thead><tr><th>Guest</th><th>Country</th><th>ID No</th><th>Check-in</th><th>Check-out</th><th></th></tr></thead>
              <tbody>
                {guests.length === 0 && <tr><td colSpan={6} className="muted" style={{ textAlign: 'center' }}>{t('reports.noData')}</td></tr>}
                {guests.map((g) => (
                  <tr key={g.id}>
                    <td><b>{g.full_name}</b></td>
                    <td className="muted">{g.nationality || '—'}</td>
                    <td className="muted">{g.license_plate || '—'}</td>
                    <td>{fmtDateTime2(res.checkin_date)}</td>
                    <td>{fmtDateTime2(res.checkout_date)}</td>
                    <td><button className="bk-btn" onClick={() => void removeGuest(g.id)} disabled={busy}>Remove</button></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {tab === 'payments' && (
          <div className="bk-tabpane">
            {payments.length === 0 && <p className="muted">—</p>}
            <table className="hms bk-table">
              <thead><tr><th>Payment</th><th>Type</th><th>Amount</th><th></th></tr></thead>
              <tbody>
                {payments.map((p) => (
                  <tr key={p.id}>
                    <td>{p.is_reversal ? `${payTypeName(p)} (Return)` : payTypeName(p)}</td>
                    <td className="muted">{p.payment_type ? t(`pay.${p.payment_type === 'credit_card' ? 'creditCard' : p.payment_type}`) : '—'}</td>
                    <td style={{ color: p.is_reversal ? '#e5484d' : undefined }}>{fmtMoney2(p.amount_cents, currency)}</td>
                    <td>{!p.is_reversal && can('payment_refund') && (
                      <button className="bk-btn" onClick={() => void api.reversePayment(p.id).then(refresh).catch(() => {})}>{t('pay.refund')}</button>
                    )}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {canPay && (
              <form className="row" style={{ marginTop: 10 }} onSubmit={(e) => { e.preventDefault(); void addPayment() }}>
                <input type="number" placeholder={t('pay.amount')} value={payAmount} onChange={(e) => setPayAmount(e.target.value)} required style={{ width: 120 }} />
                <select value={payType} onChange={(e) => setPayType(e.target.value)}>
                  <option value="cash">{t('pay.cash')}</option>
                  <option value="credit_card">{t('pay.creditCard')}</option>
                  <option value="bank_transfer">{t('pay.bank')}</option>
                  <option value="agency">{t('pay.agency')}</option>
                </select>
                <button className="bk-btn bk-primary" type="submit" disabled={busy}>{t('pay.record')}</button>
              </form>
            )}
          </div>
        )}
        {tab === 'charges' && (
          <div className="bk-tabpane">
            {folio.length === 0 && <p className="muted">—</p>}
            <table className="hms bk-table">
              <thead><tr><th>Category</th><th>Description</th><th>Qty</th><th>Total</th></tr></thead>
              <tbody>
                {folio.map((f) => (
                  <tr key={f.id}>
                    <td>{f.category}</td>
                    <td>{f.description}</td>
                    <td className="muted">{f.quantity ?? 1}</td>
                    <td>{fmtMoney2(f.total_cents, currency)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {canFolio && (
              <form className="row" style={{ marginTop: 10, flexWrap: 'wrap' }} onSubmit={(e) => { e.preventDefault(); void addFolioLine() }}>
                <select value={folioCat} onChange={(e) => setFolioCat(e.target.value)}>
                  <option value="room">{t('folio.category_room')}</option>
                  <option value="minibar">{t('folio.category_minibar')}</option>
                  <option value="spa">{t('folio.category_spa')}</option>
                  <option value="restaurant">{t('folio.category_restaurant')}</option>
                  <option value="extra">{t('folio.category_extra')}</option>
                </select>
                <input placeholder={t('folio.desc')} value={folioDesc} onChange={(e) => setFolioDesc(e.target.value)} required />
                <input type="number" step="0.01" placeholder={t('folio.unitPrice')} value={folioPrice} onChange={(e) => setFolioPrice(e.target.value)} required style={{ width: 110 }} />
                <input type="number" min={1} value={folioQty} onChange={(e) => setFolioQty(Number(e.target.value))} style={{ width: 64 }} />
                <button className="bk-btn bk-primary" type="submit" disabled={busy}>{t('folio.add')}</button>
              </form>
            )}
          </div>
        )}
      </div>
    </div>
  )
}

function payTypeName(p: PayRow): string {
  switch (p.payment_type) {
    case 'credit_card': return 'Credit Card'
    case 'bank_transfer': return 'Bank Transfer'
    case 'agency': return 'Agency'
    default: return 'Cash'
  }
}
function chargeLabel(f: FolioRow): string {
  const cat = f.category ? f.category.charAt(0).toUpperCase() + f.category.slice(1) : 'Charge'
  return f.description ? `${cat} - ${f.description}` : cat
}
function paymentLabel(_p: PayRow): string { return 'Payment' }

function LedgerTab(props: {
  rows: Array<{ id: string; kind: 'payment' | 'charge'; memo: string; amount: number; note: string }>
  currency: string
}) {
  return (
    <div className="bk-tabpane">
      <table className="hms bk-table">
        <thead><tr><th>Type</th><th>Description</th><th></th><th>Amount</th></tr></thead>
        <tbody>
          {props.rows.length === 0 && <tr><td colSpan={4} className="muted" style={{ textAlign: 'center' }}>No transactions</td></tr>}
          {props.rows.map((r) => (
            <tr key={r.id + r.kind}>
              <td><b>{r.kind === 'payment' ? 'Payment' : 'Charge'}</b></td>
              <td>{r.memo}</td>
              <td className="muted">{r.note}</td>
              <td>{fmtMoney2(r.amount, props.currency)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

// small date-only formatter without the weekday for table rows
function fmtDateTime2(d: string): string {
  const dt = new Date(d + 'T00:00:00Z')
  if (Number.isNaN(dt.getTime())) return d
  const p = (n: number) => String(n).padStart(2, '0')
  return `${p(dt.getUTCDate())}.${p(dt.getUTCMonth() + 1)}.${dt.getUTCFullYear()}`
}
