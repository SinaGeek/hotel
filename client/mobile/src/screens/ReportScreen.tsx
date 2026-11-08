// G5: Today tab — the three numbers a manager checks on a phone: occupancy,
// room revenue, payments. Reuses GET /reports/summary (README §12).
import React, { useCallback, useEffect, useState } from 'react'
import type { SummaryReport } from '@hms/shared-types'
import { api } from '../api/client'
import { Button, Card, ErrorText, Loading, Row, Screen, theme } from '../components/ui'
import { formatMoney, todayISO } from '../utils'

export function ReportScreen() {
  const [report, setReport] = useState<SummaryReport | null>(null)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(async () => {
    try {
      setError(null)
      const today = todayISO()
      setReport(await api.reportSummary(today, today))
    } catch (e) {
      setError(String(e))
    }
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  if (!report) return <Screen title="Today">{error ? <ErrorText message={error} /> : <Loading />}</Screen>

  return (
    <Screen title="Today" subtitle={todayISO()}>
      {error ? <ErrorText message={error} /> : null}
      <Card>
        <Row label="Occupancy" value={`${report.occupancy_pct.toFixed(1)} %`} tone={theme.accent} />
        <Row label="Nights sold" value={`${report.occupied_nights} / ${report.available_room_nights}`} />
        <Row label="ADR" value={report.adr_cents == null ? '—' : formatMoney(report.adr_cents, report.currency)} />
        <Row label="RevPAR" value={report.revpar_cents == null ? '—' : formatMoney(report.revpar_cents, report.currency)} />
      </Card>
      <Card>
        <Row label="Room revenue" value={formatMoney(report.room_revenue_cents, report.currency)} tone={theme.ok} />
        <Row label="Extra revenue" value={formatMoney(report.extra_revenue_cents, report.currency)} />
        <Row label="Payments" value={formatMoney(report.payments_collected_cents, report.currency)} />
      </Card>
      <Button label="Refresh" onPress={() => void load()} />
    </Screen>
  )
}
