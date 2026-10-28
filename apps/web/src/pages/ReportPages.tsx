// Reports hub + the Daily report set (Phase 5b.6, reference parity per REA
// captures reports.ax.txt / reports-forecast / reports-inhouse /
// reports-maincourante). The hub is a grouped link grid; every reference
// group is rendered, but only the reports backed by our own APIs are links —
// the rest are muted "Soon" chips (honest pending-work markers, todo 5b.6).
import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Link } from 'react-router-dom'
import {
  Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis,
} from 'recharts'
import { api, ApiError, sessionHasPermission } from '../api/client'
import { useAuth } from '../api/auth'
import { PageHeader } from '../components/ui'
import { iso } from '../components/reservation-modals'
import type { ForecastRow, InhouseReport, MainCouranteReport } from '../types/generated/api'

const isoToday = () => iso(new Date())
const money = (cents: number, currency: string) =>
  new Intl.NumberFormat(undefined, { style: 'currency', currency, maximumFractionDigits: 2 }).format(cents / 100)
const money0 = (cents: number, currency: string) =>
  new Intl.NumberFormat(undefined, { style: 'currency', currency, maximumFractionDigits: 0 }).format(cents / 100)

function downloadCsv(filename: string, rows: (string | number)[][]) {
  const csv = '\ufeff' + rows.map((r) => r.map((c) => (typeof c === 'string' && (c.includes(',') || c.includes('"')) ? `"${c.replace(/"/g, '""')}"` : String(c))).join(',')).join('\r\n')
  const url = URL.createObjectURL(new Blob([csv], { type: 'text/csv;charset=utf-8' }))
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

function ErrorStrip({ error }: { error: string | null }) {
  const { t } = useTranslation()
  if (!error) return null
  return <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>
}

// Common permission gate: reports are report_view_tenant-served (server-side
// 403 for housekeeping, mirroring the README §6 matrix); the shell hides the
// page shell too so HK never sees the reference grid.
function useReportGate(): React.ReactNode | null {
  const { t } = useTranslation()
  const { session } = useAuth()
  const isHk = session?.user.role === 'housekeeping'
  const can = (p: string) => sessionHasPermission(session, p)
  if (isHk || !can('report_view_tenant')) {
    return <div className="page"><h2>{t('rep.title')}</h2><p className="muted">{t('reports.noAccess')}</p></div>
  }
  return null
}

// A date input + Download row like the reference (Date: <..., 2026>).
function DateBar({ label, date, onDate, onDownload }: {
  label: string
  date: string
  onDate: (v: string) => void
  onDownload: () => void
}) {
  const { t } = useTranslation()
  return (
    <div className="row" style={{ gap: 8, marginBottom: 10 }}>
      <label className="muted" style={{ fontSize: 12 }}>
        {label}
        <input type="date" value={date} onChange={(e) => onDate(e.target.value)} style={{ marginLeft: 6 }} />
      </label>
      <button className="ghost" onClick={onDownload}>{t('gu.download')} (CSV)</button>
    </div>
  )
}

// ----------------------------------------------------------------------------- hub

const HUB: { key: string; label?: string; to?: string; soon?: boolean }[][] = [
  [
    { key: 'maincourante', label: 'linkMainCourante', to: '/reports/maincourante' },
    { key: 'forecast', label: 'linkForecast', to: '/reports/forecast' },
    { key: 'inhouse', label: 'linkInhouse', to: '/reports/inhouse' },
    { key: 'mealList', soon: true },
    { key: 'nationality', soon: true },
    { key: 'channelAnalysis', soon: true },
    { key: 'collectionSummary', soon: true },
    { key: 'expenseSummary', soon: true },
    { key: 'freeRooms', soon: true },
  ],
  [
    { key: 'willCheckin', soon: true },
    { key: 'checkin', soon: true },
    { key: 'willCheckout', soon: true },
    { key: 'earlyExit', soon: true },
    { key: 'dateChange', soon: true },
    { key: 'deleted', soon: true },
    { key: 'unselectedRoom', soon: true },
    { key: 'transfer', soon: true },
  ],
  [
    { key: 'roomChange', soon: true },
    { key: 'roomOccupancy', soon: true },
    { key: 'roomClean', soon: true },
    { key: 'periodPrice', soon: true },
    { key: 'priceChange', soon: true },
  ],
  [
    { key: 'guestDaily', soon: true },
  ],
  [
    { key: 'analysis', to: '/reports/analysis' },
    { key: 'collection', soon: true },
    { key: 'expense', soon: true },
    { key: 'extra', soon: true },
    { key: 'invoice', soon: true },
    { key: 'bankAccounts', soon: true },
    { key: 'bankTransactions', soon: true },
    { key: 'agencyBalance', soon: true },
    { key: 'agencyRate', soon: true },
  ],
  [
    { key: 'log', soon: true },
  ],
]

export function ReportsHub() {
  const { t } = useTranslation()
  const { session } = useAuth()
  const can = (p: string) => sessionHasPermission(session, p)
  const isHk = session?.user.role === 'housekeeping'
  if (isHk || !can('report_view_tenant')) {
    return <div className="page"><h2>{t('rep.title')}</h2><p className="muted">{t('reports.noAccess')}</p></div>
  }
  const groups = ['rep.group.daily', 'rep.group.bookings', 'rep.group.rooms', 'rep.group.guests', 'rep.group.finance', 'rep.group.system']
  return (
    <div className="page">
      <PageHeader title={t('rep.title')} subtitle={t('rep.hubSubtitle')} />
      <div style={{ display: 'flex', gap: 24, flexWrap: 'wrap', alignItems: 'flex-start' }}>
        {groups.map((g, gi) => (
          <div key={g} className="card" style={{ flex: '1 1 240px', minWidth: 220 }}>
            <h3 style={{ margin: '0 0 10px' }}>{t(g)}</h3>
            <ul style={{ listStyle: 'none', margin: 0, padding: 0, display: 'grid', gap: 6 }}>
              {HUB[gi].map((item) => (
                <li key={item.key}>
                  {item.to != null ? (
                    <Link to={item.to} style={{ fontWeight: 500 }}>{t(`rep.${item.label ?? item.key}`)}</Link>
                  ) : (
                    <span className="muted" style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
                      {t(`rep.${item.key}`)}
                      <span
                        className="pill gold"
                        style={{ fontSize: 10.5, padding: '1px 7px' }}
                      >{t('rep.soon')}</span>
                    </span>
                  )}
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
    </div>
  )
}

function PageHeaderWithBack({ title }: { title: string }) {
  const { t } = useTranslation()
  return (
    <PageHeader
      title={title}
      actions={<Link to="/reports" className="ghost" style={{ textDecoration: 'none' }}>{t('rep.back')}</Link>}
    />
  )
}

// ------------------------------------------------------------------------- forecast

export function ReportsForecast() {
  const { t } = useTranslation()
  const gate = useReportGate()
  const [start, setStart] = useState(isoToday)
  const [days, setDays] = useState(15)
  const [rows, setRows] = useState<ForecastRow[]>([])
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const load = useCallback(async () => {
    try {
      setBusy(true)
      setError(null)
      setRows(await api.reportForecast(start, days))
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }, [start, days])
  useEffect(() => { void load() }, [load])

  if (gate) return gate

  return (
    <div className="page">
      <PageHeaderWithBack title={t('rep.forecast.title')} />
      <ErrorStrip error={error} />
      <DateBar
        label={t('rep.forecast.start')}
        date={start}
        onDate={setStart}
        onDownload={() => downloadCsv(`occupancy_forecast_${start}_${days}d.csv`, [
          [t('rep.forecast.date'), t('rep.forecast.arrivals'), t('rep.forecast.occupied'), t('rep.forecast.vacant'), t('rep.forecast.guests'), t('rep.forecast.occupancy')],
          ...rows.map((r) => [r.date, r.arrivals, r.occupied, r.vacant, r.guests, `${r.occupancy_pct.toFixed(1)}%`]),
        ])}
      />
      <div className="row" style={{ gap: 8, marginBottom: 10 }}>
        {[7, 15, 30].map((d) => (
          <button key={d} className={days === d ? 'on' : 'ghost'} onClick={() => setDays(d)}>{d} {t('rep.days')}</button>
        ))}
        <button className="primary" disabled={busy} onClick={() => void load()}>{busy ? t('common.loading') : t('reports.refresh')}</button>
      </div>
      <table className="hms">
        <thead>
          <tr>
            <th>{t('rep.forecast.date')}</th><th>{t('rep.forecast.arrivals')}</th><th>{t('rep.forecast.occupied')}</th>
            <th>{t('rep.forecast.vacant')}</th><th>{t('rep.forecast.guests')}</th><th>{t('rep.forecast.occupancy')}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.date}>
              <td>{r.date}</td>
              <td>{r.arrivals}</td>
              <td>{r.occupied}</td>
              <td>{r.vacant}</td>
              <td>{r.guests}</td>
              <td>
                <div className="row" style={{ gap: 8 }}>
                  <span>{r.occupancy_pct.toFixed(1)}%</span>
                  <div style={{ width: 90, height: 6, borderRadius: 3, overflow: 'hidden' }}>
                    <div style={{ width: `${Math.min(100, r.occupancy_pct)}%`, height: '100%', background: 'var(--accent, #6366f1)' }} />
                  </div>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

// ------------------------------------------------------------------------- in-house

export function ReportsInhouse() {
  const { t } = useTranslation()
  const gate = useReportGate()
  const [date, setDate] = useState(isoToday)
  const [rep, setRep] = useState<InhouseReport | null>(null)
  const [view, setView] = useState<'table' | 'chart'>('table')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const load = useCallback(async () => {
    try {
      setBusy(true)
      setError(null)
      setRep(await api.reportInhouse(date))
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }, [date])
  useEffect(() => { void load() }, [load])

  const chartData = (rep?.rows ?? []).map((r) => ({
    room: r.room_number,
    balance: r.balance_cents / 100,
  }))
  const cur = (c: number) => money(c, rep?.currency ?? 'TRY')

  if (gate) return gate

  return (
    <div className="page">
      <PageHeaderWithBack title={t('rep.inhouse.title')} />
      <ErrorStrip error={error} />
      <div className="row" style={{ gap: 10, marginBottom: 10 }}>
        <DateBar
          label={t('rep.date')}
          date={date}
          onDate={setDate}
          onDownload={() => downloadCsv(`inhouse_${date}.csv`, [
            [t('res.guest'), t('rooms.number'), t('rooms.type'), t('res.checkin'), t('res.checkout'), t('rep.inhouse.guests'), t('rep.inhouse.balance')],
            ...(rep?.rows ?? []).map((r) => [
              r.guest_name ?? '—', r.room_number, r.room_type_name, r.checkin_date, r.checkout_date,
              r.guests, (r.balance_cents / 100).toFixed(2),
            ]),
          ])}
        />
        <div className="seg" role="tablist" aria-label={t('rep.view')}>
          <button className={view === 'table' ? 'on' : ''} onClick={() => setView('table')}>{t('rep.table')}</button>
          <button className={view === 'chart' ? 'on' : ''} onClick={() => setView('chart')}>{t('rep.chart')}</button>
        </div>
      </div>

      {rep && (
        <div className="kpis" style={{ marginBottom: 12 }}>
          <div className="kpi"><b>{rep.bookings}</b><span>{t('rep.inhouse.totalBookings')}</span></div>
          <div className="kpi"><b>{rep.adults} + {rep.children}</b><span>{t('rep.inhouse.guests')}</span></div>
          <div className="kpi"><b>{money0(rep.room_rate_cents, rep.currency)}</b><span>{t('rep.inhouse.roomRate')}</span></div>
          <div className="kpi"><b>{money0(rep.balance_cents, rep.currency)}</b><span>{t('rep.inhouse.balance')}</span></div>
        </div>
      )}

      {view === 'table' && (
        <table className="hms">
          <thead>
            <tr>
              <th>{t('res.guest')}</th><th>{t('rooms.number')}</th><th>{t('rooms.type')}</th>
              <th>{t('res.checkin')}</th><th>{t('res.checkout')}</th>
              <th>{t('rep.inhouse.guests')}</th><th>{t('rep.inhouse.balance')}</th>
            </tr>
          </thead>
          <tbody>
            {(rep?.rows ?? []).length === 0 && !busy && (
              <tr><td colSpan={7} className="muted" style={{ textAlign: 'center' }}>{t('bk.noRows')}</td></tr>
            )}
            {(rep?.rows ?? []).map((r) => (
              <tr key={r.reservation_id}>
                <td>{r.guest_name ?? <span className="muted">—</span>}</td>
                <td style={{ fontWeight: 600 }}>{r.room_number}</td>
                <td className="muted">{r.room_type_name}</td>
                <td>{r.checkin_date}</td>
                <td>{r.checkout_date}</td>
                <td>{r.guests}</td>
                <td>{cur(r.balance_cents)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {view === 'chart' && (
        <div style={{ height: Math.max(240, chartData.length * 36) }}>
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={chartData} layout="vertical">
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis type="number" />
              <YAxis dataKey="room" type="category" width={60} />
              <Tooltip />
              <Bar dataKey="balance" fill="#6366f1" name={t('rep.inhouse.balance')} />
            </BarChart>
          </ResponsiveContainer>
        </div>
      )}
    </div>
  )
}

// ------------------------------------------------------------------------- main courante

export function ReportsMainCourante() {
  const { t } = useTranslation()
  const gate = useReportGate()
  const [date, setDate] = useState(isoToday)
  const [rep, setRep] = useState<MainCouranteReport | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const load = useCallback(async () => {
    try {
      setBusy(true)
      setError(null)
      setRep(await api.reportMaincourante(date))
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }, [date])
  useEffect(() => { void load() }, [load])

  if (gate) return gate

  return (
    <div className="page">
      <PageHeaderWithBack title={t('rep.maincourante.title')} />
      <ErrorStrip error={error} />
      <DateBar
        label={t('rep.date')}
        date={date}
        onDate={setDate}
        onDownload={() => downloadCsv(`maincourante_${date}.csv`, [
          [t('res.guest'), t('rooms.number'), t('res.checkin'), t('res.checkout'), t('rep.mc.dailyRate'), t('rep.mc.prevBalance'), t('rep.mc.balance'), t('rep.mc.roomRate')],
          ...(rep?.rows ?? []).map((r) => [
            r.guest_name ?? '—', r.room_number, r.checkin_date, r.checkout_date,
            ((r.daily_rate_cents ?? 0) / 100).toFixed(2),
            (r.previous_balance_cents / 100).toFixed(2),
            (r.balance_cents / 100).toFixed(2),
            (r.room_rate_cents / 100).toFixed(2),
          ]),
        ])}
      />
      <p className="muted" style={{ fontSize: 13, marginTop: 0 }}>
        {t('bk.records', { n: rep?.count ?? 0 })} · {date}
      </p>
      <table className="hms">
        <thead>
          <tr>
            <th>{t('res.guest')}</th><th>{t('rooms.number')}</th><th>{t('res.checkin')}</th><th>{t('res.checkout')}</th>
            <th>{t('rep.mc.dailyRate')}</th><th>{t('rep.mc.prevBalance')}</th><th>{t('rep.mc.balance')}</th><th>{t('rep.mc.roomRate')}</th>
          </tr>
        </thead>
        <tbody>
          {(rep?.rows ?? []).length === 0 && !busy && (
            <tr><td colSpan={8} className="muted" style={{ textAlign: 'center' }}>{t('bk.noRows')}</td></tr>
          )}
          {(rep?.rows ?? []).map((r) => (
            <tr key={r.reservation_id}>
              <td>{r.guest_name ?? <span className="muted">—</span>}</td>
              <td style={{ fontWeight: 600 }}>{r.room_number}</td>
              <td>{r.checkin_date}</td>
              <td>{r.checkout_date}</td>
              <td>{r.daily_rate_cents != null ? money(r.daily_rate_cents, r.currency) : <span className="muted">—</span>}</td>
              <td>{money(r.previous_balance_cents, r.currency)}</td>
              <td>{money(r.balance_cents, r.currency)}</td>
              <td className="muted">{money(r.room_rate_cents, r.currency)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
