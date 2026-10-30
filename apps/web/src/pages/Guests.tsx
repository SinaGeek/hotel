// Guests (Phase 5b.4, reference parity per docs/ui-reference.md): title +
// record count, Download dropdown (CSV export), Birthday/Garage quick views,
// search + country filters, "+ Add Guest". Our data model has no guest IDs
// or balances, so the honest columns are full name, nationality, license
// plate, birth date and primary/KBS flags.
import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { api, ApiError, sessionHasPermission } from '../api/client'
import { useAuth } from '../api/auth'
import { FilterBar, PageHeader, StatusPill } from '../components/ui'
import { iso } from '../components/reservation-modals'
import type { GuestDto } from '../types/generated/api'

type Quick = 'all' | 'birthday' | 'garage'

function downloadCsv(rows: GuestDto[]) {
  const esc = (v: string) => (/[",\n;]/.test(v) ? `"${v.replace(/"/g, '""')}"` : v)
  const header = 'Full name,Nationality,License plate,Birth date,Category,KBS reported'
  const lines = rows.map((g) => [
    esc(g.full_name), esc(g.nationality ?? ''), esc(g.license_plate ?? ''), esc(g.birth_date ?? ''),
    esc(g.is_primary ? 'Primary' : 'Standard'), esc(g.kbs_reported ? 'Yes' : 'No'),
  ].join(','))
  const blob = new Blob(['\ufeff' + header + '\n' + lines.join('\n')], { type: 'text/csv;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `guests-${iso(new Date())}.csv`
  a.click()
  URL.revokeObjectURL(url)
}

export function Guests() {
  const { t } = useTranslation()
  const { session } = useAuth()
  const canCreate = sessionHasPermission(session, 'reservation_create')

  const [items, setItems] = useState<GuestDto[]>([])
  const [search, setSearch] = useState('')
  const [country, setCountry] = useState('')
  const [quick, setQuick] = useState<Quick>('all')
  const [showAdd, setShowAdd] = useState(false)
  const [fullName, setFullName] = useState('')
  const [nationality, setNationality] = useState('')
  const [phone, setPhone] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const load = useCallback(async () => {
    try {
      setBusy(true)
      setError(null)
      setItems(await api.listGuests(search))
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }, [search])

  useEffect(() => { void load() }, [load])

  const countries = useMemo(
    () => [...new Set(items.map((g) => g.nationality).filter((n): n is string => n != null))].sort(),
    [items],
  )

  const todayMD = iso(new Date()).slice(5)
  const visible = useMemo(() => {
    return items.filter((g) => {
      if (country !== '' && g.nationality !== country) return false
      if (quick === 'birthday' && g.birth_date?.slice(5) !== todayMD) return false
      if (quick === 'garage' && g.license_plate == null) return false
      return true
    })
  }, [items, country, quick, todayMD])

  async function create(e: React.FormEvent) {
    e.preventDefault()
    try {
      await api.createGuest({ full_name: fullName, nationality: nationality || undefined, phone: phone || undefined })
      setFullName(''); setNationality(''); setPhone('')
      setShowAdd(false)
      await load()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  return (
    <div className="page">
      <PageHeader
        title={t('nav.guests')}
        subtitle={t('bk.records', { n: items.length })}
        actions={(
          <>
            <button className="ghost" onClick={() => downloadCsv(items)}>
              {t('gu.download')} (CSV)
            </button>
            {canCreate && <button className="primary" onClick={() => setShowAdd((s) => !s)}>{t('gu.add')}</button>}
          </>
        )}
      />
      {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}

      <div className="seg" role="tablist" aria-label={t('gu.quickLabel')}>
        {(['all', 'birthday', 'garage'] as const).map((q) => (
          <button key={q} className={quick === q ? 'on' : ''} onClick={() => setQuick(q)}>
            {t(`gu.quick.${q}`)}
          </button>
        ))}
      </div>

      <FilterBar>
        <input
          placeholder={t('gu.search')}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          style={{ width: 220 }}
        />
        <select value={country} onChange={(e) => setCountry(e.target.value)}>
          <option value="">{t('gu.country')}</option>
          {countries.map((c) => <option key={c} value={c}>{c}</option>)}
        </select>
        <button
          className="ghost"
          disabled={search === '' && country === '' && quick === 'all'}
          onClick={() => { setSearch(''); setCountry(''); setQuick('all') }}
        >
          {t('bk.clear')}
        </button>
      </FilterBar>

      <div className="table-wrap"><table className="hms">
        <thead>
          <tr>
            <th>#</th><th>{t('gu.fullName')}</th><th>{t('gu.nationality')}</th>
            <th>{t('gu.plate')}</th><th>{t('gu.birth')}</th><th>{t('gu.category')}</th>
          </tr>
        </thead>
        <tbody>
          {visible.length === 0 && !busy && (
            <tr><td colSpan={6} className="muted" style={{ textAlign: 'center' }}>{t('bk.noRows')}</td></tr>
          )}
          {visible.map((g, i) => (
            <tr key={g.id}>
              <td className="muted">{i + 1}</td>
              <td>{g.full_name}</td>
              <td className="muted">{g.nationality ?? '—'}</td>
              <td className="muted">{g.license_plate ?? '—'}</td>
              <td className="muted">{g.birth_date ?? '—'}</td>
              <td>
                <StatusPill
                  status={g.is_primary ? 'active' : 'inactive'}
                  label={g.is_primary ? t('gu.primary') : t('gu.standard')}
                />
              </td>
            </tr>
          ))}
        </tbody>
      </table></div>

      {showAdd && canCreate && (
        <form className="card row" style={{ marginTop: 12 }} onSubmit={create}>
          <input placeholder={t('gu.fullName')} value={fullName} onChange={(e) => setFullName(e.target.value)} required style={{ width: 200 }} />
          <input placeholder={t('gu.nationality')} value={nationality} onChange={(e) => setNationality(e.target.value)} style={{ width: 110 }} />
          <input placeholder={t('gu.phone')} value={phone} onChange={(e) => setPhone(e.target.value)} style={{ width: 130 }} />
          <button className="primary" type="submit" disabled={busy}>{t('gu.add')}</button>
        </form>
      )}
    </div>
  )
}
