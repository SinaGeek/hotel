// Update pill (todo D3/D4). Shows up only inside the desktop shell when the
// background checker finds a newer release. Downloads in the background and
// installs only on an explicit click, so an unsaved form is never yanked away.
// When the server refuses this build's sync protocol (426) the pill turns
// red and becomes blocking: syncing resumes only after the install.
import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { desktop, isDesktop, type UpdateInfo } from '../api/desktop'

const POLL_MS = 30 * 60 * 1000

export function UpdatePill() {
  const { t } = useTranslation()
  const [info, setInfo] = useState<UpdateInfo | null>(null)
  const [phase, setPhase] = useState<'idle' | 'installing' | 'done' | 'failed'>('idle')
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    if (!isDesktop) return
    try {
      setInfo(await desktop.updateCheck())
    } catch {
      /* an unreachable update host is not an app error */
    }
  }, [])

  useEffect(() => {
    if (!isDesktop) return
    void refresh()
    const iv = setInterval(() => { void refresh() }, POLL_MS)
    return () => clearInterval(iv)
  }, [refresh])

  if (!isDesktop || !info) return null
  const blocking = info.mandatory
  if (!info.available && !blocking) return null

  async function install() {
    setPhase('installing')
    setError(null)
    try {
      await desktop.updateInstall()
      setPhase('done')
    } catch (e) {
      setPhase('failed')
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  const color = blocking ? '#ef4444' : '#2563eb'
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        padding: '4px 10px',
        borderRadius: 16,
        fontSize: 13,
        fontWeight: 600,
        color: '#fff',
        background: color,
      }}
      title={error ?? info.notes ?? ''}
    >
      {blocking ? '⚠️ ' : ''}
      {phase === 'installing'
        ? t('update.installing')
        : phase === 'done'
          ? t('update.installed')
          : blocking
            ? t('update.mandatory', { version: info.version ?? '' })
            : t('update.available', { version: info.version ?? '' })}
      {phase !== 'installing' && phase !== 'done' && (
        <button
          type="button"
          onClick={() => void install()}
          style={{
            border: '1px solid rgba(255,255,255,.6)',
            background: 'transparent',
            color: '#fff',
            borderRadius: 10,
            padding: '2px 8px',
            fontSize: 12,
            cursor: 'pointer',
          }}
        >
          {t('update.install')}
        </button>
      )}
    </div>
  )
}
