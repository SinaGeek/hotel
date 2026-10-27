// Rooms + housekeeping: list and update cleanliness (housekeeping updates its
// own rooms; last-write-wins per README §11).
import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { api, ApiError, sessionHasPermission } from '../api/client'
import { useAuth } from '../api/auth'
import type { RoomDto, RoomTypeDto } from '../types/generated/api'

const STATES = ['dirty', 'cleaning', 'clean', 'inspected', 'out_of_order']

export function Rooms() {
  const { t } = useTranslation()
  const { session } = useAuth()
  const [rooms, setRooms] = useState<RoomDto[]>([])
  const [types, setTypes] = useState<RoomTypeDto[]>([])
  const [error, setError] = useState<string | null>(null)
  const canUpdate = sessionHasPermission(session, 'housekeeping_update_status')

  const load = useCallback(async () => {
    try {
      setError(null)
      const [r, rt] = await Promise.all([api.listRooms(), api.listRoomTypes()])
      setRooms(r)
      setTypes(rt)
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    }
  }, [])
  useEffect(() => { void load() }, [load])

  async function setCleanliness(room: RoomDto, cleanliness: string) {
    try {
      await api.setCleanliness(room.id, cleanliness)
      await load()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    }
  }

  const typeName = (id: string) => types.find((x) => x.id === id)?.name ?? id.slice(0, 8)

  return (
    <div className="page">
      <h2>{t('rooms.title')}</h2>
      {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}
      <table className="hms">
        <thead><tr><th>{t('rooms.number')}</th><th>{t('rooms.type')}</th><th>{t('rooms.cleanliness')}</th></tr></thead>
        <tbody>
          {rooms.map((r) => (
            <tr key={r.id}>
              <td style={{ fontWeight: 600 }}>{r.room_number}</td>
              <td>{typeName(r.room_type_id)}</td>
              <td>
                {canUpdate ? (
                  <select value={r.cleanliness} onChange={(e) => void setCleanliness(r, e.target.value)}>
                    {STATES.map((s) => <option key={s} value={s}>{t(`rooms.${s}`)}</option>)}
                  </select>
                ) : (<span>{t(`rooms.${r.cleanliness}`) || r.cleanliness}</span>)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
