// G5: mobile app root. Restores the session, then either the sign-in screen or
// the five-tab reception companion (Board / Bookings / Housekeeping / Today /
// Settings). Sync kicks off on mount and every time the app returns to focus.
import React, { useEffect, useState } from 'react'
import { AppState, StatusBar, View } from 'react-native'
import { NavigationContainer, DarkTheme } from '@react-navigation/native'
import { loadSession } from './src/api/client'
import { mobileSync } from './src/api/sync'
import { LoginScreen } from './src/LoginScreen'
import { RootNavigator } from './src/navigation/RootNavigator'
import { Loading, theme } from './src/components/ui'

export default function App() {
  const [ready, setReady] = useState(false)
  const [signedIn, setSignedIn] = useState(false)

  useEffect(() => {
    void (async () => {
      setSignedIn(Boolean(await loadSession()))
      setReady(true)
      await mobileSync.init()
      void mobileSync.syncOnce()
    })()
  }, [])

  useEffect(() => {
    const sub = AppState.addEventListener('change', (state) => {
      if (state === 'active') void mobileSync.syncOnce()
    })
    return () => sub.remove()
  }, [])

  if (!ready) {
    return (
      <View style={{ flex: 1, backgroundColor: theme.bg, justifyContent: 'center' }}>
        <Loading message="Starting HMS…" />
      </View>
    )
  }

  return (
    <NavigationContainer theme={DarkTheme}>
      <StatusBar barStyle="light-content" />
      {signedIn ? <RootNavigator /> : <LoginScreen onDone={() => setSignedIn(true)} />}
    </NavigationContainer>
  )
}
