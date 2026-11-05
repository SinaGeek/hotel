// G5: Board tab — today's rooms with one-tap cleanliness advance.
// Data source: GET /board (same endpoint + tenant-scoped read cache as web).
import React, { useCallback, useEffect, useState } from 'react'
import { FlatList, ScrollView } from 'react-native'
import type { BoardResponse, BoardRoom } from '@hms/shared-types'
import { api } from '../api/client'
import { Card, ErrorText, Loading, Row, Screen, theme } from '../components/ui'
import { todayISO } from '../utils'

const NEXT_CLEANLINESS: Record<string, string> = {
  dirty: 'cleaning',
  cleaning: 'clean',
  clean: 'inspected',
  inspected: 'dirty',
  out_of_order: 'clean',
}

export function BoardScreen() {
  const [data, setData] = useState<BoardResponse | null>(null)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(async () => {
    try {
      setError(null)
      const today = todayISO()
      setData(await api.board(today, today))
    } catch (e) {
      setError(String(e))
    }
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  const advance = async (room: BoardRoom) => {
    const next = NEXT_CLEANLINESS[room.cleanliness] ?? 'clean'
    try {
      await api.setCleanliness(room.id, next)
      await load()
    } catch (e) {
      setError(String(e))
    }
  }

  if (!data) return <Screen title="Board">{error ? <ErrorText message={error} /> : <Loading />}</Screen>

  const day = data.occupancy[0]
  return (
    <Screen title="Board" subtitle={todayISO()}>
      <ScrollView horizontal showsHorizontalScrollIndicator={false}>
        <Row label="Occupied" value={day ? `${day.occupied} / ${day.occupied + day.available}` : '—'} />
      </ScrollView>
      {error ? <ErrorText message={error} /> : null}
      <FlatList
        data={data.rooms}
        keyExtractor={(r) => r.id}
        renderItem={({ item }) => {
          const stay = item.stays[0]
          return (
            <Card onPress={() => void advance(item)}>
              <Row label={`Room ${item.room_number} · ${item.room_type_name}`} value={item.cleanliness} tone={theme.accent} />
              <Row
                label={stay ? (stay.guest_name ?? 'Guest') : 'Vacant'}
                value={stay ? `${stay.checkin_date} → ${stay.checkout_date}` : '—'}
              />
            </Card>
          )
        }}
      />
    </Screen>
  )
}
