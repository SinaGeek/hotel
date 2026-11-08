import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { api, ApiError } from '../api/client'
import type { BoardRoom, GuestDto, ReservationDto, RoomDto } from '../types/generated/api'
import { fmtMoney } from './reservation-modals'

export function ReassignModal(props: {
  reservation: ReservationDto
  rooms: BoardRoom[]
  guests: GuestDto[]
  onClose: () => void
  onResolved: () => void
}) {
  const { t } = useTranslation()
  const res = props.reservation
  const guest = props.guests.find((g) => g.id === res.guest_id)?.full_name ?? '—'
  const currentRoom = props.rooms.find((r) => r.id === res.room_id)

  const [suggested, setSuggested] = useState<RoomDto[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    let alive = true
    setLoading(true)
    api.suggestedRooms(res.id)
      .then((rooms) => {
        if (!alive) return
        setSuggested(rooms)
      })
      .catch((e) => {
        if (!alive) return
        setError(e instanceof ApiError ? e.message : String(e))
      })
      .finally(() => {
        if (alive) setLoading(false)
      })
    return () => { alive = false }
  }, [res.id])

  async function handleReassign(newRoomId: string) {
    setBusy(true)
    setError(null)
    try {
      await api.editReservation(res.id, { room_id: newRoomId })
      props.onResolved()
      props.onClose()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  async function handleCancel() {
    if (!confirm(t('res.cancel') + '?')) return
    setBusy(true)
    setError(null)
    try {
      await api.cancelReservation(res.id)
      props.onResolved()
      props.onClose()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="modal-back" onClick={props.onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 560 }}>
        <h3>⚠️ {t('bk.reassignTitle')} #{res.id.slice(0, 8)}</h3>
        <p className="muted" style={{ margin: '8px 0 16px' }}>
          {t('bk.reassignDesc')}
        </p>

        <div className="card" style={{ marginBottom: 16, backgroundColor: 'rgba(245, 158, 11, 0.08)', border: '1px solid rgba(245, 158, 11, 0.3)' }}>
          <div className="row" style={{ justifyContent: 'space-between', marginBottom: 6 }}>
            <span><b>{t('res.guest')}:</b> {guest}</span>
            <span><b>{t('res.total')}:</b> {fmtMoney(res.total_price_cents, res.currency)}</span>
          </div>
          <div className="row" style={{ justifyContent: 'space-between' }}>
            <span><b>{t('res.checkin')} - {t('res.checkout')}:</b> {res.checkin_date} → {res.checkout_date}</span>
            <span><b>{t('res.room')}:</b> {currentRoom ? `${currentRoom.room_number} (${currentRoom.room_type_name})` : res.room_id}</span>
          </div>
        </div>

        {error && <p style={{ color: 'red', marginBottom: 12 }}>{t('common.error')}: {error}</p>}

        <h4>{t('bk.suggestedRooms')}</h4>
        {loading ? (
          <p className="muted">{t('common.loading')}</p>
        ) : suggested.length === 0 ? (
          <p className="muted">{t('bk.noRoomsAvailable')}</p>
        ) : (
          <div style={{ display: 'grid', gap: 8, maxHeight: 240, overflowY: 'auto', margin: '8px 0 16px' }}>
            {suggested.map((room) => {
              const bRoom = props.rooms.find((r) => r.id === room.id)
              const sameType = currentRoom && room.room_type_id === currentRoom.room_type_id
              return (
                <div
                  key={room.id}
                  className="card row"
                  style={{ justifyContent: 'space-between', alignItems: 'center', padding: '10px 14px' }}
                >
                  <div>
                    <span style={{ fontWeight: 600, fontSize: 15 }}>Room {room.room_number}</span>
                    <span className="muted" style={{ marginLeft: 8, fontSize: 13 }}>
                      {bRoom?.room_type_name ?? ''} · Floor {room.floor} · {room.cleanliness}
                    </span>
                    {sameType && (
                      <span style={{ marginLeft: 8, fontSize: 11, background: '#e0f2fe', color: '#0369a1', padding: '2px 6px', borderRadius: 4 }}>
                        {t('bk.sameRoomType')}
                      </span>
                    )}
                  </div>
                  <button
                    className="primary"
                    disabled={busy}
                    onClick={() => handleReassign(room.id)}
                    style={{ whiteSpace: 'nowrap' }}
                  >
                    {t('bk.reassignBtn', { room: room.room_number })}
                  </button>
                </div>
              )
            })}
          </div>
        )}

        <div className="row" style={{ justifyContent: 'space-between', marginTop: 16 }}>
          <button className="danger" disabled={busy} onClick={handleCancel}>
            {t('res.cancel')}
          </button>
          <button type="button" onClick={props.onClose}>
            {t('common.back')}
          </button>
        </div>
      </div>
    </div>
  )
}
