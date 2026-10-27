import { Outlet, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useAuth } from '../api/auth'

export function AppShell() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const auth = useAuth()
  return (
    <div>
      <header style={{ display: 'flex', justifyContent: 'space-between', padding: 12 }}>
        <strong onClick={() => navigate('/')}>{t('app.title')}</strong>
        <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
          <select
            value={localStorage.getItem('lang') || 'tr'}
            onChange={(e) => { localStorage.setItem('lang', e.target.value); location.reload() }}
          >
            <option value="tr">TR</option>
            <option value="en">EN</option>
          </select>
          <span>{auth.user?.username}</span>
          <button onClick={() => { auth.signOut(); navigate('/login') }}>↩</button>
        </div>
      </header>
      <main style={{ padding: 16 }}>
        <Outlet />
      </main>
    </div>
  )
}
