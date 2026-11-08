import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { api } from '../api/client'
import { useAuth } from '../api/auth'
import { desktop, isDesktop } from '../api/desktop'

export function getDefaultPermissions(roleStr: string): string[] {
  const roles = roleStr.split(',').map((r) => r.trim().toLowerCase())
  const perms = new Set<string>()

  for (const role of roles) {
    if (role === 'admin') {
      return [
        'hotel_create', 'hotel_edit', 'hotel_disable', 'hotel_view_all',
        'user_manage', 'user_view', 'room_type_manage', 'room_manage',
        'price_rule_manage', 'reservation_view', 'reservation_create',
        'reservation_edit', 'reservation_check_in', 'reservation_check_out',
        'reservation_cancel', 'financial_view', 'payment_record',
        'payment_refund', 'folio_manage', 'housekeeping_view',
        'housekeeping_update_status', 'report_view_tenant',
        'report_view_platform', 'audit_log_view', 'guest_export',
        'guest_delete', 'invoice_manage', 'stock_manage',
        'expense_manage', 'agency_manage',
      ]
    }
    if (role === 'manager') {
      ;[
        'user_manage', 'user_view', 'room_type_manage', 'room_manage',
        'price_rule_manage', 'reservation_view', 'reservation_create',
        'reservation_edit', 'reservation_check_in', 'reservation_check_out',
        'reservation_cancel', 'financial_view', 'payment_record',
        'payment_refund', 'folio_manage', 'housekeeping_view',
        'housekeeping_update_status', 'report_view_tenant',
        'audit_log_view', 'guest_export', 'guest_delete',
        'invoice_manage', 'stock_manage', 'expense_manage', 'agency_manage',
      ].forEach((p) => perms.add(p))
    }
    if (role === 'reception') {
      ;[
        'user_view', 'reservation_view', 'reservation_create',
        'reservation_edit', 'reservation_check_in', 'reservation_check_out',
        'reservation_cancel', 'financial_view', 'payment_record',
        'payment_refund', 'folio_manage', 'housekeeping_view',
        'housekeeping_update_status', 'report_view_tenant', 'invoice_manage',
      ].forEach((p) => perms.add(p))
    }
    if (role === 'accountant') {
      ;[
        'financial_view', 'payment_record', 'payment_refund',
        'folio_manage', 'report_view_tenant', 'reservation_view', 'guest_export',
      ].forEach((p) => perms.add(p))
    }
    if (role === 'housekeeping') {
      ;[
        'housekeeping_view', 'housekeeping_update_status',
      ].forEach((p) => perms.add(p))
    }
  }

  return Array.from(perms)
}

export function getHomeRoute(roleStr: string): string {
  const roles = roleStr.split(',').map((r) => r.trim().toLowerCase())
  if (roles.includes('admin')) return '/admin'
  if (roles.includes('manager') || roles.includes('reception')) return '/'
  if (roles.includes('accountant')) return '/reports'
  if (roles.includes('housekeeping')) return '/rooms'
  return '/'
}

export function Login() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const auth = useAuth()
  const [loginType, setLoginType] = useState<'user' | 'admin'>('user')
  const [hotelId, setHotelId] = useState('')
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function submit(e: React.FormEvent) {
    e.preventDefault()
    setBusy(true)
    setError(null)
    const targetHotel = loginType === 'admin' ? 0 : Number(hotelId)

    // Standalone desktop: a next-shift approval is a hard gate before normal login.
    if (isDesktop && loginType === 'user') {
      try {
        const pending = await desktop.pendingShiftTransfer(Number(hotelId), username)
        if (pending) {
          const approve = window.confirm(
            `Shift handover from ${pending.from_user}: ${(pending.amount_cents / 100).toFixed(2)} ${pending.currency}. Approve the cash transfer? Choose Cancel to reject.`,
          )
          await desktop.decideShiftTransfer(pending.id, username, approve)
        }
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e))
        setBusy(false)
        return
      }
    }

    // On standalone desktop, authenticate against local encrypted store first
    if (isDesktop) {
      try {
        const off = await desktop.offlineLogin(targetHotel, username, password)
        const matchedHotel = (off as { hotel_id?: number }).hotel_id ?? targetHotel
        auth.signIn({
          token: '',
          user: {
            user_id: off.user_id,
            hotel_id: matchedHotel,
            username,
            full_name: username,
            role: off.role,
            force_password_change: false,
          },
          permissions: getDefaultPermissions(off.role),
          offline: true,
        })
        await desktop.setOfflineMode(true)
        navigate(getHomeRoute(off.role))
        return
      } catch (offlineErr) {
        const msg = offlineErr instanceof Error ? offlineErr.message : String(offlineErr)
        if (msg.includes('wrong password')) {
          setError('Wrong password')
          setBusy(false)
          return
        }
        // If not matched locally, fallback to online server if one is running
      }
    }

    try {
      const res = await api.login(targetHotel, username, password, isDesktop)
      const perms = res.permissions && res.permissions.length > 0
        ? res.permissions
        : getDefaultPermissions(res.user.role)

      auth.signIn({ token: res.token, user: res.user, permissions: perms })
      if (isDesktop) {
        try {
          await desktop.setAuthToken(res.token, targetHotel)
          if (res.offline_verifier) {
            await desktop.rememberOfflineCredentials(
              res.user.user_id, targetHotel, username, res.offline_verifier, res.user.role,
            )
          }
        } catch (bridgeError) {
          console.warn('desktop bridge:', bridgeError)
        }
      }
      navigate(getHomeRoute(res.user.role))
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div style={{ display: 'grid', placeItems: 'center', minHeight: '100vh' }}>
      <form onSubmit={submit} style={{ display: 'grid', gap: 12, width: 320 }} className="card">
        <h1 style={{ margin: 0 }}>{t('app.title')}</h1>
        
        {/* Login Type Dropdown */}
        <select 
          value={loginType} 
          onChange={(e) => setLoginType(e.target.value as 'user' | 'admin')}
          style={{ padding: '8px', fontSize: '14px' }}
        >
          <option value="user">User</option>
          <option value="admin">Admin</option>
        </select>

        {/* Hotel ID - only shown for user login */}
        {loginType === 'user' && (
          <input 
            placeholder={t('login.hotelId')} 
            value={hotelId} 
            onChange={(e) => setHotelId(e.target.value)} 
            inputMode="numeric" 
            required 
          />
        )}
        
        <input placeholder={t('login.username')} value={username} onChange={(e) => setUsername(e.target.value)} required />
        <input placeholder={t('login.password')} type="password" value={password} onChange={(e) => setPassword(e.target.value)} required />
        {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}
        <button className="primary" disabled={busy} type="submit">{busy ? t('common.loading') : t('login.signIn')}</button>
      </form>
    </div>
  )
}
