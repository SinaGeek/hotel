import { NavLink, Outlet, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useAuth } from '../api/auth'
import { sessionHasPermission } from '../api/client'


// Role-aware nav (README §4/§6): housekeeping only sees rooms/housekeeping.
export function AppShell() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const auth = useAuth()
  const session = auth.session
  const role = session?.user.role ?? 'housekeeping'
  const can = (perm: string) => sessionHasPermission(session, perm)
  const isHk = role === 'housekeeping'

  const links: Array<{ to: string; label: string; show: boolean }> = [
    { to: '/', label: t('nav.dashboard'), show: true },
    { to: '/board', label: t('nav.board'), show: true },
    { to: '/reservations', label: t('nav.reservations'), show: can('reservation_view') && !isHk },
    { to: '/guests', label: t('nav.guests'), show: can('reservation_view') && !isHk },
    { to: '/rooms', label: isHk ? t('nav.housekeeping') : t('nav.rooms'), show: true },
    { to: '/reports', label: t('reports.title'), show: sessionHasPermission(session, 'report_view_tenant') },
    { to: '/users', label: t('nav.users'), show: can('user_view') },
  ]

  return (
    <div>
      <header style={{ display: 'flex', justifyContent: 'space-between', padding: '10px 18px', background: '#fff', borderBottom: '1px solid #e3e8f1' }}>
        <strong style={{ cursor: 'pointer' }} onClick={() => navigate('/')}>{t('app.title')}</strong>
        <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
          <span className="muted">{session?.user.username} · {t(`users.${role}`)}</span>
          <button onClick={() => { auth.signOut(); navigate('/login') }}>{t('nav.logout')}</button>
        </div>
      </header>
      <div style={{ display: 'flex' }}>
        <nav style={{ width: 190, padding: '14px 10px', display: 'flex', flexDirection: 'column', gap: 4 }}>
          {links.filter((l) => l.show).map((l) => (
            <NavLink
              key={l.to}
              to={l.to}
              end={l.to === '/'}
              style={({ isActive }) => ({
                padding: '8px 12px',
                borderRadius: 8,
                textDecoration: 'none',
                color: isActive ? '#fff' : '#333',
                background: isActive ? '#2e5bff' : 'transparent',
              })}
            >
              {l.label}
            </NavLink>
          ))}
        </nav>
        <main style={{ flexBasis: 'calc(100% - 190px)', minWidth: 0 }}>
          <Outlet />
        </main>
      </div>
    </div>
  )
}
