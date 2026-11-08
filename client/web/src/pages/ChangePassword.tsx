// Forced password change after first login (README §8): the server rejects
// every other endpoint while the fpc flag is set, so this page re-logins
// afterwards to obtain a clean token.
import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { api } from '../api/client'
import { useAuth } from '../api/auth'

export function ChangePassword() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const auth = useAuth()
  const [oldPassword, setOld] = useState('')
  const [newPassword, setNew] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function submit(e: React.FormEvent) {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      await api.changePassword(oldPassword, newPassword)
      // re-login to clear the fpc flag (refresh keeps it, per README §8)
      const session = auth.session
      if (session) {
        const res = await api.login(session.user.hotel_id, session.user.username, newPassword)
        auth.signIn({ token: res.token, user: res.user, permissions: res.permissions })
      } else {
        auth.signOut()
      }
      navigate('/')
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div style={{ display: 'grid', placeItems: 'center', minHeight: '100vh' }}>
      <form onSubmit={submit} style={{ display: 'grid', gap: 12, width: 320 }} className="card">
        <h2 style={{ margin: 0 }}>{t('changePassword.title')}</h2>
        <p className="muted" style={{ margin: 0 }}>{t('login.mustChangePassword')}</p>
        <input placeholder={t('changePassword.old')} type="password" value={oldPassword} onChange={(e) => setOld(e.target.value)} required />
        <input placeholder={t('changePassword.new')} type="password" value={newPassword} minLength={8} onChange={(e) => setNew(e.target.value)} required />
        {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}
        <div style={{ display: 'flex', gap: 8 }}>
          <button
            type="button"
            className="ghost"
            style={{ flex: 1 }}
            disabled={busy}
            onClick={() => navigate(-1)}
          >
            {t('common.back')}
          </button>
          <button className="primary" style={{ flex: 2 }} disabled={busy} type="submit">
            {busy ? t('common.loading') : t('changePassword.submit')}
          </button>
        </div>
      </form>
    </div>
  )
}
