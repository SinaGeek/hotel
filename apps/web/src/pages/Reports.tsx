// Reports (README §10): earnings by channel/day/month/room, occupancy, ADR,
// RevPAR — read daily_revenue server-side. Charts via Recharts; CSV export +
// print (browser print-to-PDF).
import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  Bar, BarChart, CartesianGrid, Legend, ResponsiveContainer, Tooltip, XAxis, YAxis,
} from 'recharts'
import { api, ApiError, sessionHasPermission } from '../api/client'
import { useAuth } from '../api/auth'
import type { ChannelRevenue, DayReport, RoomRevenue, SummaryReport } from '../types/generated/api'

// Revenue reporting basis (README §10 open question — toggle IS the answer for
// now): "accrued" recognizes revenue when the stay happens (daily_revenue wall
// values); "collected" shows the cash basis (payments actually recorded). The
// server already returns both series in every response.
type Basis = 'accrued' | 'collected'

const DAY_MS = 86_400_000
const iso = (d: Date) => d.toISOString().slice(0, 10)
const money = (cents: number, currency: string) =>
  new Intl.NumberFormat(undefined, { style: 'currency', currency, maximumFractionDigits: 2 }).format(cents / 100)
const money0 = (cents: number, currency: string) =>
  new Intl.NumberFormat(undefined, { style: 'currency', currency, maximumFractionDigits: 0 }).format(cents / 100)

// CSV download helper (BOM keeps Excel happy with UTF-8)
function downloadCsv(filename: string, rows: (string | number)[][]) {
  const csv = '\ufeff' + rows.map((r) => r.map((c) => (typeof c === 'string' && c.includes(',') ? `"${c}"` : String(c))).join(',')).join('\r\n')
  const url = URL.createObjectURL(new Blob([csv], { type: 'text/csv;charset=utf-8' }))
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

export function Reports() {
  const { t } = useTranslation()
  const { session } = useAuth()
  const can = (p: string) => sessionHasPermission(session, p)
  const isHk = session?.user.role === 'housekeeping'

  const [from, setFrom] = useState(() => iso(new Date(Date.now() - 29 * DAY_MS)))
  const [to, setTo] = useState(() => iso(new Date(Date.now() + DAY_MS))) // exclusive end
  const [summary, setSummary] = useState<SummaryReport | null>(null)
  const [byDay, setByDay] = useState<DayReport[]>([])
  const [byChannel, setByChannel] = useState<ChannelRevenue[]>([])
  const [byRoom, setByRoom] = useState<RoomRevenue[]>([])
  const [basis, setBasis] = useState<Basis>('accrued')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const load = useCallback(async () => {
    try {
      setError(null)
      setBusy(true)
      const [s, d, c, r] = await Promise.all([
        api.reportSummary(from, to),
        api.reportsByDay(from, to),
        api.reportsByChannel(from, to),
        api.reportsByRoom(from, to),
      ])
      setSummary(s)
      setByDay(d)
      setByChannel(c)
      setByRoom(r)
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }, [from, to])

  useEffect(() => { if (session) void load() }, [load, session])

  const currency = summary?.currency ?? 'TRY'
  const cur = (c: number) => money0(c, currency)

  const chartData = useMemo(
    () => byDay.map((d) => ({
      date: d.date.slice(5),
      Room: d.room_revenue_cents / 100,
      Extras: d.extra_revenue_cents / 100,
      Collected: d.payments_collected_cents / 100,
    })),
    [byDay],
  )
  const showAccrued = basis === 'accrued'
  const onlyDaysWithMoney = chartData.some((d) => d.Room || d.Extras || d.Collected)

  // Monthly rollup: group the same byDay rows by yyyy-mm — the reference has no
  // monthly endpoint, and the aggregate over our own rows is exact.
  const monthData = useMemo(
    () => Object.entries(
      byDay.reduce<Record<string, { month: string; Room: number; Extras: number; Collected: number }>>((acc, d) => {
        const m = d.date.slice(0, 7)
        acc[m] ??= { month: m, Room: 0, Extras: 0, Collected: 0 }
        acc[m].Room += d.room_revenue_cents / 100
        acc[m].Extras += d.extra_revenue_cents / 100
        acc[m].Collected += d.payments_collected_cents / 100
        return acc
      }, {}),
    ).sort(([a], [b]) => a.localeCompare(b)).map(([, v]) => v),
    [byDay],
  )
  const showMonths = monthData.length > 1 || (monthData[0]?.month.slice(0, 7) ?? '') !== iso(new Date()).slice(0, 7)

  const channelData = useMemo(
    () => byChannel.map((c) => ({
      channel: c.channel_name,
      Room: c.room_revenue_cents / 100,
      Extras: c.extra_revenue_cents / 100,
      Collected: c.payments_collected_cents / 100,
    })),
    [byChannel],
  )

  if (isHk || !can('report_view_tenant')) {
    return <div className="page"><h2>{t('reports.title')}</h2><p className="muted">{t('reports.noAccess')}</p></div>
  }

  const preset = (days: number) => {
    setFrom(iso(new Date(Date.now() - (days - 1) * DAY_MS)))
    setTo(iso(new Date(Date.now() + DAY_MS)))
  }

  return (
    <div className="page">
      <div className="row">
        <h2 style={{ margin: 0, flexGrow: 1 }}>{t('reports.title')}</h2>
        <button className="ghost" onClick={() => preset(1)}>{t('reports.today')}</button>
        <button className="ghost" onClick={() => preset(7)}>{t('reports.last7')}</button>
        <button className="ghost" onClick={() => preset(30)}>{t('reports.last30')}</button>
        <button className="ghost" onClick={() => preset(90)}>{t('reports.last90')}</button>
        <input type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
        <input type="date" value={to} onChange={(e) => setTo(e.target.value)} />
        {/* revenue basis (README §10): accrued = recognized with the stay,
            collected = cash received. Chart + channel table follow the basis. */}
        <div className="seg" role="tablist" aria-label={t('reports.basis')}>
          <button className={showAccrued ? 'on' : ''} onClick={() => setBasis('accrued')}>{t('reports.accrued')}</button>
          <button className={showAccrued ? '' : 'on'} onClick={() => setBasis('collected')}>{t('reports.collected')}</button>
        </div>
        <button className="primary" disabled={busy} onClick={() => void load()}>{busy ? t('common.loading') : t('reports.refresh')}</button>
        <button className="ghost" onClick={() => window.print()}>{t('reports.printPdf')}</button>
      </div>
      {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}

      {/* KPI summary */}
      <div className="kpis">
        <div className="kpi"><b>{summary ? cur(summary.room_revenue_cents) : '—'}</b><span>{t('reports.roomRevenue')}</span></div>
        <div className="kpi"><b>{summary ? cur(summary.extra_revenue_cents) : '—'}</b><span>{t('reports.extraRevenue')}</span></div>
        <div className="kpi"><b>{summary ? cur(summary.payments_collected_cents) : '—'}</b><span>{t('reports.collected')}</span></div>
        <div className="kpi"><b>{summary ? `${summary.occupancy_pct.toFixed(1)}%` : '—'}</b><span>{t('reports.occupancy')} ({summary?.occupied_nights ?? 0}/{summary?.available_room_nights ?? 0})</span></div>
        <div className="kpi"><b>{summary?.adr_cents != null ? money(summary.adr_cents, currency) : '—'}</b><span>{t('reports.adr')}</span></div>
        <div className="kpi"><b>{summary?.revpar_cents != null ? money(summary.revpar_cents, currency) : '—'}</b><span>{t('reports.revpar')}</span></div>
      </div>

      {/* revenue per day chart */}
      <div className="card" style={{ marginBottom: 12 }}>
        <div className="row">
          <h3 style={{ margin: 0, flexGrow: 1 }}>{t('reports.byDay')}</h3>
          <button className="ghost" onClick={() => downloadCsv(`revenue_days_${from}_${to}.csv`, [
            [t('reports.date'), t('reports.roomRevenue'), t('reports.extraRevenue'), t('reports.collected'), t('reports.occupiedNights')],
            ...byDay.map((d) => [d.date, (d.room_revenue_cents / 100).toFixed(2), (d.extra_revenue_cents / 100).toFixed(2), (d.payments_collected_cents / 100).toFixed(2), d.occupied_nights]),
          ])}>{t('reports.exportCsv')}</button>
        </div>
        {onlyDaysWithMoney ? (
          <ResponsiveContainer width="100%" height={260}>
            <BarChart data={chartData}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis dataKey="date" />
              <YAxis />
              <Tooltip />
              <Legend />
              {showAccrued ? (
                <>
                  <Bar dataKey="Room" fill="#2e5bff" name={t('reports.roomRevenue')} />
                  <Bar dataKey="Extras" fill="#2b9a5b" name={t('reports.extraRevenue')} />
                </>
              ) : (
                <Bar dataKey="Collected" fill="#e5a52b" name={t('reports.collected')} />
              )}
            </BarChart>
          </ResponsiveContainer>
        ) : (
          <p className="muted">{t('reports.noData')}</p>
        )}
      </div>

      {/* monthly rollup (grouped bars when the range spans months) */}
      {showMonths && (
        <div className="card" style={{ marginBottom: 12 }}>
          <div className="row">
            <h3 style={{ margin: 0, flexGrow: 1 }}>{t('reports.byMonth')}</h3>
            <button className="ghost" onClick={() => downloadCsv(`revenue_months_${from}_${to}.csv`, [
              [t('reports.month'), t('reports.roomRevenue'), t('reports.extraRevenue'), t('reports.collected')],
              ...monthData.map((m) => [m.month, m.Room.toFixed(2), m.Extras.toFixed(2), m.Collected.toFixed(2)]),
            ])}>{t('reports.exportCsv')}</button>
          </div>
          <ResponsiveContainer width="100%" height={220}>
            <BarChart data={monthData}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis dataKey="month" />
              <YAxis />
              <Tooltip />
              <Legend />
              {showAccrued ? (
                <>
                  <Bar dataKey="Room" fill="#2e5bff" name={t('reports.roomRevenue')} />
                  <Bar dataKey="Extras" fill="#2b9a5b" name={t('reports.extraRevenue')} />
                </>
              ) : (
                <Bar dataKey="Collected" fill="#e5a52b" name={t('reports.collected')} />
              )}
            </BarChart>
          </ResponsiveContainer>
        </div>
      )}

      <div style={{ display: 'flex', gap: 12, flexWrap: 'wrap' }}>
        {/* by channel */}
        <div className="card" style={{ flex: '1 1 380px', minWidth: 320 }}>
          <div className="row">
            <h3 style={{ margin: 0, flexGrow: 1 }}>{t('reports.byChannel')}</h3>
            <button className="ghost" onClick={() => downloadCsv(`revenue_channels_${from}_${to}.csv`, [
              [t('nav.reservations') + ' channel', t('reports.roomRevenue'), t('reports.extraRevenue'), t('reports.collected'), t('reports.occupiedNights')],
              ...byChannel.map((c) => [c.channel_name, (c.room_revenue_cents / 100).toFixed(2), (c.extra_revenue_cents / 100).toFixed(2), (c.payments_collected_cents / 100).toFixed(2), c.occupied_nights]),
            ])}>{t('reports.exportCsv')}</button>
          </div>
          <div className="table-wrap"><table className="hms">
            <thead><tr><th>{t('reports.channel')}</th><th>{t('reports.roomRevenue')}</th><th>{t('reports.extraRevenue')}</th><th>{t('reports.collected')}</th><th>{t('reports.occupiedNights')}</th></tr></thead>
            <tbody>
              {byChannel.map((c) => (
                <tr key={c.channel_id}>
                  <td>{c.channel_name}</td>
                  <td>{cur(c.room_revenue_cents)}</td>
                  <td>{cur(c.extra_revenue_cents)}</td>
                  <td>{cur(c.payments_collected_cents)}</td>
                  <td>{c.occupied_nights}</td>
                </tr>
              ))}
            </tbody>
          </table></div>
        </div>

        {/* by room */}
        <div className="card" style={{ flex: '1 1 380px', minWidth: 320 }}>
          <div className="row">
            <h3 style={{ margin: 0, flexGrow: 1 }}>{t('reports.byRoom')}</h3>
            <button className="ghost" onClick={() => downloadCsv(`revenue_rooms_${from}_${to}.csv`, [
              [t('rooms.number'), t('rooms.type'), t('reports.roomRevenue'), t('reports.extraRevenue'), t('reports.occupiedNights')],
              ...byRoom.map((r) => [r.room_number, r.room_type_name, (r.room_revenue_cents / 100).toFixed(2), (r.extra_revenue_cents / 100).toFixed(2), r.occupied_nights]),
            ])}>{t('reports.exportCsv')}</button>
          </div>
          <div className="table-wrap"><table className="hms">
            <thead><tr><th>{t('rooms.number')}</th><th>{t('rooms.type')}</th><th>{t('reports.roomRevenue')}</th><th>{t('reports.extraRevenue')}</th><th>{t('reports.occupiedNights')}</th></tr></thead>
            <tbody>
              {byRoom.map((r) => (
                <tr key={r.room_id}>
                  <td>{r.room_number}</td>
                  <td>{r.room_type_name}</td>
                  <td>{cur(r.room_revenue_cents)}</td>
                  <td>{cur(r.extra_revenue_cents)}</td>
                  <td>{r.occupied_nights}</td>
                </tr>
              ))}
            </tbody>
          </table></div>
        </div>
      </div>

      {/* per-channel money chart (all channels present) */}
      <div className="card" style={{ marginTop: 12 }}>
        <h3 style={{ margin: '0 0 8px' }}>{t('reports.byChannelChart')}</h3>
        <ResponsiveContainer width="100%" height={Math.max(220, channelData.length * 44)}>
          <BarChart data={channelData} layout="vertical">
            <CartesianGrid strokeDasharray="3 3" />
            <XAxis type="number" />
            <YAxis dataKey="channel" type="category" width={100} />
            <Tooltip />
            <Legend />
            {showAccrued ? (
              <>
                <Bar dataKey="Room" fill="#2e5bff" name={t('reports.roomRevenue')} />
                <Bar dataKey="Extras" fill="#2b9a5b" name={t('reports.extraRevenue')} />
              </>
            ) : (
              <Bar dataKey="Collected" fill="#e5a52b" name={t('reports.collected')} />
            )}
          </BarChart>
        </ResponsiveContainer>
      </div>
    </div>
  )
}
