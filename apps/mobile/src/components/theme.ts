// G5: shared cross-platform design tokens for the mobile app.
// Kept in one file so screens stay declarative and light/dark can be swapped
// without touching layout code.
export interface Theme {
  bg: string
  card: string
  text: string
  muted: string
  accent: string
  danger: string
  warn: string
  ok: string
  border: string
  radius: number
}

export const light: Theme = {
  bg: '#f6f7f9',
  card: '#ffffff',
  text: '#14181f',
  muted: '#6b7280',
  accent: '#0f766e',
  danger: '#b91c1c',
  warn: '#b45309',
  ok: '#15803d',
  border: '#e5e7eb',
  radius: 14,
}

export const dark: Theme = {
  bg: '#0b0f14',
  card: '#151b23',
  text: '#f3f4f6',
  muted: '#9aa4b2',
  accent: '#2dd4bf',
  danger: '#f87171',
  warn: '#fbbf24',
  ok: '#4ade80',
  border: '#243040',
  radius: 14,
}

/** Cleanliness → colour mapping used by the housekeeping and board screens. */
export function cleanlinessColor(theme: Theme, cleanliness: string): string {
  switch (cleanliness) {
    case 'clean':
    case 'inspected':
      return theme.ok
    case 'cleaning':
      return theme.warn
    case 'out_of_order':
      return theme.danger
    default:
      return theme.muted
  }
}
