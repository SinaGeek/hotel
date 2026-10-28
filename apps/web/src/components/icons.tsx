// Inline SVG icon set (no icon library installed by design).
// All icons are stroke-based, inherit currentColor, and size via props.
import type { JSX } from 'react'

interface IconProps {
  size?: number
}

function base(size: number, children: JSX.Element): JSX.Element {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  )
}

export function IconDashboard({ size = 18 }: IconProps) {
  return base(size, <>
    <rect x="3" y="3" width="7" height="9" rx="1.5" />
    <rect x="14" y="3" width="7" height="5" rx="1.5" />
    <rect x="14" y="12" width="7" height="9" rx="1.5" />
    <rect x="3" y="16" width="7" height="5" rx="1.5" />
  </>)
}

export function IconCalendar({ size = 18 }: IconProps) {
  return base(size, <>
    <rect x="3" y="5" width="18" height="16" rx="2" />
    <path d="M8 3v4M16 3v4M3 10h18" />
  </>)
}

export function IconUsers({ size = 18 }: IconProps) {
  return base(size, <>
    <circle cx="9" cy="8" r="3.5" />
    <path d="M2.5 20c0-3.3 2.9-5.5 6.5-5.5s6.5 2.2 6.5 5.5" />
    <path d="M16 4.6a3.5 3.5 0 0 1 0 6.8M17.5 14.7c2.3.6 4 2.3 4 5.3" />
  </>)
}

export function IconList({ size = 18 }: IconProps) {
  return base(size, <path d="M9 6h12M9 12h12M9 18h12M4.5 6h.01M4.5 12h.01M4.5 18h.01" />)
}

export function IconBed({ size = 18 }: IconProps) {
  return base(size, <path d="M3 7v11M3 15h18M21 18v-6a2 2 0 0 0-2-2H9v5M6.5 11.5a1.75 1.75 0 1 0 0-.01M3 18v1M21 18v1" />)
}

export function IconChart({ size = 18 }: IconProps) {
  return base(size, <path d="M4 20V10M10 20V4M16 20v-7M22 20H2" />)
}

export function IconGear({ size = 18 }: IconProps) {
  return base(size, <>
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1 1.55V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1-1.55 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87 1.7 1.7 0 0 0-1.55-1H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.55-1 1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34h0a1.7 1.7 0 0 0 1-1.55V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1 1.55h0a1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87v0a1.7 1.7 0 0 0 1.55 1H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.55 1z" />
  </>)
}

export function IconBell({ size = 18 }: IconProps) {
  return base(size, <>
    <path d="M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9" />
    <path d="M13.7 21a2 2 0 0 1-3.4 0" />
  </>)
}

export function IconGlobe({ size = 18 }: IconProps) {
  return base(size, <>
    <circle cx="12" cy="12" r="9" />
    <path d="M3 12h18M12 3a14.5 14.5 0 0 1 0 18 14.5 14.5 0 0 1 0-18z" />
  </>)
}

export function IconMoon({ size = 18 }: IconProps) {
  return base(size, <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />)
}

export function IconSun({ size = 18 }: IconProps) {
  return base(size, <>
    <circle cx="12" cy="12" r="4" />
    <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
  </>)
}

export function IconChevronRight({ size = 16 }: IconProps) {
  return base(size, <path d="M9 18l6-6-6-6" />)
}

export function IconPlus({ size = 20 }: IconProps) {
  return base(size, <path d="M12 5v14M5 12h14" />)
}

export function IconLogout({ size = 18 }: IconProps) {
  return base(size, <>
    <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
    <path d="M16 17l5-5-5-5M21 12H9" />
  </>)
}

export function IconHotel({ size = 22 }: IconProps) {
  return base(size, <>
    <path d="M4 21V5a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v16M16 9h2a2 2 0 0 1 2 2v10M2 21h20" />
    <path d="M8 7h2M12 7h2M8 11h2M12 11h2M8 15h2M12 15h2" />
  </>)
}
