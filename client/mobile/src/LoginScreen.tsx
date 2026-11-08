// G5: mobile sign-in. Same /auth/login contract as web and desktop.
import React, { useState } from 'react'
import { TextInput, View } from 'react-native'
import { api, saveSession } from './api/client'
import { Button, Card, ErrorText, Row, Screen, theme } from './components/ui'

export function LoginScreen({ onDone }: { onDone: () => void }) {
  const [hotelId, setHotelId] = useState('')
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const submit = async () => {
    setBusy(true)
    setError(null)
    try {
      const res = await api.login(Number(hotelId), username, password)
      await saveSession({ token: res.token, user: res.user, permissions: res.permissions })
      onDone()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const field = (value: string, set: (v: string) => void, placeholder: string, secure = false) => (
    <TextInput
      value={value}
      onChangeText={set}
      placeholder={placeholder}
      placeholderTextColor={theme.muted}
      secureTextEntry={secure}
      autoCapitalize="none"
      style={{
        color: theme.text,
        borderColor: theme.border,
        borderWidth: 1,
        borderRadius: 8,
        paddingHorizontal: 10,
        paddingVertical: 9,
      }}
    />
  )

  return (
    <Screen title="HMS" subtitle="Sign in to your hotel">
      <Card>
        <Row label="Hotel code" value="" />
        {field(hotelId, setHotelId, '4442')}
        {field(username, setUsername, 'username')}
        {field(password, setPassword, 'password', true)}
        {error ? <ErrorText message={error} /> : null}
        <View>
          <Button label={busy ? 'Signing in…' : 'Sign in'} disabled={busy} onPress={() => void submit()} />
        </View>
      </Card>
    </Screen>
  )
}
