// Sync status pill for the desktop shell (README §11.8 / todo 182): shows the
// background worker's latest state from Rust — green when synced (with
// last-synced time, pending ops and unresolved conflicts), red with the
// failure reason while offline/failing, and a decisive "locked" state that
// signs the user out when a sync handed back AccountLocked (deactivated
// user; the offline verifier was already wiped by the worker). Renders
// NOTHING in a plain browser — REST sampling keeps working.
import { useCallback, useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { desktop, isDesktop, type SyncSnapshot } from '../api/desktop'
import { clearSession } from '../api/client'

const REFRESH_MS = 10_000

function timeAgo(ms: number | undefined, t: (k: string, o?: Record<string, unknown>) => string): string | null {
  if (ms == null) return null
  const secs = Math.max(0, Math.round((Date.now() - ms) / 1000))
  if (secs < 60) return t('sync.justNow')
  if (secs < 3600) return t('sync.minutesAgo', { n: Math.floor(secs / 60) })
  if (secs < 86400) return t('sync.hoursAgo', { n: Math.floor(secs / 3600) })
  return t('sync.daysAgo', { n: Math.floor(secs / 86400) })
}

export function SyncStatusPill() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const [status, setStatus] = useState<SyncSnapshot | null>(null)
  const [syncing, setSyncing] = useState(false)

  const tick = useCallback(async () => {
    try {
      const snapshot = await desktop.syncStatus()
      const offline = await desktop.getOfflineMode()
      setStatus(offline
        ? { state: 'Failing', detail: { reason: 'Offline mode is enabled on this device', pending: snapshot.detail?.pending ?? 0 } }
        : snapshot)
    } catch {
      setStatus({ state: 'Unknown' })
    }
  }, [])

  /** F6: manual "Sync now" — fires one cycle, then refreshes the pill. */
  const syncNow = useCallback(async () => {
    setSyncing(true)
    try {
      await desktop.syncNow()
    } catch {
      /* the pill reports the outcome of the cycle itself */
    } finally {
      await tick()
      setSyncing(false)
    }
  }, [tick])

  useEffect(() => {
    if (!isDesktop) return
    let alive = true
    const run = () => { if (alive) void tick() }
    run()
    const iv = setInterval(run, REFRESH_MS)
    return () => { alive = false; clearInterval(iv) }
  }, [tick])

  // Locked = the sync cycle brought back an inactive/deactivated account
  // (README §11.7): the offline verifier is already wiped; drop the REST
  // session too and force an online re-login.
  useEffect(() => {
    if (status?.state === 'Locked') {
      clearSession()
      navigate('/login', { replace: true })
    }
  }, [status?.state, navigate])

  if (!isDesktop || !status || status.state === 'Locked') return null

  let dot = 'var(--ok, #22c55e)'
  let text: string
  let title: string
  switch (status.state) {
    case 'Synced': {
      const d = status.detail as { synced_at_ms?: number; pending: number; conflicts: number }
      const when = timeAgo((d as { synced_at_ms?: number }).synced_at_ms, t)
      text = t('sync.synced', { when: when ?? '' })
      title = d.pending > 0
        ? t('sync.syncedPending', { pending: d.pending })
        : t('sync.allSynced')
      if (d.pending > 0) dot = '#f59e0b'
      if (d.conflicts > 0) { dot = '#f59e0b'; text = t('sync.conflicts', { n: d.conflicts }) }
      break
    }
    case 'Failing': {
      const d = status.detail as { reason?: string; pending: number }
      dot = '#ef4444'
      text = t('sync.offline', { pending: d.pending })
      title = d.reason ? t('sync.offlineReason', { reason: d.reason }) : t('sync.offlineTitle')
      break
    }
    case 'UpdateRequired': {
      // D4: the server refused this build (426). Syncing is blocked until the
      // install completes; the UpdatePill carries the action button.
      const d = status.detail as { reason?: string }
      dot = '#ef4444'
      text = t('update.blockedTitle')
      title = t('update.blockedMsg', { reason: d.reason ?? '426' })
      break
    }
    default:
      text = t('sync.unknown')
      title = t('sync.unknownTitle')
  }

  return (
    <div className="sync-pill" title={title} data-state={status.state.toLowerCase()}>
      <span className="sync-dot" style={{ background: dot }} />
      <span className="sync-text">{text}</span>
      <button
        type="button"
        className="ghost"
        onClick={() => { void syncNow() }}
        disabled={syncing}
        title={t('sync.syncNowHint')}
        style={{
          marginLeft: 6,
          fontSize: 11,
          padding: '1px 7px',
          borderRadius: 8,
          border: '1px solid currentColor',
          background: 'transparent',
          color: 'inherit',
          cursor: syncing ? 'default' : 'pointer',
          opacity: syncing ? 0.6 : 1,
        }}
      >
        {syncing ? t('sync.syncing') : t('sync.syncNow')}
      </button>
    </div>
  )
}
