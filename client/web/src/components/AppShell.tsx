// App shell: dark navy sidebar with expanders, white header (hotel name +
// code, bell / language / theme / user), round violet FAB. Reference frame
// per docs/ui-reference.md. Role-aware nav (README §4/§6).
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { NavLink, Outlet, useLocation, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useAuth } from '../api/auth'
import { api } from '../api/client'
import { desktop, isDesktop } from '../api/desktop'
import { LANG_LABEL, currentLang, setLang, type Lang } from '../i18n'
import { SyncStatusPill } from './SyncStatus'
import { UpdatePill } from './UpdatePill'
import {
  IconBed, IconBell, IconCalendar, IconChart, IconChevronRight, IconDashboard,
  IconGlobe, IconHotel, IconList, IconLogout, IconMoon, IconSun,
  IconUsers, IconGear,
} from './icons'

interface Item { key: string; label: string; to: string; show: boolean }
interface Group { key: string; label: string; icon: ReactNode; items: Item[]; show: boolean }
interface Single { key: string; label: string; to: string; icon: ReactNode; show: boolean; end?: boolean }

interface PendingTransfer {
  id: string
  fromUser: string
  toUser: string
  hotelId: number
  amount: string
  currency: string
  notes?: string
  timestamp: number
}

const THEME_KEY = 'hms.theme'
const EXPAND_FOR: Record<string, string> = {
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
  const roles = (session?.user.role ?? '').split(',').map((r) => r.trim().toLowerCase())
  const isAdmin = roles.includes('admin') || session?.user.hotel_id === 0
  const isManager = roles.includes('manager')
  const isReception = roles.includes('reception')
  const isHk = roles.includes('housekeeping')
  const isAccountant = roles.includes('accountant')

  // Multi-role access permissions per confirmed matrix:
  const canSeeDashboard = isAdmin || isManager || isReception
  const canSeeBoard = isAdmin || isManager || isReception
  const canSeeReservations = isAdmin || isManager || isReception || isAccountant
  const canSeeFacility = isAdmin || isManager || isReception || isHk
  const canSeeRooms = isAdmin || isManager || isReception || isHk
  const canSeeHousekeeping = isAdmin || isManager || isReception
  const canSeeGuests = isAdmin || isManager || isReception
  const canSeeReports = isAdmin || isManager || isReception || isAccountant
  const canSeeStaff = isAdmin || isManager
  const canSeeChannels = isAdmin || isManager
  const canSeeAdminPanel = isAdmin
  const canSeeBulkImport = isAdmin || isManager

  const [hotelName, setHotelName] = useState<string | null>(null)
  const [conflictCount, setConflictCount] = useState(0)
  const [newConflictAlert, setNewConflictAlert] = useState<number | null>(null)
  const activeHotel = localStorage.getItem('hms.active_hotel')
  const effectiveHotelId = (session?.user.role === 'admin' && activeHotel) ? Number(activeHotel) : session?.user.hotel_id
  const hotelId = effectiveHotelId
  const hotelCode = effectiveHotelId

  // 1- Hamburger collapsible sidebar state
  const [collapsed, setCollapsed] = useState(() => localStorage.getItem('hms.sidebar_collapsed') === 'true')
  const [navOpen, setNavOpen] = useState(false)
  const closeNav = () => setNavOpen(false)

  // 2- User avatar dropdown & modals
  const [userMenuOpen, setUserMenuOpen] = useState(false)
  const userMenuRef = useRef<HTMLDivElement>(null)

  const [showProfileModal, setShowProfileModal] = useState(false)
  const [profileOldPassword, setProfileOldPassword] = useState('')
  const [profileNewPassword, setProfileNewPassword] = useState('')
  const [profileMsg, setProfileMsg] = useState<{ type: 'success' | 'error'; text: string } | null>(null)
  const [profileBusy, setProfileBusy] = useState(false)

  const [showShiftChangeModal, setShowShiftChangeModal] = useState(false)
  const [cashInSafe, setCashInSafe] = useState('2500.00')
  const [shiftTargetUser, setShiftTargetUser] = useState('')
  const [shiftNotes, setShiftNotes] = useState('')
  const [availableStaff, setAvailableStaff] = useState<Array<{ username: string; full_name?: string }>>([])
  const [shiftError, setShiftError] = useState<string | null>(null)

  // Blocking Shift Transfer Approval
  const [pendingTransfer, setPendingTransfer] = useState<PendingTransfer | null>(null)
  const [shiftDecisionBusy, setShiftDecisionBusy] = useState(false)
  const [shiftDecisionError, setShiftDecisionError] = useState<string | null>(null)

  useEffect(() => {
    if (!isDesktop || !session) return
    desktop.pendingShiftTransfer(session.user.hotel_id, session.user.username)
      .then((pending) => {
        if (!pending) return
        setPendingTransfer({
          id: pending.id,
          fromUser: pending.from_user,
          toUser: pending.to_user,
          hotelId: session.user.hotel_id,
          amount: (pending.amount_cents / 100).toFixed(2),
          currency: pending.currency,
          notes: pending.notes,
          timestamp: pending.created_ms,
        })
      })
      .catch(() => {})
  }, [session?.user.username, session?.user.hotel_id])

  // Load available staff for shift handover
  useEffect(() => {
    if (isDesktop) {
      desktop.listLocalUsers().then((us) => {
        const others = us
          .filter((u) => u.username.toLowerCase() !== (session?.user.username || '').toLowerCase())
          .map((u) => ({ username: u.username, full_name: u.full_name || u.username }))
        setAvailableStaff(others)
        if (others.length > 0 && !shiftTargetUser) {
          setShiftTargetUser(others[0].username)
        }
      }).catch(() => {})
    } else {
      api.listUsers().then((us) => {
        const others = us
          .filter((u) => u.username.toLowerCase() !== (session?.user.username || '').toLowerCase())
          .map((u) => ({ username: u.username, full_name: u.full_name || u.username }))
        setAvailableStaff(others)
        if (others.length > 0 && !shiftTargetUser) {
          setShiftTargetUser(others[0].username)
        }
      }).catch(() => {})
    }
  }, [session?.user.username, shiftTargetUser])

  // Click outside to close user menu
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (userMenuRef.current && !userMenuRef.current.contains(event.target as Node)) {
        setUserMenuOpen(false)
      }
    }
    document.addEventListener('mousedown', handleClickOutside)
    return () => document.removeEventListener('mousedown', handleClickOutside)
  }, [])

  useEffect(() => {
    if (hotelId === undefined) return
    let alive = true
    api.listHotels()
      .then((hs) => {
        if (!alive) return
        const hotel = hs.find((h) => h.id === hotelId)
        setHotelName(hotel?.name ?? null)
        if (isDesktop && hotel?.default_currency) {
          void desktop.setReportCurrency(hotel.default_currency).catch(() => {})
        }
      })
      .catch(() => {})
    return () => { alive = false }
  }, [hotelId])

  // Conflict polling
  useEffect(() => {
    if (!session || isHk) return
    let alive = true
    let prev = 0
    let initialized = false

    const checkConflicts = async () => {
      try {
        let count = 0
        if (isDesktop) {
          const s = await desktop.syncStatus()
          if (s.state === 'Synced') {
            count = (s.detail as { conflicts: number }).conflicts ?? 0
          } else {
            count = await api.reservationsCount({ status: 'conflict' })
          }
        } else {
          count = await api.reservationsCount({ status: 'conflict' })
        }
        if (!alive) return
        setConflictCount(count)
        if (initialized && count > prev) {
          setNewConflictAlert(count)
        }
        prev = count
        initialized = true
      } catch {
        /* best effort */
      }
    }

    void checkConflicts()
    const iv = setInterval(checkConflicts, 10_000)
    return () => {
      alive = false
      clearInterval(iv)
    }
  }, [session, isHk])

  // theme toggle
  const [theme, setTheme] = useState<'light' | 'dark'>(() =>
    localStorage.getItem(THEME_KEY) === 'dark' ? 'dark' : 'light')
  const [lang, setLangState] = useState<Lang>(() => currentLang())
  const switchLang = (next: Lang) => {
    setLangState(next)
    void setLang(next)
  }
  useEffect(() => {
    document.documentElement.dataset.theme = theme
    localStorage.setItem(THEME_KEY, theme)
  }, [theme])

  const groups: Group[] = [
    {
      key: 'bookings',
      label: 'nav.bookings',
      icon: <IconList />,
      show: canSeeReservations,
      items: [{ key: 'list', label: 'nav.list', to: '/reservations', show: canSeeReservations }],
    },
    {
      key: 'facility',
      label: 'nav.facility',
      icon: <IconBed />,
      show: canSeeFacility,
      items: isHk && !canSeeHousekeeping
        ? [{ key: 'update', label: 'nav.updateStatus', to: '/rooms', show: true }]
        : [
            { key: 'rooms', label: 'nav.rooms', to: '/rooms', show: canSeeRooms },
            { key: 'housekeeping', label: 'nav.housekeeping', to: '/housekeeping', show: canSeeHousekeeping },
          ],
    },
    {
      key: 'tools',
      label: 'nav.tools',
      icon: <IconGear />,
      show: canSeeStaff || canSeeChannels || isDesktop,
      items: [
        { key: 'staff', label: 'nav.staff', to: '/users', show: canSeeStaff },
        { key: 'channels', label: 'nav.channels', to: '/channels', show: canSeeChannels },
        { key: 'settings', label: 'Settings', to: '/settings', show: isDesktop },
      ],
    },
  ]
  const singles: Single[] = [
    { key: 'dashboard', label: 'nav.dashboard', to: '/', icon: <IconDashboard />, show: canSeeDashboard, end: true },
    { key: 'admin', label: 'Admin Panel', to: '/admin', icon: <IconGear />, show: canSeeAdminPanel, end: false },
    { key: 'import', label: 'Bulk Import', to: '/import', icon: <IconGear />, show: canSeeBulkImport, end: false },
    { key: 'board', label: 'nav.board', to: '/board', icon: <IconCalendar />, show: canSeeBoard },
    { key: 'guests', label: 'nav.guests', to: '/guests', icon: <IconUsers />, show: canSeeGuests },
    { key: 'reports', label: 'nav.reports', to: '/reports', icon: <IconChart />, show: canSeeReports },
  ]

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

  const toggleSidebar = () => {
    if (window.innerWidth <= 768) {
      setNavOpen((v) => !v)
    } else {
      setCollapsed((c) => {
        const next = !c
        localStorage.setItem('hms.sidebar_collapsed', String(next))
        return next
      })
    }
  }

  // Shift handover actions
  const handleShiftTransfer = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!shiftTargetUser) {
      setShiftError('Please choose the next shift person')
      return
    }
    const amountCents = Math.round(Number(cashInSafe) * 100)
    if (!Number.isFinite(amountCents) || amountCents < 0) {
      setShiftError('Enter a valid cash amount')
      return
    }
    const id = crypto.randomUUID()
    try {
      if (!isDesktop) throw new Error('Shift handover is available in the standalone desktop app only')
      await desktop.createShiftTransfer({
        id,
        hotelId: session?.user.hotel_id || 0,
        fromUser: session?.user.username || 'staff',
        toUser: shiftTargetUser,
        amountCents,
        currency: 'TRY',
        notes: shiftNotes.trim() || undefined,
      })
      setShowShiftChangeModal(false)
      auth.signOut()
      navigate('/login')
    } catch (e) {
      setShiftError(e instanceof Error ? e.message : String(e))
    }
  }

  const decideTransfer = async (p: PendingTransfer, approved: boolean) => {
    setShiftDecisionBusy(true)
    setShiftDecisionError(null)
    try {
      if (!isDesktop) throw new Error('Shift handover is available in the standalone desktop app only')
      await desktop.decideShiftTransfer(p.id, session?.user.username || '', approved)
      setPendingTransfer(null)
      if (!approved) {
        auth.signOut()
        navigate('/login')
      }
    } catch (e) {
      setShiftDecisionError(e instanceof Error ? e.message : String(e))
    } finally {
      setShiftDecisionBusy(false)
    }
  }

  // Profile password change
  const handlePasswordChange = async (e: React.FormEvent) => {
    e.preventDefault()
    if (profileNewPassword.length < 3) {
      setProfileMsg({ type: 'error', text: 'Password must be at least 3 characters' })
      return
    }
    setProfileBusy(true)
    setProfileMsg(null)
    try {
      if (isDesktop) {
        const all = await desktop.listLocalUsers()
        const target = all.find((u) => u.username === session?.user.username && u.hotel_id === session?.user.hotel_id)
        if (!target) throw new Error('Local profile not found on this device')
        await desktop.updateLocalUser(target.user_id, { password: profileNewPassword })
      } else if (session?.user.user_id) {
        await api.updateUser(session.user.user_id, { password: profileNewPassword })
      }
      setProfileMsg({ type: 'success', text: 'Password updated successfully' })
      setProfileOldPassword('')
      setProfileNewPassword('')
    } catch (err) {
      setProfileMsg({ type: 'error', text: err instanceof Error ? err.message : String(err) })
    } finally {
      setProfileBusy(false)
    }
  }

  return (
    <div className="app-shell">
      {/* 1- Hamburger collapsible left sidebar */}
      <nav className={`sidebar${collapsed ? ' collapsed' : ''}${navOpen ? ' open' : ''}`}>
        <div
          className="side-brand"
          style={{ cursor: 'pointer' }}
          onClick={toggleSidebar}
          title={collapsed ? 'Expand menu' : 'Collapse menu'}
        >
          <span className="logo"><IconHotel /></span>
          {!collapsed && <span>{t('app.title')}</span>}
        </div>
        <div className="side-nav">
          {singles.filter((s) => s.show).map((s) => (
            <NavLink
              key={s.key}
              to={s.to}
              end={s.end}
              onClick={closeNav}
              className={({ isActive }) => `side-item${isActive ? ' active' : ''}`}
              title={collapsed ? t(s.label) : undefined}
            >
              {s.icon}
              {!collapsed && <span>{t(s.label)}</span>}
            </NavLink>
          ))}
          {groups.filter((g) => g.show).map((g) => (
            <div key={g.key} className={g.items.some((i) => i.show) ? 'side-group' : ''}>
              <button
                type="button"
                className={`side-item${g.items.some((i) => i.to === location.pathname) ? ' active' : ''}`}
                onClick={() => {
                  if (collapsed) setCollapsed(false)
                  setOpen((o) => ({ ...o, [g.key]: !o[g.key] }))
                }}
                aria-expanded={!collapsed && !!open[g.key]}
                title={collapsed ? t(g.label) : undefined}
              >
                {g.icon}
                {!collapsed && <span>{t(g.label)}</span>}
                {!collapsed && <span className={`caret${open[g.key] ? ' open' : ''}`}><IconChevronRight /></span>}
              </button>
              {!collapsed && open[g.key] && (
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
        {/* Logout removed from sidebar per requirement 2 */}
      </nav>
      {navOpen && <div className="nav-back" onClick={closeNav} />}

      <div className="content">
        <header className="header">
          <div className="header-left">
            <button
              type="button"
              className="icon-btn nav-burger"
              aria-label={t('nav.menu')}
              aria-expanded={!collapsed}
              title={collapsed ? 'Expand menu' : 'Collapse menu'}
              onClick={toggleSidebar}
            ><IconList /></button>
            <div>
              {isAdmin ? (
                // Admin: editable hotel code switcher
                <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                  <form
                    onSubmit={(e) => {
                      e.preventDefault()
                      const form = e.currentTarget
                      const input = form.elements.namedItem('hotelCode') as HTMLInputElement
                      const val = input.value.trim()
                      if (val) {
                        localStorage.setItem('hms.active_hotel', val)
                        window.location.reload()
                      }
                    }}
                    style={{ display: 'flex', alignItems: 'center', gap: 6 }}
                  >
                    <input
                      name="hotelCode"
                      type="number"
                      defaultValue={localStorage.getItem('hms.active_hotel') || String(hotelCode ?? 0)}
                      style={{
                        width: 90,
                        padding: '4px 8px',
                        border: '1px solid #cbd5e1',
                        borderRadius: 6,
                        fontSize: 14,
                        fontWeight: 600,
                      }}
                      placeholder="Hotel ID"
                    />
                    <button
                      type="submit"
                      style={{
                        padding: '4px 8px',
                        fontSize: 12,
                        backgroundColor: '#3b82f6',
                        color: 'white',
                        border: 'none',
                        borderRadius: 4,
                        cursor: 'pointer',
                      }}
                    >
                      Go
                    </button>
                  </form>
                  <span style={{ fontSize: 14, color: '#64748b' }}>{hotelName || 'Platform Admin'}</span>
                </div>
              ) : (
                // Regular users: read-only hotel name and code
                <>
                  <div className="header-title">{hotelName ?? t('app.title')}</div>
                  <div className="header-sub">{t('shell.hotelCode', { code: hotelCode ?? '' })}</div>
                </>
              )}
            </div>
          </div>
          <div className="header-right">
            {conflictCount > 0 && (
              <button
                className="icon-btn"
                style={{
                  backgroundColor: 'rgba(245, 158, 11, 0.15)',
                  color: '#d97706',
                  fontWeight: 600,
                  borderRadius: 16,
                  padding: '4px 10px',
                  fontSize: 13,
                  width: 'auto',
                  display: 'flex',
                  alignItems: 'center',
                  gap: 4,
                }}
                onClick={() => navigate('/reservations?tab=conflict')}
                title={t('bk.conflictBanner', { n: conflictCount })}
              >
                {conflictCount} {t('bk.tabs.conflict')}
              </button>
            )}
            <SyncStatusPill />
            <UpdatePill />
            <button className="icon-btn" aria-label={t('shell.notifications')} title={t('shell.notifications')}><IconBell /></button>
            <button
              className="icon-btn"
              aria-label={t('shell.language')}
              title={`${t('shell.language')}: ${LANG_LABEL[lang]}`}
              onClick={() => switchLang(lang === 'en' ? 'tr' : 'en')}
            >
              <IconGlobe />
              <span style={{ fontSize: 10, marginLeft: 2, fontWeight: 700 }}>{lang.toUpperCase()}</span>
            </button>
            <button
              className="icon-btn"
              aria-label={theme === 'dark' ? t('shell.lightMode') : t('shell.darkMode')}
              title={theme === 'dark' ? t('shell.lightMode') : t('shell.darkMode')}
              onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
            >
              {theme === 'dark' ? <IconSun /> : <IconMoon />}
            </button>

            {/* 2- User avatar dropdown menu */}
            <div className="user-chip-wrapper" ref={userMenuRef}>
              <div
                className="user-chip"
                style={{ cursor: 'pointer', userSelect: 'none' }}
                onClick={() => setUserMenuOpen((v) => !v)}
              >
                <span className="avatar">{initials}</span>
                <div style={{ lineHeight: 1.2 }}>
                  <div style={{ fontWeight: 600 }}>{session?.user.username}</div>
                  <div className="muted" style={{ fontSize: 12 }}>
                    {roles.map((r) => t(`users.${r}`) || r).join(', ')}
                  </div>
                </div>
              </div>

              {userMenuOpen && (
                <div className="user-menu-dropdown">
                  <button
                    type="button"
                    className="user-menu-item"
                    onClick={() => {
                      setUserMenuOpen(false)
                      setShowProfileModal(true)
                    }}
                  >
                    <IconUsers />
                    <span>Profile</span>
                  </button>
                  <button
                    type="button"
                    className="user-menu-item"
                    onClick={() => {
                      setUserMenuOpen(false)
                      setShowShiftChangeModal(true)
                    }}
                  >
                    <IconCalendar />
                    <span>Shift Change</span>
                  </button>
                  <div style={{ height: 1, backgroundColor: 'var(--border)', margin: '4px 0' }} />
                  <button
                    type="button"
                    className="user-menu-item danger"
                    onClick={() => {
                      setUserMenuOpen(false)
                      auth.signOut()
                      navigate('/login')
                    }}
                  >
                    <IconLogout />
                    <span>Logout</span>
                  </button>
                </div>
              )}
            </div>
          </div>
        </header>
        <main>
          <Outlet />
        </main>
      </div>

      {/* 2-1 Profile Modal */}
      {showProfileModal && (
        <div
          style={{
            position: 'fixed',
            inset: 0,
            backgroundColor: 'rgba(0,0,0,0.5)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 9000,
          }}
        >
          <div className="card" style={{ width: 440, padding: 24, backgroundColor: 'var(--surface)', borderRadius: 12 }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
              <h3 style={{ margin: 0 }}>User Profile</h3>
              <button
                type="button"
                className="ghost"
                style={{ padding: '4px 8px', fontSize: 13 }}
                onClick={() => setShowProfileModal(false)}
              >
                Close
              </button>
            </div>
            <div style={{ display: 'grid', gap: 10, marginBottom: 20 }}>
              <div><strong>Username:</strong> {session?.user.username}</div>
              <div><strong>Full Name:</strong> {session?.user.full_name || session?.user.username}</div>
              <div><strong>Hotel ID:</strong> {session?.user.hotel_id}</div>
              <div><strong>Assigned Roles:</strong> {roles.join(', ')}</div>
            </div>

            <hr style={{ border: 0, borderTop: '1px solid var(--border)', margin: '16px 0' }} />

            <form onSubmit={handlePasswordChange} style={{ display: 'grid', gap: 12 }}>
              <h4 style={{ margin: 0 }}>Change Password</h4>
              {profileMsg && (
                <div style={{
                  padding: 8,
                  fontSize: 13,
                  borderRadius: 6,
                  backgroundColor: profileMsg.type === 'success' ? '#dcfce7' : '#fee2e2',
                  color: profileMsg.type === 'success' ? '#15803d' : '#b91c1c',
                }}>
                  {profileMsg.text}
                </div>
              )}
              <input
                type="password"
                placeholder="Current Password"
                value={profileOldPassword}
                onChange={(e) => setProfileOldPassword(e.target.value)}
                style={{ padding: 8 }}
              />
              <input
                type="password"
                placeholder="New Password (min 3 chars)"
                value={profileNewPassword}
                onChange={(e) => setProfileNewPassword(e.target.value)}
                required
                minLength={3}
                style={{ padding: 8 }}
              />
              <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 4 }}>
                <button
                  type="button"
                  className="ghost"
                  onClick={() => setShowProfileModal(false)}
                >
                  Cancel
                </button>
                <button type="submit" className="primary" disabled={profileBusy}>
                  {profileBusy ? 'Saving...' : 'Update Password'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* 2-2 Shift Change Modal */}
      {showShiftChangeModal && (
        <div
          style={{
            position: 'fixed',
            inset: 0,
            backgroundColor: 'rgba(0,0,0,0.5)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 9000,
          }}
        >
          <form
            onSubmit={handleShiftTransfer}
            className="card"
            style={{ width: 460, padding: 24, backgroundColor: 'var(--surface)', borderRadius: 12, display: 'grid', gap: 16 }}
          >
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <h3 style={{ margin: 0 }}>Shift Change & Cash Handover</h3>
              <button
                type="button"
                className="ghost"
                style={{ padding: '4px 8px', fontSize: 13 }}
                onClick={() => setShowShiftChangeModal(false)}
              >
                Close
              </button>
            </div>

            {shiftError && (
              <div style={{ padding: 8, fontSize: 13, backgroundColor: '#fee2e2', color: '#b91c1c', borderRadius: 6 }}>
                {shiftError}
              </div>
            )}

            <div>
              <label style={{ display: 'block', fontSize: 13, fontWeight: 600, marginBottom: 4 }}>
                Cash in Safe (Counted balance):
              </label>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <input
                  type="number"
                  step="0.01"
                  value={cashInSafe}
                  onChange={(e) => setCashInSafe(e.target.value)}
                  required
                  style={{ flex: 1, padding: 8, fontSize: 16, fontWeight: 700 }}
                />
                <span style={{ fontWeight: 700, color: 'var(--muted)' }}>TRY</span>
              </div>
              <small style={{ color: 'var(--muted)', fontSize: 11, marginTop: 4, display: 'block' }}>
                Verify the exact physical cash inside the safe drawer before handing over.
              </small>
            </div>

            <div>
              <label style={{ display: 'block', fontSize: 13, fontWeight: 600, marginBottom: 4 }}>
                Next Shift Person:
              </label>
              <select
                value={shiftTargetUser}
                onChange={(e) => setShiftTargetUser(e.target.value)}
                required
                style={{ width: '100%', padding: 8 }}
              >
                {availableStaff.map((u) => (
                  <option key={u.username} value={u.username}>
                    {u.username} ({u.full_name})
                  </option>
                ))}
              </select>
            </div>

            <div>
              <label style={{ display: 'block', fontSize: 13, fontWeight: 600, marginBottom: 4 }}>
                Shift Notes / Observations:
              </label>
              <textarea
                value={shiftNotes}
                onChange={(e) => setShiftNotes(e.target.value)}
                placeholder="Important guest notes, pending tasks, cash notes..."
                rows={3}
                style={{ width: '100%', padding: 8, resize: 'vertical' }}
              />
            </div>

            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 10, marginTop: 6 }}>
              <button
                type="button"
                className="ghost"
                onClick={() => setShowShiftChangeModal(false)}
              >
                Cancel
              </button>
              <button type="submit" className="primary" style={{ padding: '8px 18px' }}>
                Transfer & Log Out
              </button>
            </div>
          </form>
        </div>
      )}

      {/* 2-3 Blocking Shift Handover Approval Modal */}
      {pendingTransfer && (
        <div
          style={{
            position: 'fixed',
            inset: 0,
            backgroundColor: 'rgba(0,0,0,0.85)',
            zIndex: 9999,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            backdropFilter: 'blur(3px)',
          }}
        >
          <div
            className="card"
            style={{
              maxWidth: 480,
              width: '90%',
              padding: 28,
              backgroundColor: 'var(--surface)',
              borderRadius: 14,
              boxShadow: '0 20px 40px rgba(0,0,0,0.3)',
              border: '2px solid #3b82f6',
            }}
          >
            <h3 style={{ margin: '0 0 12px 0', color: '#2563eb', fontSize: 20 }}>
              Shift Handover Approval Required
            </h3>
            <p style={{ margin: '0 0 16px 0', lineHeight: 1.5, fontSize: 14 }}>
              <strong>{pendingTransfer.fromUser}</strong> has initiated a shift transfer to you with the counted Cash in Safe:
            </p>

            <div
              style={{
                padding: 16,
                backgroundColor: 'rgba(59, 130, 246, 0.1)',
                border: '1px solid rgba(59, 130, 246, 0.3)',
                borderRadius: 8,
                marginBottom: 16,
                textAlign: 'center',
              }}
            >
              <div style={{ fontSize: 28, fontWeight: 800, color: '#1d4ed8' }}>
                {pendingTransfer.amount} {pendingTransfer.currency}
              </div>
              {pendingTransfer.notes && (
                <div style={{ fontSize: 13, marginTop: 8, color: 'var(--muted)', textAlign: 'left', fontStyle: 'italic' }}>
                  "{pendingTransfer.notes}"
                </div>
              )}
            </div>

            <p style={{ fontSize: 13, color: 'var(--muted)', marginBottom: 24 }}>
              You must confirm whether you approve or reject this cash transfer to proceed into the system.
            </p>
            {shiftDecisionError && (
              <div role="alert" style={{ padding: 10, background: '#fee2e2', color: '#b91c1c', borderRadius: 6, marginBottom: 12 }}>
                {shiftDecisionError}
              </div>
            )}

            <div style={{ display: 'flex', gap: 12, justifyContent: 'flex-end' }}>
              <button
                type="button"
                style={{
                  padding: '10px 18px',
                  backgroundColor: '#fee2e2',
                  color: '#b91c1c',
                  border: 'none',
                  borderRadius: 6,
                  fontWeight: 600,
                  cursor: 'pointer',
                  fontSize: 14,
                }}
                disabled={shiftDecisionBusy}
                onClick={() => void decideTransfer(pendingTransfer, false)}
              >
                Reject Transfer
              </button>
              <button
                type="button"
                className="primary"
                style={{
                  padding: '10px 22px',
                  fontWeight: 600,
                  fontSize: 14,
                }}
                disabled={shiftDecisionBusy}
                onClick={() => void decideTransfer(pendingTransfer, true)}
              >
                Approve Cash Transfer
              </button>
            </div>
          </div>
        </div>
      )}

      {newConflictAlert != null && (
        <div className="modal-back" onClick={() => setNewConflictAlert(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 440 }}>
            <h3>Warning: {t('bk.conflictAlertTitle')}</h3>
            <p style={{ margin: '12px 0 20px', lineHeight: 1.5 }}>
              {t('bk.conflictAlertMsg', { n: newConflictAlert })}
            </p>
            <div className="row" style={{ justifyContent: 'flex-end', gap: 10 }}>
              <button
                type="button"
                className="ghost"
                onClick={() => setNewConflictAlert(null)}
              >
                {t('common.cancel')}
              </button>
              <button
                type="button"
                className="primary"
                onClick={() => {
                  setNewConflictAlert(null)
                  navigate('/reservations?tab=conflict')
                }}
              >
                {t('bk.viewConflicts')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
