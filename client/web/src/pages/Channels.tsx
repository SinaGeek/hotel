// Tools > Channels (Phase 5b.7, reference parity per REA capture
// tools-channels.ax.txt): subtitle, record count, search, status filter,
// fixed "Newest" sort, and a Channel / Kind / Status table. The reference
// shows a per-channel integration "Hotel Code" which our model does not
// store — the channel `kind` is shown instead (deviation in todo.md).
import { useCallback, useEffect, useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { api, ApiError } from '../api/client'
import { FilterBar, PageHeader, StatusPill } from '../components/ui'
import type { ChannelDto } from '../types/generated/api'

export function Channels() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const [items, setItems] = useState<ChannelDto[]>([])
  const [search, setSearch] = useState('')
  const [status, setStatus] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const load = useCallback(async () => {
    try {
      setBusy(true)
      setError(null)
      setItems(await api.listChannels())
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }, [])
  useEffect(() => { void load() }, [load])

  const visible = useMemo(() => {
    const q = search.trim().toLowerCase()
    return items
      .filter((c) => status === '' || (status === 'active') === c.is_active)
      .filter((c) => q === '' || c.name.toLowerCase().includes(q) || c.kind.toLowerCase().includes(q))
  }, [items, search, status])

  const channelsCount = items.length

  return (
    <div className="page">
      <PageHeader title={t('nav.channels')} subtitle={t('ch.subtitle')} />
      {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}

      <div className="mb-4 p-4 bg-gradient-to-r from-blue-50 to-indigo-50 border border-blue-200 rounded-lg">
        <div className="flex items-center justify-between">
          <div>
            <h3 className="font-semibold text-lg mb-1">🌐 OTA Channel Manager Integrations</h3>
            <p className="text-sm text-gray-700">
              Connect with Booking.com and Expedia to automatically sync reservations, rates, and availability
            </p>
          </div>
          <button
            onClick={() => navigate('/ota-integrations')}
            className="px-6 py-3 bg-blue-600 text-white rounded-lg hover:bg-blue-700 font-medium shadow-md transition-colors"
          >
            Configure OTAs →
          </button>
        </div>
      </div>

      <p className="muted" style={{ fontSize: 13, margin: '0 0 8px' }}>
        {t('bk.records', { n: channelsCount })} · {busy ? t('common.loading') : visible.length + ' ' + t('ch.shown')}
      </p>

      <FilterBar>
        <input
          placeholder={t('ch.search')}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="search-input"
        />
        <select value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">{t('ch.all')}</option>
          <option value="active">{t('ch.active')}</option>
          <option value="inactive">{t('ch.inactive')}</option>
        </select>
        {/* the server orders channels by name; shown for reference parity */}
        <select value="newest" onChange={() => {}} disabled title={t('bk.orderFixed')}>
          <option value="newest">{t('bk.orderNewest')}</option>
        </select>
        <button className="ghost" disabled={search === '' && status === ''} onClick={() => { setSearch(''); setStatus('') }}>
          {t('bk.clear')}
        </button>
      </FilterBar>

      <div className="table-wrap"><table className="hms">
        <thead>
          <tr>
            <th>{t('ch.channel')}</th><th>{t('ch.kind')}</th><th>{t('res.status')}</th>
          </tr>
        </thead>
        <tbody>
          {visible.length === 0 && !busy && (
            <tr><td colSpan={3} className="muted" style={{ textAlign: 'center' }}>{t('bk.noRows')}</td></tr>
          )}
          {visible.map((c) => (
            <tr key={c.id}>
              <td style={{ fontWeight: 600 }}>{c.name}</td>
              <td className="muted">{c.kind}</td>
              <td>
                <StatusPill
                  status={c.is_active ? 'active' : 'inactive'}
                  label={c.is_active ? t('ch.active') : t('ch.inactive')}
                />
              </td>
            </tr>
          ))}
        </tbody>
      </table></div>
    </div>
  )
}
