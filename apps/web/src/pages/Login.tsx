import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { api } from '../api/client'
import { useAuth } from '../api/auth'

export function Login() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const auth = useAuth()
  const [hotelId, setHotelId] = useState('')
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function submit(e: React.FormEvent) {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      const user = await api.login(Number(hotelId), username, password)
      auth.signIn(user)
      navigate('/')
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div style={{ display: 'grid', placeItems: 'center', minHeight: '100vh' }}>
      <form onSubmit={submit} style={{ display: 'grid', gap: 12, width: 320 }}>
        <h1>{t('app.title')}</h1>
        <input placeholder={t('login.hotelId')} value={hotelId} onChange={(e) => setHotelId(e.target.value)} inputMode="numeric" required />
        <input placeholder={t('login.username')} value={username} onChange={(e) => setUsername(e.target.value)} required />
        <input placeholder={t('login.password')} type="password" value={password} onChange={(e) => setPassword(e.target.value)} required />
        {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}
        <button disabled={busy} type="submit">{busy ? t('common.loading') : t('login.signIn')}</button>
      </form>
    </div>
  )
}
