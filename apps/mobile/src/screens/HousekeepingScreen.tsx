// G5: Housekeeping tab — the cleaner's phone. Every tap stages the status
// change immediately (offline-first, README §11) and pushes when possible.
// Last-write-wins per room, which is exactly the server's update model (§11.4).
import React, { useCallback, useEffect, useState } from 'react'
import { FlatList, Pressable, Text, View } from 'react-native'
import type { RoomDto } from '@hms/shared-types'
import { api, loadSession } from '../api/client'
import { mobileSync } from '../api/sync'
import { Card, ErrorText, Loading, Row, Screen, theme } from '../components/ui'
import { cleanlinessColor } from '../components/theme'

const CYCLE = ['dirty', 'cleaning', 'clean', 'inspected', 'out_of_order'] as const

export function HousekeepingScreen() {
  const [rooms, setRooms] = useState<RoomDto[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(0)

  const load = useCallback(async () => {
    try {
      setError(null)
      setRooms(await api.listRooms())
    } catch (e) {
      setError(`Offline — showing last known in-house state (${String(e)})`)
      setRooms((prev) => prev ?? [])
    }
    setPending(await mobileSync.pendingCount())
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  const cycle = async (room: RoomDto) => {
    const idx = CYCLE.indexOf(room.cleanliness as (typeof CYCLE)[number])
    const next = CYCLE[(idx + 1) % CYCLE.length]
    // Optimistic local update, then durable staging + best-effort push.
    setRooms((prev) => (prev ? prev.map((r) => (r.id === room.id ? { ...r, cleanliness: next } : r)) : prev))
    const session = await loadSession()
    if (!session) {
      setError('Session expired — sign in again')
      return
    }
    await mobileSync.stage('update', 'room_status', room.id, { cleanliness: next }, session.user.hotel_id)
    const status = await mobileSync.syncOnce()
    setPending(status.pending)
    if (status.lastError) {
      setError(status.lastError)
    } else {
      setError(null)
    }
  }

  if (!rooms) return <Screen title="Housekeeping">{error ? <ErrorText message={error} /> : <Loading />}</Screen>

  return (
    <Screen title="Housekeeping" subtitle={`${rooms.length} rooms · ${pending} queued`}>
      {error ? <ErrorText message={error} /> : null}
      <FlatList
        data={rooms}
        keyExtractor={(r) => r.id}
        renderItem={({ item }) => (
          <Pressable onPress={() => void cycle(item)} accessibilityRole="button">
            <Card>
              <Row label={`Room ${item.room_number}`} value={item.cleanliness} tone={cleanlinessColor(theme, item.cleanliness)} />
              <View style={{ flexDirection: 'row', gap: 6 }}>
                {CYCLE.map((c) => (
                  <Text key={c} style={{ color: cleanlinessColor(theme, c), fontSize: 11 }}>
                    {c === item.cleanliness ? '●' : '○'}
                  </Text>
                ))}
              </View>
            </Card>
          </Pressable>
        )}
      />
    </Screen>
  )
}
