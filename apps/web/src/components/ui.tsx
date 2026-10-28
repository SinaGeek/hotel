// Shared UI primitives matching the reference design language:
// page headers, soft status pills, filter rows, and list footers.
import type { ReactNode } from 'react'

export function PageHeader({ title, subtitle, actions }: {
  title: string
  subtitle?: string
  actions?: ReactNode
}) {
  return (
    <div style={{ display: 'flex', alignItems: 'flex-start', gap: 12, marginBottom: 14, flexWrap: 'wrap' }}>
      <div style={{ flexGrow: 1, minWidth: 0 }}>
        <h2 style={{ margin: 0 }}>{title}</h2>
        {subtitle && <div className="muted" style={{ marginTop: 2 }}>{subtitle}</div>}
      </div>
      {actions && <div className="row" style={{ gap: 8 }}>{actions}</div>}
    </div>
  )
}

type PillTone = 'green' | 'gold' | 'gray' | 'red' | 'violet'

const TONES: Record<string, PillTone> = {
  // reservation statuses
  confirmed: 'violet',
  checked_in: 'green',
  in_house: 'green',
  checked_out: 'gray',
  cancelled: 'red',
  conflict: 'gold',
  pending: 'gold',
  blocked: 'gray',
  out_of_service: 'red',
  out_of_order: 'red',
  // room cleanliness
  clean: 'green',
  inspected: 'violet',
  cleaning: 'gold',
  dirty: 'red',
  // generic
  active: 'green',
  inactive: 'gray',
  upcoming: 'gray',
}

export function StatusPill({ status, label }: { status: string; label?: string | null }) {
  const tone = TONES[status] ?? 'gray'
  return <span className={`pill ${tone}`}>{label ?? status}</span>
}

export function FilterBar({ children }: { children: ReactNode }) {
  return <div className="row" style={{ marginBottom: 12 }}>{children}</div>
}

// Reference list footer: "1-20 / 86" range text, per-page select,
// first/prev/next/last chevron buttons.
export function ListFooter({ first, last, total, perPage, onPerPage, onFirst, onPrev, onNext, onLast }: {
  first: number
  last: number
  total: number
  perPage: number
  onPerPage: (n: number) => void
  onFirst: () => void
  onPrev: () => void
  onNext: () => void
  onLast: () => void
}) {
  return (
    <div className="row" style={{ justifyContent: 'flex-end', gap: 12, marginTop: 12 }}>
      <span className="muted">
        {total === 0 ? '0 / 0' : `${first}-${last} / ${total}`}
      </span>
      <label className="muted" style={{ fontSize: 13 }}>
        {'Per page '}
        <select value={perPage} onChange={(e) => onPerPage(Number(e.target.value))} style={{ padding: '4px 8px' }}>
          {[10, 20, 50, 100].map((n) => <option key={n} value={n}>{n}</option>)}
        </select>
      </label>
      <span className="row" style={{ gap: 4 }}>
        <button className="ghost" style={{ padding: '4px 10px' }} disabled={first <= 1} onClick={onFirst} aria-label="First page">&#171;</button>
        <button className="ghost" style={{ padding: '4px 10px' }} disabled={first <= 1} onClick={onPrev} aria-label="Previous page">&#8249;</button>
        <button className="ghost" style={{ padding: '4px 10px' }} disabled={last >= total} onClick={onNext} aria-label="Next page">&#8250;</button>
        <button className="ghost" style={{ padding: '4px 10px' }} disabled={last >= total} onClick={onLast} aria-label="Last page">&#187;</button>
      </span>
    </div>
  )
}
