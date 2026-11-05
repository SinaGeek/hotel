// G5: mobile navigation.
//
// Five bottom tabs, each mapping 1:1 to an existing server API surface, so the
// mobile app adds no new endpoints:
//   Board        → GET /board,          PATCH /rooms/:id/cleanliness
//   Bookings     → GET /reservations,   POST /reservations/:id/check-in|check-out
//   Housekeeping → GET /rooms,          PATCH /rooms/:id/cleanliness (offline-staged)
//   Reports      → GET /reports/summary (today)
//   Settings     → session, sync status, force sync
import React from 'react'
import { Text } from 'react-native'
import { createBottomTabNavigator } from '@react-navigation/bottom-tabs'
import { BoardScreen } from '../screens/BoardScreen'
import { BookingsScreen } from '../screens/BookingsScreen'
import { HousekeepingScreen } from '../screens/HousekeepingScreen'
import { ReportScreen } from '../screens/ReportScreen'
import { SettingsScreen } from '../screens/SettingsScreen'

export type RootTabs = {
  Board: undefined
  Bookings: undefined
  Housekeeping: undefined
  Report: undefined
  Settings: undefined
}

const Tab = createBottomTabNavigator<RootTabs>()

const ICONS: Record<keyof RootTabs, string> = {
  Board: '🛏️',
  Bookings: '📅',
  Housekeeping: '🧹',
  Report: '📊',
  Settings: '⚙️',
}

export function RootNavigator() {
  return (
    <Tab.Navigator
      screenOptions={({ route }) => ({
        headerShown: false,
        tabBarActiveTintColor: '#2dd4bf',
        tabBarInactiveTintColor: '#9aa4b2',
        tabBarStyle: { backgroundColor: '#151b23', borderTopColor: '#243040' },
        tabBarIcon: ({ color }) => <TabIcon glyph={ICONS[route.name]} color={color} />,
      })}
    >
      <Tab.Screen name="Board" component={BoardScreen} />
      <Tab.Screen name="Bookings" component={BookingsScreen} />
      <Tab.Screen name="Housekeeping" component={HousekeepingScreen} />
      <Tab.Screen name="Report" component={ReportScreen} options={{ title: 'Today' }} />
      <Tab.Screen name="Settings" component={SettingsScreen} />
    </Tab.Navigator>
  )
}

function TabIcon({ glyph, color }: { glyph: string; color: string }) {
  // Emoji glyphs keep the bundle framework-free (no icon font dependency).
  return <Text style={{ fontSize: 18, color }}>{glyph}</Text>
}
