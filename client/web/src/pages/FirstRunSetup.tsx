import { useEffect, useState } from 'react'
import { desktop, isDesktop } from '../api/desktop'

interface Preferences {
  first_run: boolean
  mode: 'local' | 'cloud'
  api_base: string | null
  storage_dir: string | null
  theme: 'light' | 'dark'
}

export default function FirstRunSetup({ onDone }: { onDone: () => void }) {
  const [mode, setMode] = useState<'local' | 'cloud'>('local')
  const [apiBase, setApiBase] = useState('https://your-server.example/api/v1')
  const [storageDir, setStorageDir] = useState('')
  const [theme, setTheme] = useState<'light' | 'dark'>('light')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!isDesktop) { setLoading(false); return }
    desktop.getAppPreferences().then((p: Preferences) => {
      setMode(p.mode)
      if (p.api_base) setApiBase(p.api_base)
      if (p.storage_dir) setStorageDir(p.storage_dir)
      setTheme(p.theme)
    }).catch((e) => setError(String(e))).finally(() => setLoading(false))
  }, [])

  if (!isDesktop) return null
  if (loading) return <main className="page"><p>Preparing local encrypted storage…</p></main>

  const submit = async () => {
    setSaving(true)
    setError(null)
    try {
      await desktop.completeFirstRun(mode, mode === 'cloud' ? apiBase.trim() : null, storageDir.trim(), theme)
      document.documentElement.dataset.theme = theme
      onDone()
    } catch (e) {
      setError(String(e))
    } finally {
      setSaving(false)
    }
  }

  return (
    <main className="page" style={{ maxWidth: 760, margin: '40px auto' }}>
      <section className="card" style={{ padding: 28 }}>
        <h1>Welcome to HMS Desktop</h1>
        <p>Choose how this desktop will connect. The desktop app is a client; it does not install or run the HMS web server.</p>

        <fieldset style={{ border: 0, padding: 0, margin: '24px 0' }}>
          <legend style={{ fontWeight: 700, marginBottom: 12 }}>Choose your setup</legend>
          <label style={{ display: 'flex', gap: 12, padding: 14, border: '1px solid var(--border)', borderRadius: 8, marginBottom: 10 }}>
            <input type="radio" name="mode" checked={mode === 'local'} onChange={() => setMode('local')} />
            <span><strong>Local only</strong><br /><small>Use this device’s encrypted database. No server or internet connection is required to open the app. Cloud sharing/sync is disabled until you switch later.</small></span>
          </label>
          <label style={{ display: 'flex', gap: 12, padding: 14, border: '1px solid var(--border)', borderRadius: 8 }}>
            <input type="radio" name="mode" checked={mode === 'cloud'} onChange={() => setMode('cloud')} />
            <span><strong>Connect to an online server</strong><br /><small>Use your company’s HMS server and keep a local encrypted mirror for offline operation.</small></span>
          </label>
        </fieldset>

        {mode === 'cloud' && <label style={{ display: 'grid', gap: 6, marginBottom: 18 }}>
          <strong>HMS API address</strong>
          <input value={apiBase} onChange={(e) => setApiBase(e.target.value)} placeholder="https://hms.example.com/api/v1" />
          <small>Example: https://hms.example.com/api/v1. Use your administrator’s API URL; do not enter a password or secret key here.</small>
        </label>}

        <p style={{ marginBottom: 18 }}><strong>Local encrypted storage:</strong> {storageDir || 'Default Windows user data folder (%APPDATA%\\com.hms.desktop)'}</p>

        <label style={{ display: 'grid', gap: 6, marginBottom: 20 }}>
          <strong>Appearance</strong>
          <select value={theme} onChange={(e) => setTheme(e.target.value as 'light' | 'dark')}>
            <option value="light">Light</option><option value="dark">Dark</option>
          </select>
        </label>

        <p style={{ background: '#fff7ed', padding: 12, borderRadius: 8 }}>
          <strong>Important:</strong> Local mode initializes the encrypted client database only. Hotel setup (rooms, rates, users) and reservation/payment workflows currently require the HMS API; the full standalone hotel wizard is not yet implemented.
        </p>
        {error && <p role="alert" style={{ color: 'var(--danger, #b91c1c)' }}>{error}</p>}
        <button className="primary" disabled={saving || (mode === 'cloud' && !apiBase.trim())} onClick={() => void submit()}>
          {saving ? 'Setting up…' : 'Continue'}
        </button>
      </section>
    </main>
  )
}
