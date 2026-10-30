// App shell: dark navy sidebar with expanders, white header (hotel name +
// code, bell / language / theme / user), round violet FAB. Reference frame
// per docs/ui-reference.md. Role-aware nav (README §4/§6).
import { useEffect, useState, type ReactNode } from 'react'
import { NavLink, Outlet, useLocation, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useAuth } from '../api/auth'
import { api, sessionHasPermission } from '../api/client'
import { desktop, isDesktop } from '../api/desktop'
import { SyncStatusPill } from './SyncStatus'
import {
  IconBed, IconBell, IconCalendar, IconChart, IconChevronRight, IconDashboard,
  IconGlobe, IconHotel, IconList, IconLogout, IconMoon, IconPlus, IconSun,
  IconUsers, IconGear,
} from './icons'

interface Item { key: string; label: string; to: string; show: boolean }
interface Group { key: string; label: string; icon: ReactNode; items: Item[]; show: boolean }
interface Single { key: string; label: string; to: string; icon: ReactNode; show: boolean; end?: boolean }

const THEME_KEY = 'hms.theme'
// Deferred reference-module groups (todo 5b.8, README §4 deferral) are kept
// OUT of the sidebar on purpose — Invoices / Transactions (Expense-Stock /
// Open Sale) / Periodic Prices / Agencies & Accounts and the Tools extras
// (Settings, Currency Exchange, Applications) have no backing data model
// (no invoice numbering, agency balances, stock, exchange ledgers, SMTP/SMS
// config). Do not re-add them until Phase 7 adds those tables.
const EXPAND_FOR: Record<string, string> = {
  // routes that belong to an expander group -> group key
  '/reservations': 'bookings',
  '/rooms': 'facility',
  '/housekeeping': 'facility',
  '/users': 'tools',
  '/channels': 'tools',
}

export function AppShell() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const location = useLocation()
  const auth = useAuth()
  const session = auth.session
  const role = session?.user.role ?? 'housekeeping'
  const can = (perm: string) => sessionHasPermission(session, perm)
  const isHk = role === 'housekeeping'
  const hotelCode = session?.user.hotel_id

  const [hotelName, setHotelName] = useState<string | null>(null)
  const hotelId = session?.user.hotel_id
  useEffect(() => {
    if (hotelId === undefined) return
    let alive = true
    api.listHotels()
      .then((hs) => {
        if (!alive) return
        const hotel = hs.find((h) => h.id === hotelId)
        setHotelName(hotel?.name ?? null)
        if (isDesktop && hotel?.default_currency) {
          void desktop.setReportCurrency(hotel.default_currency).catch(() => { /* report currency cache is best effort */ })
        }
      })
      .catch(() => { /* header falls back to the code only */ })
    return () => { alive = false }
  }, [hotelId])

  // theme toggle (light default, persisted)
  const [theme, setTheme] = useState<'light' | 'dark'>(() =>
    localStorage.getItem(THEME_KEY) === 'dark' ? 'dark' : 'light')
  useEffect(() => {
    document.documentElement.dataset.theme = theme
    localStorage.setItem(THEME_KEY, theme)
  }, [theme])

  const showReservations = can('reservation_view') && !isHk

  const groups: Group[] = [
    {
      key: 'bookings',
      label: 'nav.bookings',
      icon: <IconList />,
      show: showReservations,
      items: [{ key: 'list', label: 'nav.list', to: '/reservations', show: showReservations }],
    },
    {
      key: 'facility',
      label: 'nav.facility',
      icon: <IconBed />,
      show: true,
      items: isHk
        ? [{ key: 'update', label: 'nav.updateStatus', to: '/rooms', show: true }]
        : [
            { key: 'rooms', label: 'nav.rooms', to: '/rooms', show: true },
            { key: 'housekeeping', label: 'nav.housekeeping', to: '/housekeeping', show: true },
          ],
    },
    {
      key: 'tools',
      label: 'nav.tools',
      icon: <IconGear />,
      show: can('user_view'),
      items: [
        { key: 'staff', label: 'nav.staff', to: '/users', show: can('user_view') },
        { key: 'channels', label: 'nav.channels', to: '/channels', show: !isHk },
      ],
    },
  ]
  const singles: Single[] = [
    { key: 'dashboard', label: 'nav.dashboard', to: '/', icon: <IconDashboard />, show: true, end: true },
    { key: 'board', label: 'nav.board', to: '/board', icon: <IconCalendar />, show: true },
    { key: 'guests', label: 'nav.guests', to: '/guests', icon: <IconUsers />, show: showReservations },
    { key: 'reports', label: 'nav.reports', to: '/reports', icon: <IconChart />, show: can('report_view_tenant') },
  ]

  // auto-open the expander that contains the active route
  const [open, setOpen] = useState<Record<string, boolean>>(() => {
    const initial: Record<string, boolean> = {}
    for (const g of groups) initial[g.key] = g.items.some((i) => i.to === location.pathname)
    return initial
  })
  useEffect(() => {
    const groupKey = EXPAND_FOR[location.pathname]
    if (groupKey) setOpen((o) => ({ ...o, [groupKey]: true }))
  }, [location.pathname])

  const initials = (session?.user.username ?? '?').slice(0, 2).toUpperCase()

  // Mobile nav (README §4 responsive): the sidebar becomes off-canvas behind a
  // hamburger at <=768px; every navigation (and backdrop tap) closes it.
  const [navOpen, setNavOpen] = useState(false)
  const closeNav = () => setNavOpen(false)

  return (
    <div className="app-shell">
      <nav className={`sidebar${navOpen ? ' open' : ''}`}>
        <div className="side-brand">
          <span className="logo"><IconHotel /></span>
          <span>{t('app.title')}</span>
        </div>
        <div className="side-nav">
          {singles.filter((s) => s.show).map((s) => (
            <NavLink key={s.key} to={s.to} end={s.end} onClick={closeNav} className={({ isActive }) => `side-item${isActive ? ' active' : ''}`}>
              {s.icon}
              <span>{t(s.label)}</span>
            </NavLink>
          ))}
          {groups.filter((g) => g.show).map((g) => (
            <div key={g.key} className={g.items.some((i) => i.show) ? 'side-group' : ''}>
              <button
                type="button"
                className={`side-item${g.items.some((i) => i.to === location.pathname) ? ' active' : ''}`}
                onClick={() => setOpen((o) => ({ ...o, [g.key]: !o[g.key] }))}
                aria-expanded={!!open[g.key]}
              >
                {g.icon}
                <span>{t(g.label)}</span>
                <span className={`caret${open[g.key] ? ' open' : ''}`}><IconChevronRight /></span>
              </button>
              {open[g.key] && (
                <div className="side-sub">
                  {g.items.filter((i) => i.show).map((i) => (
                    <NavLink key={i.key} to={i.to} onClick={closeNav} className={({ isActive }) => `side-item${isActive ? ' active' : ''}`}>
                      <span>{t(i.label)}</span>
                    </NavLink>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
        <div className="side-bottom">
          <button type="button" className="side-item" onClick={() => { closeNav(); auth.signOut(); navigate('/login') }}>
            <IconLogout />
            <span>{t('nav.logout')}</span>
          </button>
        </div>
      </nav>
      {navOpen && <div className="nav-back" onClick={closeNav} />}

      <div className="content">
        <header className="header">
          <div className="header-left">
            <button
              type="button"
              className="icon-btn nav-burger"
              aria-label={t('nav.menu')}
              aria-expanded={navOpen}
              title={t('nav.menu')}
              onClick={() => setNavOpen((v) => !v)}
            ><IconList /></button>
            <div>
              <div className="header-title">{hotelName ?? t('app.title')}</div>
              <div className="header-sub">{t('shell.hotelCode', { code: hotelCode ?? '' })}</div>
            </div>
          </div>
          <div className="header-right">
            <SyncStatusPill />
            <button className="icon-btn" aria-label={t('shell.notifications')} title={t('shell.notifications')}><IconBell /></button>
            <button className="icon-btn" aria-label={t('shell.language')} title={t('shell.language')}><IconGlobe /></button>
            <button
              className="icon-btn"
              aria-label={theme === 'dark' ? t('shell.lightMode') : t('shell.darkMode')}
              title={theme === 'dark' ? t('shell.lightMode') : t('shell.darkMode')}
              onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
            >
              {theme === 'dark' ? <IconSun /> : <IconMoon />}
            </button>
            <div className="user-chip">
              <span className="avatar">{initials}</span>
              <div style={{ lineHeight: 1.2 }}>
                <div style={{ fontWeight: 600 }}>{session?.user.username}</div>
                <div className="muted" style={{ fontSize: 12 }}>{t(`users.${role}`)}</div>
              </div>
            </div>
          </div>
        </header>
        <main>
          <Outlet />
        </main>
      </div>

      {/* "new record" shortcut; Monthly Room supports create from the board */}
      <button className="fab" onClick={() => navigate('/board')} aria-label={t('shell.newRecord')} title={t('shell.newRecord')}>
        <IconPlus />
      </button>
    </div>
  )
}
