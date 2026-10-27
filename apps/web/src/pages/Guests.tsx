import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { api, ApiError, sessionHasPermission } from '../api/client'
import { useAuth } from '../api/auth'
import type { GuestDto } from '../types/generated/api'

export function Guests() {
  const { t } = useTranslation()
  const { session } = useAuth()
  const [items, setItems] = useState<GuestDto[]>([])
  const [search, setSearch] = useState('')
  const [fullName, setFullName] = useState('')
  const [nationality, setNationality] = useState('')
  const [error, setError] = useState<string | null>(null)
  const canCreate = sessionHasPermission(session, 'reservation_create')

  const load = useCallback(async () => {
    try {
      setError(null)
      setItems(await api.listGuests(search))
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    }
  }, [search])

  useEffect(() => { void load() }, [load])

  async function create(e: React.FormEvent) {
    e.preventDefault()
    try {
      await api.createGuest({ full_name: fullName, nationality: nationality || undefined })
      setFullName(''); setNationality('')
      await load()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    }
  }

  return (
    <div className="page">
      <div className="row">
        <h2 style={{ margin: 0, flexGrow: 1 }}>{t('guests.title')}</h2>
        <input placeholder={t('guests.search')} value={search} onChange={(e) => setSearch(e.target.value)} />
      </div>
      {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}
      <table className="hms">
        <thead><tr><th>{t('guests.fullName')}</th><th>{t('guests.nationality')}</th><th>{t('guests.phone')}</th></tr></thead>
        <tbody>
          {items.map((g) => (
            <tr key={g.id}><td>{g.full_name}</td><td>{g.nationality ?? ''}</td><td className="muted">•••</td></tr>
          ))}
        </tbody>
      </table>
      {canCreate && (
        <form className="card row" style={{ marginTop: 12 }} onSubmit={create}>
          <input placeholder={t('guests.fullName')} value={fullName} onChange={(e) => setFullName(e.target.value)} required style={{ width: 220 }} />
          <input placeholder={t('guests.nationality')} value={nationality} onChange={(e) => setNationality(e.target.value)} style={{ width: 110 }} />
          <button className="primary" type="submit">{t('guests.create')}</button>
        </form>
      )}
    </div>
  )
}
