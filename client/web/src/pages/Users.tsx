// Users (manager) + permission matrix (README §6). Manager can create/edit
// accounts below manager level with multi-role support; admin sees all.
import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { api, ApiError, sessionHasPermission } from '../api/client'
import { useAuth } from '../api/auth'
import type { RoleMatrix, UserDto } from '../types/generated/api'

export function Users() {
  const { t } = useTranslation()
  const { session } = useAuth()
  const [users, setUsers] = useState<UserDto[]>([])
  const [roleMatrices, setRoleMatrices] = useState<RoleMatrix[]>([])
  const [username, setUsername] = useState('')
  const [fullName, setFullName] = useState('')
  const [selectedRoles, setSelectedRoles] = useState<string[]>(['reception'])
  const [password, setPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [editingUser, setEditingUser] = useState<UserDto | null>(null)
  const [editRoles, setEditRoles] = useState<string[]>([])
  const [editFullName, setEditFullName] = useState('')
  const [editPassword, setEditPassword] = useState('')
  const [busy, setBusy] = useState(false)

  const canManage = sessionHasPermission(session, 'user_manage')
  const isAdmin = session?.user.role === 'admin'

  const availableRoles = [
    { key: 'reception', label: t('users.reception') },
    { key: 'accountant', label: t('users.accountant') },
    { key: 'housekeeping', label: t('users.housekeeping') },
    ...(isAdmin ? [{ key: 'manager', label: t('users.manager') }] : []),
  ]

  const load = useCallback(async () => {
    try {
      setError(null)
      setUsers(await api.listUsers())
      if (isAdmin) {
        setRoleMatrices(
          await Promise.all(['admin', 'manager', 'reception', 'accountant', 'housekeeping'].map((r) => api.roleMatrix(r)))
        )
      }
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    }
  }, [isAdmin])

  useEffect(() => {
    void load()
  }, [load])

  function toggleRole(role: string, current: string[], setter: (r: string[]) => void) {
    if (current.includes(role)) {
      if (current.length > 1) {
        setter(current.filter((r) => r !== role))
      }
    } else {
      setter([...current, role])
    }
  }

  async function create(e: React.FormEvent) {
    e.preventDefault()
    if (selectedRoles.length === 0) {
      setError('Select at least one role')
      return
    }
    setBusy(true)
    setError(null)
    try {
      await api.createUser({
        username,
        full_name: fullName || username,
        role: selectedRoles.join(','),
        password,
      })
      setUsername('')
      setFullName('')
      setPassword('')
      setSelectedRoles(['reception'])
      await load()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  function startEdit(u: UserDto) {
    setEditingUser(u)
    setEditFullName(u.full_name || '')
    setEditRoles(u.role.split(',').map((r) => r.trim()))
    setEditPassword('')
  }

  async function saveEdit(e: React.FormEvent) {
    e.preventDefault()
    if (!editingUser) return
    if (editRoles.length === 0) {
      setError('User must have at least one role')
      return
    }
    setBusy(true)
    setError(null)
    try {
      await api.updateUser(editingUser.id, {
        full_name: editFullName,
        role: editRoles.join(','),
        ...(editPassword ? { password: editPassword } : {}),
      })
      setEditingUser(null)
      await load()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  async function toggleActive(u: UserDto) {
    try {
      setError(null)
      await api.updateUser(u.id, { is_active: !u.is_active })
      await load()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    }
  }

  function formatRole(roleStr: string) {
    return roleStr
      .split(',')
      .map((r) => {
        const tr = t(`users.${r.trim()}`)
        return tr || r.trim()
      })
      .join(', ')
  }

  return (
    <div className="page" style={{ padding: 24, maxWidth: 1000, margin: '0 auto' }}>
      <h2>{t('users.title')}</h2>
      <p style={{ color: '#64748b' }}>Manage user accounts, assign multiple roles, and change passwords.</p>

      {error && (
        <div style={{ padding: 12, backgroundColor: '#fef2f2', color: '#dc2626', borderRadius: 8, marginBottom: 16 }}>
          {error}
        </div>
      )}

      {/* Edit User Modal */}
      {editingUser && (
        <div
          style={{
            position: 'fixed',
            inset: 0,
            backgroundColor: 'rgba(0,0,0,0.5)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 1000,
          }}
        >
          <form
            onSubmit={saveEdit}
            className="card"
            style={{ width: 440, padding: 24, display: 'grid', gap: 16, backgroundColor: 'white', borderRadius: 12 }}
          >
            <h3>Edit User: {editingUser.username}</h3>
            <div>
              <label style={{ display: 'block', fontSize: 13, fontWeight: 600, marginBottom: 4 }}>Full Name</label>
              <input
                value={editFullName}
                onChange={(e) => setEditFullName(e.target.value)}
                style={{ width: '100%', padding: 8 }}
              />
            </div>
            <div>
              <label style={{ display: 'block', fontSize: 13, fontWeight: 600, marginBottom: 6 }}>
                Roles (select all that apply):
              </label>
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 12 }}>
                {availableRoles.map((r) => (
                  <label key={r.key} style={{ display: 'flex', alignItems: 'center', gap: 6, cursor: 'pointer' }}>
                    <input
                      type="checkbox"
                      checked={editRoles.includes(r.key)}
                      onChange={() => toggleRole(r.key, editRoles, setEditRoles)}
                    />
                    <span>{r.label}</span>
                  </label>
                ))}
              </div>
            </div>
            <div>
              <label style={{ display: 'block', fontSize: 13, fontWeight: 600, marginBottom: 4 }}>
                New Password (leave blank to keep current)
              </label>
              <input
                type="password"
                placeholder="Enter new password"
                value={editPassword}
                onChange={(e) => setEditPassword(e.target.value)}
                style={{ width: '100%', padding: 8 }}
              />
            </div>
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 8 }}>
              <button type="button" className="ghost" onClick={() => setEditingUser(null)} disabled={busy}>
                {t('common.cancel')}
              </button>
              <button type="submit" className="primary" disabled={busy}>
                {busy ? t('common.loading') : t('common.save')}
              </button>
            </div>
          </form>
        </div>
      )}

      {/* Users Table */}
      <div className="table-wrap" style={{ border: '1px solid #e2e8f0', borderRadius: 8, overflow: 'hidden' }}>
        <table className="hms" style={{ width: '100%', borderCollapse: 'collapse' }}>
          <thead>
            <tr style={{ backgroundColor: '#f8fafc', borderBottom: '1px solid #e2e8f0', textAlign: 'left' }}>
              <th style={{ padding: '12px 16px' }}>{t('login.username')}</th>
              <th style={{ padding: '12px 16px' }}>{t('users.fullName')}</th>
              <th style={{ padding: '12px 16px' }}>{t('users.role')}</th>
              <th style={{ padding: '12px 16px' }}>{t('users.active')}</th>
              {canManage && <th style={{ padding: '12px 16px' }}>Actions</th>}
            </tr>
          </thead>
          <tbody>
            {users.map((u) => (
              <tr key={u.id} style={{ borderBottom: '1px solid #e2e8f0' }}>
                <td style={{ padding: '12px 16px', fontWeight: 600 }}>{u.username}</td>
                <td style={{ padding: '12px 16px' }}>{u.full_name}</td>
                <td style={{ padding: '12px 16px' }}>
                  <span
                    style={{
                      padding: '2px 8px',
                      borderRadius: 10,
                      backgroundColor: '#eff6ff',
                      color: '#1d4ed8',
                      fontWeight: 600,
                      fontSize: 12,
                    }}
                  >
                    {formatRole(u.role)}
                  </span>
                </td>
                <td style={{ padding: '12px 16px' }}>
                  <span
                    style={{
                      padding: '2px 8px',
                      borderRadius: 12,
                      fontSize: 12,
                      backgroundColor: u.is_active ? '#dcfce7' : '#fee2e2',
                      color: u.is_active ? '#15803d' : '#b91c1c',
                    }}
                  >
                    {u.is_active ? t('users.active') : t('users.passive')}
                  </span>
                </td>
                {canManage && (
                  <td style={{ padding: '12px 16px' }}>
                    <div style={{ display: 'flex', gap: 6 }}>
                      <button
                        type="button"
                        style={{
                          padding: '4px 8px',
                          fontSize: 12,
                          border: '1px solid #cbd5e1',
                          borderRadius: 4,
                          cursor: 'pointer',
                          backgroundColor: 'white',
                        }}
                        onClick={() => startEdit(u)}
                      >
                        Edit
                      </button>
                      {u.username !== 'admin' && (
                        <button
                          type="button"
                          style={{
                            padding: '4px 8px',
                            fontSize: 12,
                            backgroundColor: u.is_active ? '#fee2e2' : '#dcfce7',
                            color: u.is_active ? '#b91c1c' : '#15803d',
                            border: 'none',
                            borderRadius: 4,
                            cursor: 'pointer',
                          }}
                          onClick={() => toggleActive(u)}
                        >
                          {u.is_active ? 'Disable' : 'Enable'}
                        </button>
                      )}
                    </div>
                  </td>
                )}
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* Add User Form */}
      {canManage && (
        <form
          className="card"
          style={{ marginTop: 20, padding: 20, display: 'grid', gap: 14 }}
          onSubmit={create}
        >
          <h3 style={{ margin: 0 }}>Add New User</h3>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(180px, 1fr))', gap: 12 }}>
            <input
              placeholder={t('login.username')}
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              required
              style={{ padding: 8 }}
            />
            <input
              placeholder={t('users.fullName')}
              value={fullName}
              onChange={(e) => setFullName(e.target.value)}
              style={{ padding: 8 }}
            />
            <input
              placeholder={t('users.password')}
              type="password"
              value={password}
              minLength={3}
              onChange={(e) => setPassword(e.target.value)}
              required
              style={{ padding: 8 }}
            />
          </div>
          <div>
            <label style={{ display: 'block', fontSize: 13, fontWeight: 600, marginBottom: 6 }}>
              Assign Roles (select multiple if applicable):
            </label>
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 16 }}>
              {availableRoles.map((r) => (
                <label key={r.key} style={{ display: 'flex', alignItems: 'center', gap: 6, cursor: 'pointer' }}>
                  <input
                    type="checkbox"
                    checked={selectedRoles.includes(r.key)}
                    onChange={() => toggleRole(r.key, selectedRoles, setSelectedRoles)}
                  />
                  <span>{r.label}</span>
                </label>
              ))}
            </div>
          </div>
          <div style={{ display: 'flex', justifyContent: 'flex-start' }}>
            <button className="primary" type="submit" disabled={busy}>
              {busy ? t('common.loading') : t('users.create')}
            </button>
          </div>
        </form>
      )}

      {/* Permission matrix for admin */}
      {roleMatrices.length > 0 && (
        <div style={{ marginTop: 24 }}>
          <h3>{t('users.matrix')}</h3>
          {roleMatrices.map((m) => (
            <div key={m.role} className="card" style={{ marginBottom: 8, padding: 12 }}>
              <b>{t(`users.${m.role}`) || m.role}</b>{' '}
              <span className="muted">
                ({m.permissions.filter((p) => p.effective).length}/{m.permissions.length})
              </span>
              <div className="row" style={{ gap: 6, flexWrap: 'wrap', marginTop: 6 }}>
                {m.permissions
                  .filter((p) => p.effective)
                  .map((p) => (
                    <span
                      key={p.permission}
                      style={{ background: '#eaf0ff', padding: '2px 8px', borderRadius: 10, fontSize: 12 }}
                    >
                      {p.permission}
                    </span>
                  ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
