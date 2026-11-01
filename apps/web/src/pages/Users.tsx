// Users (manager) + permission matrix (README Â§6). Manager can create
// accounts below manager level; admin sees all roles' matrices.
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
  const [role, setRole] = useState('reception')
  const [password, setPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const canManage = sessionHasPermission(session, 'user_manage')

  const load = useCallback(async () => {
    try {
      setError(null)
      setUsers(await api.listUsers())
      if (session?.user.role === 'admin') {
        setRoleMatrices(await Promise.all(['admin', 'manager', 'reception', 'housekeeping'].map((r) => api.roleMatrix(r))))
      }
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    }
  }, [session])
  useEffect(() => { void load() }, [load])

  async function create(e: React.FormEvent) {
    e.preventDefault()
    try {
      await api.createUser({ username, full_name: fullName, role, password })
      setUsername(''); setFullName(''); setPassword('')
      await load()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    }
  }

  return (
    <div className="page">
      <h2>{t('users.title')}</h2>
      {error && <p style={{ color: 'red' }}>{t('common.error')}: {error}</p>}
      <div className="table-wrap"><table className="hms">
        <thead><tr><th>{t('login.username')}</th><th>{t('users.fullName')}</th><th>{t('users.role')}</th><th>{t('users.active')}</th></tr></thead>
        <tbody>
          {users.map((u) => (
            <tr key={u.id}>
              <td>{u.username}</td>
              <td>{u.full_name}</td>
              <td>{t(`users.${u.role}`)}</td>
              <td>{u.is_active ? t('users.active') : t('users.passive')}</td>
            </tr>
          ))}
        </tbody>
      </table></div>

      {canManage && (
        <form className="card row" style={{ marginTop: 12 }} onSubmit={create}>
          <input placeholder={t('login.username')} value={username} onChange={(e) => setUsername(e.target.value)} required style={{ width: 140 }} />
          <input placeholder={t('users.fullName')} value={fullName} onChange={(e) => setFullName(e.target.value)} style={{ width: 160 }} />
          <select value={role} onChange={(e) => setRole(e.target.value)}>
            <option value="reception">{t('users.reception')}</option>
            <option value="housekeeping">{t('users.housekeeping')}</option>
            <option value="manager" disabled={session?.user.role !== 'admin'}>{t('users.manager')}</option>
          </select>
          <input placeholder={t('users.password')} type="password" value={password} minLength={3} onChange={(e) => setPassword(e.target.value)} required style={{ width: 140 }} />
          <button className="primary" type="submit">{t('users.create')}</button>
        </form>
      )}

      {roleMatrices.length > 0 && (
        <div style={{ marginTop: 18 }}>
          <h3>{t('users.matrix')}</h3>
          {roleMatrices.map((m) => (
            <div key={m.role} className="card" style={{ marginBottom: 8 }}>
              <b>{t(`users.${m.role}`)}</b> <span className="muted">({m.permissions.filter((p) => p.effective).length}/{m.permissions.length})</span>
              <div className="row" style={{ gap: 6 }}>
                {m.permissions.filter((p) => p.effective).map((p) => (
                  <span key={p.permission} style={{ background: '#eaf0ff', padding: '2px 8px', borderRadius: 10, fontSize: 12 }}>{p.permission}</span>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
