// Bookings list (Phase 5b.4, reference parity per docs/ui-reference.md):
// tabs Active/Upcoming/Archive/Cancelled/All mapped to server status filters,
// search + check-in/check-out date filters (server-side where supported),
// keyset-paged ListFooter with a real count, "+ Add Booking" create modal,
// row click opens the reservation detail.
import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { api, ApiError, sessionHasPermission } from '../api/client'
import { useAuth } from '../api/auth'
import { FilterBar, ListFooter, PageHeader, StatusPill } from '../components/ui'
import { NewReservationModal, ReservationDetailModal, fmtMoney } from '../components/reservation-modals'
import type { BoardRoom, ChannelDto, GuestDto, ReservationListQuery, ReservationDto, RoomTypeDto } from '../types/generated/api'

type Tab = 'active' | 'upcoming' | 'archive' | 'cancelled' | 'all'
const TABS: Tab[] = ['active', 'upcoming', 'archive', 'cancelled', 'all']
const TAB_QUERY: Record<Tab, ReservationListQuery> = {
  active: { status: 'checked_in' },
  upcoming: { status: 'confirmed' },
  archive: { status: 'checked_out' },
  cancelled: { status: 'cancelled' },
  all: {},
}

export function Reservations() {
  const { t } = useTranslation()
  const { session } = useAuth()
  const canCreate = sessionHasPermission(session, 'reservation_create') && !isHk(session)

  // reference data for guest/room labels + create modal selects
  const [rooms, setRooms] = useState<BoardRoom[]>([])
  const [guests, setGuests] = useState<GuestDto[]>([])
  const [roomTypes, setRoomTypes] = useState<RoomTypeDto[]>([])
  const [channels, setChannels] = useState<ChannelDto[]>([])

  const [tab, setTab] = useState<Tab>('active')
  const [search, setSearch] = useState('')
  const [dateFrom, setDateFrom] = useState('')
  const [dateTo, setDateTo] = useState('')
  const [perPage, setPerPage] = useState(20)

  const [rows, setRows] = useState<ReservationDto[]>([])
  const [total, setTotal] = useState<number | null>(null)
  // keyset paging: stack of the last id of every previously loaded page
  const [afterStack, setAfterStack] = useState<(string | undefined)[]>([])
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [creating, setCreating] = useState(false)
  const [detailId, setDetailId] = useState<string | null>(null)

  const query = useMemo<ReservationListQuery>(
    () => ({ ...TAB_QUERY[tab], from: dateFrom || undefined, to: dateTo || undefined }),
    [tab, dateFrom, dateTo],
  )

  useEffect(() => {
    let alive = true
    Promise.all([api.listRooms(), api.listRoomTypes(), api.listGuests(''), api.listChannels().catch(() => [])])
      .then(([rs, rts, gs, cs]) => {
        if (!alive) return
        setRooms(rs.map((r) => {
          const rt = rts.find((x) => x.id === r.room_type_id)
          return {
            id: r.id,
            room_number: r.room_number,
            room_type_id: r.room_type_id,
            room_type_name: rt?.name ?? '',
            cleanliness: r.cleanliness,
            stays: [],
            seasonal_price_cents: rt?.base_price_cents ?? 0,
          }
        }))
        setRoomTypes(rts)
        setGuests(gs)
        setChannels(cs)
      })
      .catch(() => { /* labels degrade to ids */ })
    return () => { alive = false }
  }, [])

  const loadPage = useCallback(async (after: string | undefined) => {
    setBusy(true)
    try {
      setError(null)
      const [page, count] = await Promise.all([
        api.listReservations({ ...query, limit: perPage, after }),
        api.reservationsCount(query),
      ])
      setRows(page)
      setTotal(count)
      setAfterStack((s) => {
        // track page starts so prev/first can re-anchor keyset cursors
        if (after == null) return []
        return [...s, after]
      })
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }, [query, perPage])

  useEffect(() => { void loadPage(undefined) }, [loadPage])

  // client-side search over the loaded page (server has no free-text search
  // for reservations; dates are filtered server-side)
  const visible = useMemo(() => {
    const q = search.trim().toLowerCase()
    if (q === '') return rows
    return rows.filter((r) => {
      const guest = r.guest_id ? (guests.find((g) => g.id === r.guest_id)?.full_name ?? '') : ''
      const room = rooms.find((rm) => rm.id === r.room_id)
      return guest.toLowerCase().includes(q)
        || (room?.room_number.toLowerCase().includes(q) ?? false)
        || r.voucher_number?.toLowerCase().includes(q)
        || r.id.toLowerCase().includes(q)
        || r.checkin_date.includes(q)
        || r.checkout_date.includes(q)
    })
  }, [rows, search, guests, rooms])

  // "1-20 / 86" numbers
  const loadedBefore = afterStack.length * perPage
  const first = loadedBefore + (visible.length > 0 ? 1 : 0)
  const last = loadedBefore + visible.length
  const nextPageAfter = rows.length === perPage ? rows[rows.length - 1].id : undefined
  const prevPageAfter = afterStack.length >= 2 ? afterStack[afterStack.length - 2] : undefined

  if (!session?.user) return null

  return (
    <div className="page">
      <PageHeader title={t('nav.bookings')} actions={canCreate && (
        <button className="primary" onClick={() => setCreating(true)}>{t('bk.add')}</button>
      )} />
      {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}

      <div className="seg" role="tablist" aria-label={t('bk.view')}>
        {TABS.map((tb) => (
          <button key={tb} className={tab === tb ? 'on' : ''} onClick={() => setTab(tb)}>
            {t(`bk.tabs.${tb}`)}
          </button>
        ))}
      </div>

      <FilterBar>
        <input
          placeholder={t('bk.search')}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          style={{ width: 220 }}
        />
        <label className="muted" style={{ fontSize: 12 }}>
          {t('res.checkin')}
          <input type="date" value={dateFrom} onChange={(e) => setDateFrom(e.target.value)} />
        </label>
        <label className="muted" style={{ fontSize: 12 }}>
          {t('res.checkout')}
          <input type="date" value={dateTo} onChange={(e) => setDateTo(e.target.value)} />
        </label>
        {/* the server orders newest-first (keyset -created_at); shown for parity */}
        <select value="newest" onChange={() => {}} disabled title={t('bk.orderFixed')}>
          <option value="newest">{t('bk.orderNewest')}</option>
        </select>
        <button
          className="ghost"
          onClick={() => { setSearch(''); setDateFrom(''); setDateTo('') }}
        >
          {t('bk.clear')}
        </button>
      </FilterBar>

      <p className="muted" style={{ fontSize: 13, marginTop: 0 }}>
        {total == null ? '' : t('bk.records', { n: total })} · {t(`bk.tabs.${tab}`)}
      </p>

      <table className="hms">
        <thead>
          <tr>
            <th>#</th><th>{t('res.guest')}</th><th>{t('rooms.number')}</th>
            <th>{t('res.checkin')}</th><th>{t('res.checkout')}</th>
            <th>{t('res.status')}</th><th>{t('bk.balance')}</th><th>{t('res.total')}</th>
          </tr>
        </thead>
        <tbody>
          {visible.length === 0 && !busy && (
            <tr><td colSpan={8} className="muted" style={{ textAlign: 'center' }}>{t('bk.noRows')}</td></tr>
          )}
          {visible.map((r, i) => {
            const guest = r.guest_id ? guests.find((g) => g.id === r.guest_id)?.full_name : null
            const room = rooms.find((rm) => rm.id === r.room_id)
            const typeName = roomTypes.find((rt) => rt.id === room?.room_type_id)?.name
            return (
              <tr key={r.id} style={{ cursor: 'pointer' }} onClick={() => setDetailId(r.id)}>
                <td className="muted">{loadedBefore + i + 1}</td>
                <td>{guest ?? <span className="muted">{r.guest_id == null ? '—' : '#' + r.guest_id.slice(0, 8)}</span>}</td>
                <td>
                  {room?.room_number ?? '—'}
                  {typeName ? <span className="muted"> · {typeName}</span> : null}
                </td>
                <td>{r.checkin_date}</td>
                <td>{r.checkout_date}</td>
                <td><StatusPill status={r.status} label={t(`res.${r.status}`)} /></td>
                {/* balance needs per-row payments aggregation; shown as empty */}
                <td className="muted">—</td>
                <td>{fmtMoney(r.total_price_cents, r.currency)}</td>
              </tr>
            )
          })}
        </tbody>
      </table>

      <ListFooter
        first={first}
        last={last}
        total={total ?? 0}
        perPage={perPage}
        onPerPage={(n) => setPerPage(n)}
        onFirst={() => {
          setAfterStack([])
          void loadPage(undefined)
        }}
        onPrev={() => { setAfterStack((s) => s.slice(0, -1)); void loadPage(prevPageAfter) }}
        onNext={() => { if (nextPageAfter != null) void loadPage(nextPageAfter) }}
        onLast={() => { /* keyset paging cannot jump to the end */ }}
      />

      {creating && (
        <NewReservationModal
          prefill={{ roomId: rooms[0]?.id ?? '', date: new Date().toISOString().slice(0, 10), nights: 1 }}
          rooms={rooms}
          channels={channels}
          guests={guests}
          onClose={() => setCreating(false)}
          onCreated={(r) => { setCreating(false); setDetailId(r.id); void loadPage(undefined) }}
        />
      )}
      {detailId != null && (
        <ReservationDetailModal id={detailId} onClose={() => setDetailId(null)} onChanged={() => void loadPage(afterStack[afterStack.length - 1])} />
      )}
    </div>
  )
}

function isHk(session: ReturnType<typeof useAuth>['session']): boolean {
  return session?.user.role === 'housekeeping'
}
