// G5: small shared primitives (no UI framework dependency on purpose — the
// mobile app must stay tiny and use only React Native primitives).
import React from 'react'
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from 'react-native'
import { dark, Theme } from './theme'

export const theme: Theme = dark

export function Screen({ children, title, subtitle }: { children: React.ReactNode; title: string; subtitle?: string }) {
  return (
    <View style={styles.screen}>
      <Text style={styles.title}>{title}</Text>
      {subtitle ? <Text style={styles.subtitle}>{subtitle}</Text> : null}
      {children}
    </View>
  )
}

export function Card({ children, onPress }: { children: React.ReactNode; onPress?: () => void }) {
  const body = <View style={styles.card}>{children}</View>
  return onPress ? <Pressable onPress={onPress}>{body}</Pressable> : body
}

export function Row({ label, value, tone }: { label: string; value: string; tone?: string }) {
  return (
    <View style={styles.row}>
      <Text style={styles.rowLabel}>{label}</Text>
      <Text style={[styles.rowValue, tone ? { color: tone } : null]}>{value}</Text>
    </View>
  )
}

export function Button({ label, onPress, disabled }: { label: string; onPress: () => void; disabled?: boolean }) {
  return (
    <Pressable
      accessibilityRole="button"
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => [styles.button, disabled && styles.buttonDisabled, pressed && styles.buttonPressed]}
    >
      <Text style={styles.buttonText}>{label}</Text>
    </Pressable>
  )
}

export function Loading({ message = 'Loading…' }: { message?: string }) {
  return (
    <View style={styles.loading}>
      <ActivityIndicator color={theme.accent} />
      <Text style={styles.subtitle}>{message}</Text>
    </View>
  )
}

export function ErrorText({ message }: { message: string }) {
  return <Text style={[styles.subtitle, { color: theme.danger }]}>{message}</Text>
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: theme.bg, padding: 16, gap: 12 },
  title: { color: theme.text, fontSize: 22, fontWeight: '700' },
  subtitle: { color: theme.muted, fontSize: 13 },
  card: {
    backgroundColor: theme.card,
    borderRadius: theme.radius,
    borderWidth: StyleSheet.hairlineWidth,
    borderColor: theme.border,
    padding: 14,
    gap: 8,
  },
  row: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' },
  rowLabel: { color: theme.muted, fontSize: 13 },
  rowValue: { color: theme.text, fontSize: 14, fontWeight: '600' },
  button: {
    backgroundColor: theme.accent,
    paddingVertical: 11,
    paddingHorizontal: 16,
    borderRadius: 10,
    alignItems: 'center',
  },
  buttonPressed: { opacity: 0.8 },
  buttonDisabled: { opacity: 0.4 },
  buttonText: { color: '#04110f', fontWeight: '700' },
  loading: { alignItems: 'center', gap: 8, paddingVertical: 24 },
})
