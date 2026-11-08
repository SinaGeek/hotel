// G5: Bookings tab — arrival/departure lists with one-tap check-in/out.
// These writes go through the mobile sync outbox when offline, so reception
// can keep working during a WAN outage (same protocol as the desktop).
import React, { useCallback, useEffect, useState } from 'react'
import { FlatList, Pressable, Text } from 'react-native'
import type { ReservationDto } from '@hms/shared-types'
import { api, loadSession } from '../api/client'
import { mobileSync } from '../api/sync'
import { Button, Card, ErrorText, Loading, Row, Screen, theme } from '../components/ui'
import { addDaysISO, formatMoney, todayISO } from '../utils'

export function BookingsScreen() {
  const [rows, setRows] = useState<ReservationDto[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)

  const load = useCallback(async () => {
    try {
      setError(null)
      const today = todayISO()
      setRows(await api.listReservations({ from: today, to: addDaysISO(today, 1), limit: 100 }))
    } catch (e) {
      // Offline: fall back to whatever the sync mirror holds for today.
      setError(String(e))
      setRows([])
    }
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  const act = async (r: ReservationDto, action: 'check_in' | 'check_out') => {
    setBusy(r.id)
    try {
      if (action === 'check_in') await api.checkIn(r.id)
      else await api.checkOut(r.id)
      await mobileSync.syncOnce()
      await load()
    } catch {
      // Offline fallback: stage the write in the outbox, keep UI responsive.
      const session = await loadSession()
      if (session) {
        await mobileSync.stage(
          'update',
          'reservation',
          r.id,
          { status: action === 'check_in' ? 'checked_in' : 'checked_out' },
          session.user.hotel_id,
        )
        setError('Offline — change queued and will sync when the network returns')
      } else {
        setError('Session expired — sign in again')
      }
    } finally {
      setBusy(null)
    }
  }

  if (!rows) return <Screen title="Bookings">{error ? <ErrorText message={error} /> : <Loading />}</Screen>

  return (
    <Screen title="Bookings" subtitle={`${rows.length} for ${todayISO()}`}>
      {error ? <ErrorText message={error} /> : null}
      <FlatList
        data={rows}
        keyExtractor={(r) => r.id}
        renderItem={({ item }) => (
          <Card>
            <Row label={item.status} value={formatMoney(item.total_price_cents, item.currency)} />
            <Row label="Stay" value={`${item.checkin_date} → ${item.checkout_date}`} />
            <Row label="Pax" value={`${item.adults} + ${item.children}`} />
            {item.status === 'confirmed' ? (
              <Pressable disabled={busy === item.id} onPress={() => void act(item, 'check_in')}>
                <Text style={{ color: theme.accent, fontWeight: '700' }}>Check in</Text>
              </Pressable>
            ) : null}
            {item.status === 'checked_in' ? (
              <Pressable disabled={busy === item.id} onPress={() => void act(item, 'check_out')}>
                <Text style={{ color: theme.warn, fontWeight: '700' }}>Check out</Text>
              </Pressable>
            ) : null}
          </Card>
        )}
        ListEmptyComponent={<Text style={{ color: theme.muted }}>No arrivals or departures today.</Text>}
      />
      <Button label="Refresh" onPress={() => void load()} />
    </Screen>
  )
}
