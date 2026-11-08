// Facility > Rooms (Phase 5b.5, reference parity per docs/ui-reference.md):
// Room / Room Type / Floor / Status grid; with housekeeping_update_status
// permission the status cell is an inline dropdown (the reference's separate
// "Update status" page, merged here — deviation noted in todo.md).
import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { api, ApiError, sessionHasPermission } from '../api/client'
import { useAuth } from '../api/auth'
import { FilterBar, PageHeader, StatusPill } from '../components/ui'
import type { RoomDto, RoomTypeDto } from '../types/generated/api'

const STATES = ['dirty', 'cleaning', 'clean', 'inspected', 'out_of_order']

export function Rooms() {
  const { t } = useTranslation()
  const { session } = useAuth()
  const [rooms, setRooms] = useState<RoomDto[]>([])
  const [types, setTypes] = useState<RoomTypeDto[]>([])
  const [status, setStatus] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const canUpdate = sessionHasPermission(session, 'housekeeping_update_status')

  const load = useCallback(async () => {
    try {
      setBusy(true)
      setError(null)
      const [r, rt] = await Promise.all([api.listRooms(), api.listRoomTypes()])
      setRooms(r)
      setTypes(rt)
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }, [])
  useEffect(() => { void load() }, [load])

  const typeName = (id: string) => types.find((x) => x.id === id)?.name ?? ''

  const visible = useMemo(
    () => (status === '' ? rooms : rooms.filter((r) => r.cleanliness === status)),
    [rooms, status],
  )

  async function setCleanliness(room: RoomDto, cleanliness: string) {
    try {
      await api.setCleanliness(room.id, cleanliness)
      setRooms((rs) => rs.map((r) => (r.id === room.id ? { ...r, cleanliness } : r)))
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    }
  }

  return (
    <div className="page">
      <PageHeader title={t('nav.rooms')} subtitle={t('fac.roomCount', { n: rooms.length })} />
      {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}

      <FilterBar>
        <select value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">{t('fac.filterStatus')}</option>
          {STATES.map((s) => <option key={s} value={s}>{t(`rooms.${s}`)}</option>)}
        </select>
        <button className="ghost" disabled={status === ''} onClick={() => setStatus('')}>{t('bk.clear')}</button>
      </FilterBar>

      <div className="table-wrap"><table className="hms">
        <thead>
          <tr>
            <th>{t('rooms.number')}</th><th>{t('rooms.type')}</th>
            <th>{t('fac.floor')}</th><th>{t('rooms.cleanliness')}</th>
          </tr>
        </thead>
        <tbody>
          {visible.length === 0 && !busy && (
            <tr><td colSpan={4} className="muted" style={{ textAlign: 'center' }}>{t('bk.noRows')}</td></tr>
          )}
          {visible.map((r) => (
            <tr key={r.id}>
              <td style={{ fontWeight: 600 }}>{r.room_number}</td>
              <td>{typeName(r.room_type_id)}</td>
              <td className="muted">{r.floor}</td>
              <td>
                {canUpdate ? (
                  <select value={r.cleanliness} onChange={(e) => void setCleanliness(r, e.target.value)}>
                    {STATES.map((s) => <option key={s} value={s}>{t(`rooms.${s}`)}</option>)}
                  </select>
                ) : (
                  <StatusPill status={r.cleanliness} label={t(`rooms.${r.cleanliness}`)} />
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table></div>
    </div>
  )
}
