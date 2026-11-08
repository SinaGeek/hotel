// G5: Settings tab — session, offline queue depth, manual sync and sign-out.
// Also hosts the optional LAN Hub discovery (G4): if a hub answers on the
// hotel LAN, the pill shows "LAN hub reachable" and the hub URL is stored so a
// future release can route pushes through it while the WAN is down.
import React, { useCallback, useEffect, useState } from 'react'
import { TextInput } from 'react-native'
import { api, clearSession, loadSession, Session } from '../api/client'
import { mobileSync, SyncStatus } from '../api/sync'
import { Button, Card, ErrorText, Row, Screen, theme } from '../components/ui'

export function SettingsScreen() {
  const [session, setSession] = useState<Session | null>(null)
  const [status, setStatus] = useState<SyncStatus | null>(null)
  const [hub, setHub] = useState('http://192.168.1.50:8089')
  const [hubToken, setHubToken] = useState('')
  const [hubState, setHubState] = useState<string>('not checked')
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    setSession(await loadSession())
    setStatus(await mobileSync.syncOnce())
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const probeHub = async () => {
    try {
      const res = await fetch(`${hub.replace(/\/+$/, '')}/api/v1/lan/status`, {
        headers: { Authorization: `Bearer ${hubToken}` },
      })
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      const body = (await res.json()) as { hotel_id: number; pending_upstream_ops: number }
      setHubState(`reachable · hotel ${body.hotel_id} · ${body.pending_upstream_ops} pending upstream`)
    } catch (e) {
      setHubState(`unreachable (${String(e)})`)
    }
  }

  return (
    <Screen title="Settings" subtitle={session ? `${session.user.full_name} · ${session.user.role}` : 'not signed in'}>
      {error ? <ErrorText message={error} /> : null}
      <Card>
        <Row label="API" value={api.baseUrl} />
        <Row label="Pending writes" value={String(status?.pending ?? 0)} tone={status?.pending ? theme.warn : theme.ok} />
        <Row label="Last sync" value={status?.lastSyncedAt ?? '—'} />
        {status?.lastError ? <ErrorText message={status.lastError} /> : null}
        <Button label="Sync now" onPress={() => void refresh()} />
      </Card>

      <Card>
        <Row label="LAN hub" value={hubState} />
        <TextInput
          value={hub}
          onChangeText={setHub}
          autoCapitalize="none"
          placeholder="http://hub-ip:8089"
          placeholderTextColor={theme.muted}
          style={{
            color: theme.text,
            borderColor: theme.border,
            borderWidth: 1,
            borderRadius: 8,
            paddingHorizontal: 10,
            paddingVertical: 8,
          }}
        />
        <TextInput
          value={hubToken}
          onChangeText={setHubToken}
          autoCapitalize="none"
          secureTextEntry
          placeholder="LAN hub bearer token"
          placeholderTextColor={theme.muted}
          style={{ color: theme.text, borderColor: theme.border, borderWidth: 1, borderRadius: 8, paddingHorizontal: 10, paddingVertical: 8 }}
        />
        <Button label="Test hub" onPress={() => void probeHub()} />
      </Card>

      <Button
        label="Sign out"
        onPress={() => {
          void (async () => {
            try {
              await api.logout()
            } catch {
              // offline sign-out still clears the local session
            }
            await clearSession()
            setSession(null)
            setError('Signed out')
          })()
        }}
      />
    </Screen>
  )
}
