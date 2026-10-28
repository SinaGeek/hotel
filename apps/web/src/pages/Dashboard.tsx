// Dashboard (Phase 5b.3, reference parity per docs/ui-reference.md):
// suggestion banner, "Search room or guest" box, status chip row with counts,
// room-card grid with state-colored headers (occupied / will check-in / out
// of service / vacant) and "Make Reservation" on vacant cards, stat cards.
//
// Data sources, honestly mapped:
// - /board for today: per-room stays covering tonight (board uses half-open
//   dates, so an in-house guest with checkout == today is NOT in this list)
// - /reservations?status=checked_in in-house list: guests still physically in
//   the house (checkout >= today), incl. today's departures -> Guests Today
//   stat + the Departing Today/Tomorrow/Long Term chips (adults+children sum)
// - /guests for name lookup of reservations whose guest_name the board lacks.
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { api, sessionHasPermission, ApiError } from '../api/client'
import { useAuth } from '../api/auth'
import { NewReservationModal, ReservationDetailModal, iso } from '../components/reservation-modals'
import type { BoardResponse, BoardStay, ChannelDto, GuestDto, ReservationDto, RoomTypeDto } from '../types/generated/api'

const NIGHT_MS = 86_400_000
const addDays = (date: string, n: number) =>
  iso(new Date(new Date(date + 'T00:00:00Z').getTime() + n * NIGHT_MS))
const ddmm = (d: string) => `${d.slice(8)}.${d.slice(5, 7)}`
const nights = (checkin: string, checkout: string) =>
  Math.round((new Date(checkout + 'T00:00:00Z').getTime() - new Date(checkin + 'T00:00:00Z').getTime()) / NIGHT_MS)

type CardState = 'occupied' | 'checkin' | 'outs' | 'vacant'
type ChipId = 'departToday' | 'departTomorrow' | 'longTerm' | 'willCheckin' | 'outs' | 'dirty'
const CHIP_IDS: ChipId[] = ['departToday', 'departTomorrow', 'longTerm', 'willCheckin', 'outs', 'dirty']

// normalized active-resident view used by the room cards
type Active = { id: string; guest: string; checkout: string }

export function Dashboard() {
  const { t } = useTranslation()
  const { session } = useAuth()
  const isHk = session?.user.role === 'housekeeping'
  const canView = sessionHasPermission(session, 'reservation_view')

  const today = iso(new Date())
  const tomorrow = addDays(today, 1)

  const [board, setBoard] = useState<BoardResponse | null>(null)
  const [guestNames, setGuestNames] = useState<GuestDto[]>([])
  const [channels, setChannels] = useState<ChannelDto[]>([])
  const [roomTypes, setRoomTypes] = useState<RoomTypeDto[]>([])
  const [inhouse, setInhouse] = useState<ReservationDto[]>([])
  const [hotelName, setHotelName] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [reloadKey, setReloadKey] = useState(0)

  const [search, setSearch] = useState('')
  const [chip, setChip] = useState<ChipId | null>(null)
  const [creating, setCreating] = useState<string | null>(null)
  const [detailId, setDetailId] = useState<string | null>(null)
  const [thanked, setThanked] = useState(false)

  const hotelId = session?.user.hotel_id
  useEffect(() => {
    if (hotelId === undefined) return
    let alive = true
    api.listHotels()
      .then((hs) => { if (alive) setHotelName(hs.find((h) => h.id === hotelId)?.name ?? null) })
      .catch(() => { /* falls back to the app title */ })
    return () => { alive = false }
  }, [hotelId])

  useEffect(() => {
    void (async () => {
      try {
        setError(null)
        setBoard(await api.board(today, tomorrow))
        // in-house guests: checkout >= today. The list endpoint filters
        // checkout > from, so anchor `from` on yesterday; `to` one past
        // tomorrow so tomorrow's departures are listed too. Keyset-paginate.
        if (canView && !isHk) {
          const all: ReservationDto[] = []
          let after: string | undefined
          for (let i = 0; i < 5; i++) {
            const page = await api.listReservations({ status: 'checked_in', from: addDays(today, -1), to: addDays(tomorrow, 1), limit: 200, after })
            all.push(...page)
            if (page.length < 200) break
            after = page[page.length - 1].id
          }
          setInhouse(all)
        } else {
          setInhouse([])
        }
        setRoomTypes(await api.listRoomTypes().catch(() => []))
      } catch (e) {
        setError(e instanceof ApiError ? e.message : String(e))
      }
    })()
  }, [hotelId, reloadKey, today, tomorrow, canView, isHk])

  // guest + channel lists (guest list doubles as the name lookup and the
  // create-reservation picker; both non-fatal)
  useEffect(() => {
    if (!canView || isHk) return
    let alive = true
    api.listGuests('').then((gs) => { if (alive) setGuestNames(gs) }).catch(() => {})
    api.listChannels().then((cs) => { if (alive) setChannels(cs) }).catch(() => {})
    return () => { alive = false }
  }, [canView, isHk])

  const rooms = board?.rooms ?? []

  // per-room stays that cover today from the board (half-open: they were
  // checked in and are still resident tonight)
  const covers = useMemo(() => {
    const out = new Map<string, BoardStay[]>()
    for (const r of rooms) {
      const cs = r.stays.filter((s) =>
        (s.status === 'checked_in' || s.status === 'confirmed') &&
        s.checkout_date >= today && s.checkin_date <= today)
      cs.sort((a, b) => (a.status === 'checked_in' ? -1 : 1) - (b.status === 'checked_in' ? -1 : 1))
      out.set(r.id, cs)
    }
    return out
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [board, today])

  // in-house reservations (incl. guests whose checkout is today), by room
  const housesByRoom = useMemo(() => {
    const m = new Map<string, ReservationDto[]>()
    for (const r of inhouse) {
      if (r.checkin_date <= today && r.checkout_date >= today) {
        m.set(r.room_id, [...(m.get(r.room_id) ?? []), r])
      }
    }
    return m
  }, [inhouse, today])

  const nameOf = (r: ReservationDto): string => {
    if (r.guest_id == null) return `#${r.id.slice(0, 8)}`
    return guestNames.find((g) => g.id === r.guest_id)?.full_name ?? `#${r.guest_id.slice(0, 8)}`
  }

  // the resident shown on a card: a board stay that covers tonight, else an
  // in-house reservation whose checkout is today (leaving this morning), else
  // a confirmed arrival later today
  const activeFor = (roomId: string): Active | null => {
    const cs = covers.get(roomId) ?? []
    const s = cs.find((x) => x.status === 'checked_in') ?? cs.find((x) => x.status === 'confirmed' && x.checkin_date === today)
    if (s != null) {
      return { id: s.reservation_id, guest: isHk ? '●●●' : (s.guest_name ?? `#${s.reservation_id.slice(0, 8)}`), checkout: s.checkout_date }
    }
    const h = (housesByRoom.get(roomId) ?? []).sort((a, b) => (a.checkout_date < b.checkout_date ? 1 : -1))[0]
    if (h != null) {
      return { id: h.id, guest: isHk ? '●●●' : nameOf(h), checkout: h.checkout_date }
    }
    return null
  }

  const matchesChip = (roomId: string, c: ChipId): boolean => {
    const r = rooms.find((x) => x.id === roomId)
    if (!r) return false
    const house = housesByRoom.get(roomId) ?? []
    switch (c) {
      case 'departToday': return house.some((h) => h.checkout_date === today)
      case 'departTomorrow': return house.some((h) => h.checkout_date === tomorrow)
      case 'longTerm': return house.some((h) => nights(h.checkin_date, h.checkout_date) >= 7)
      case 'willCheckin': return (covers.get(roomId) ?? []).some((s) => s.status === 'confirmed' && s.checkin_date === today)
      case 'outs': return r.cleanliness === 'out_of_order'
      case 'dirty': return r.cleanliness === 'dirty'
    }
  }

  const cardState = (roomId: string): CardState => {
    const r = rooms.find((x) => x.id === roomId)
    if (!r) return 'vacant'
    if (r.cleanliness === 'out_of_order') return 'outs'
    const cs = covers.get(roomId) ?? []
    if (cs.some((s) => s.status === 'checked_in')) return 'occupied'
    if (cs.some((s) => s.status === 'confirmed' && s.checkin_date === today)) return 'checkin'
    if (housesByRoom.has(roomId)) return 'occupied' // in-house guest leaving today
    return 'vacant'
  }

  const chipCounts = useMemo(() => {
    const out = {} as Record<ChipId, number>
    for (const c of CHIP_IDS) out[c] = rooms.filter((r) => matchesChip(r.id, c)).length
    return out
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [board, covers, housesByRoom, today, tomorrow])

  const visibleRooms = useMemo(() => {
    const q = search.trim().toLowerCase()
    return rooms.filter((r) => {
      if (chip != null && !matchesChip(r.id, chip)) return false
      if (q === '') return true
      const guests = (covers.get(r.id) ?? []).map((s) => (s.guest_name ?? '').toLowerCase()).join(' ')
        + ' ' + (housesByRoom.get(r.id) ?? []).map(nameOf).join(' ')
      return r.room_number.toLowerCase().includes(q)
        || r.room_type_name.toLowerCase().includes(q)
        || guests.includes(q)
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rooms, covers, housesByRoom, chip, search, chipCounts])

  const occupied = rooms.filter((r) => cardState(r.id) === 'occupied').length
  const vacancies = rooms.filter((r) => cardState(r.id) === 'vacant').length
  const guestsToday = inhouse
    .filter((r) => r.checkin_date <= today && r.checkout_date >= today)
    .reduce((sum, r) => sum + r.adults + r.children, 0)

  if (!session?.user) return null

  return (
    <div className="page">
      {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}

      <div className="dash-top">
        <div className="suggest">
          <span>{t('dash.suggest')}</span>
          <button
            className="primary"
            onClick={() => { setThanked(true); window.setTimeout(() => setThanked(false), 3500) }}
          >
            {t('dash.sendSuggest')}
          </button>
          {thanked && <span className="dash-thanked">{t('dash.suggestThanks')}</span>}
        </div>
        <input
          className="dash-search"
          placeholder={t('dash.search')}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </div>

      <div className="chiprow">
        {CHIP_IDS.map((c) => (
          <button
            key={c}
            className={`chip${chip === c ? ' on' : ''}`}
            onClick={() => setChip((cur) => (cur === c ? null : c))}
          >
            {t(`dash.chip.${c}`)} <b>{chipCounts[c]}</b>
          </button>
        ))}
      </div>

      <div className="dash-hotel">
        <b>{hotelName ?? t('app.title')}</b>
        <span className="muted">{occupied} / {rooms.length} {t('dash.roomsUnit')}</span>
      </div>

      <div className="rc-grid">
        {visibleRooms.length === 0 && (
          <p className="muted" style={{ gridColumn: '1 / -1', textAlign: 'center', padding: 24 }}>{t('dash.noMatch')}</p>
        )}
        {visibleRooms.map((r) => {
          const st = cardState(r.id)
          const active = activeFor(r.id)
          const cap = roomTypes.find((rt) => rt.id === r.room_type_id)?.capacity
          return st === 'vacant' ? (
            <div key={r.id} className="rcard vacant">
              <div className="rc-head">
                <span>{r.room_number}</span>
                {cap != null && cap > 0 && <span className="cap-chip">{cap}</span>}
              </div>
              <div className="rc-body">
                <span className="rc-type">{r.room_type_name}</span>
                <span className={`dot ${r.cleanliness === 'dirty' ? 'bad' : r.cleanliness === 'cleaning' ? 'warn' : 'ok'}`} title={t(`rooms.${r.cleanliness}`)} />
                {!isHk && canView && (
                  <button className="rc-book" onClick={() => setCreating(r.id)}>{t('dash.makeRes')}</button>
                )}
              </div>
            </div>
          ) : (
            <div
              key={r.id}
              className={`rcard ${st}`}
              style={{ cursor: active != null && canView && !isHk ? 'pointer' : 'default' }}
              onClick={() => { if (active != null && canView && !isHk) setDetailId(active.id) }}
            >
              <div className="rc-head">
                <span>{r.room_number}</span>
                {active != null && <span className="rc-leaves">{t('dash.leavesOn', { d: ddmm(active.checkout) })}</span>}
              </div>
              <div className="rc-body">
                <span className="rc-guest">{active?.guest}</span>
                <span className="rc-type muted">{r.room_type_name}</span>
                {st === 'outs' && <span className="rc-state">{t('dash.chip.outs')}</span>}
              </div>
            </div>
          )
        })}
      </div>

      <div className="stat-grid">
        <div className="stat-card"><b>{occupied}</b><span>{t('dash.stat.occupied')}</span></div>
        {!isHk && <div className="stat-card"><b>{guestsToday}</b><span>{t('dash.stat.guests')}</span></div>}
        <div className="stat-card"><b>{vacancies}</b><span>{t('dash.stat.vacant')}</span></div>
        <div className="stat-card"><b>{rooms.length}</b><span>{t('dash.stat.total')}</span></div>
      </div>

      {creating != null && (
        <NewReservationModal
          prefill={{ roomId: creating, date: today, nights: 1 }}
          rooms={rooms}
          channels={channels}
          guests={guestNames}
          onClose={() => setCreating(null)}
          onCreated={(r) => { setCreating(null); setDetailId(r.id); setReloadKey((k) => k + 1) }}
        />
      )}
      {detailId != null && (
        <ReservationDetailModal id={detailId} onClose={() => setDetailId(null)} onChanged={() => setReloadKey((k) => k + 1)} />
      )}
    </div>
  )
}
