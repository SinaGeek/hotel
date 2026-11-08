import { useCallback, useEffect, useState } from 'react'
import { api, ApiError } from '../api/client'
import type { HotelDto, UserDto } from '../types/generated/api'

/**
 * Platform admin panel: create hotels (each with a manager account), add users
 * per hotel, and enable/disable hotels and users. The sidebar hides the link
 * unless the session holds `hotel_create` / `user_manage`, and the server
 * enforces both permissions.
 */
export function Admin() {
  const [hotels, setHotels] = useState<HotelDto[]>([])
  const [users, setUsers] = useState<UserDto[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const [showCreateHotel, setShowCreateHotel] = useState(false)
  const [showCreateUser, setShowCreateUser] = useState(false)
  const [newHotel, setNewHotel] = useState({ name: '', code: '' })
  const [newUser, setNewUser] = useState({
    hotel_id: '',
    username: '',
    full_name: '',
    role: 'reception',
    password: '',
  })

  const refresh = useCallback(async () => {
    try {
      setError(null)
      const [hotelsRes, usersRes] = await Promise.all([api.listHotels(), api.listUsers()])
      setHotels(hotelsRes)
      setUsers(usersRes)
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  async function createHotel() {
    const code = Number(newHotel.code)
    if (!newHotel.name.trim() || !Number.isInteger(code) || code <= 0) {
      setError('Enter a hotel name and a positive numeric hotel code.')
      return
    }
    setBusy(true)
    try {
      setError(null)
      // The hotel code doubles as the hotel id (README §7).
      await api.createHotel({ id: code, name: newHotel.name.trim() })
      // Every hotel starts with a manager account; the password must be changed
      // on first login (server sets force_password_change).
      await api.createUser({
        hotel_id: code,
        username: 'manager',
        full_name: 'Hotel Manager',
        role: 'manager',
        password: '1234',
      })
      setNewHotel({ name: '', code: '' })
      setShowCreateHotel(false)
      await refresh()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  async function createUser() {
    const hotelId = Number(newUser.hotel_id)
    if (!Number.isInteger(hotelId) || hotelId <= 0 || !newUser.username.trim() || !newUser.password) {
      setError('Pick a hotel and enter a username and password.')
      return
    }
    setBusy(true)
    try {
      setError(null)
      await api.createUser({
        hotel_id: hotelId,
        username: newUser.username.trim(),
        full_name: newUser.full_name.trim() || newUser.username.trim(),
        role: newUser.role,
        password: newUser.password,
      })
      setNewUser({ hotel_id: '', username: '', full_name: '', role: 'reception', password: '' })
      setShowCreateUser(false)
      await refresh()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  async function toggleHotel(hotel: HotelDto) {
    setBusy(true)
    try {
      setError(null)
      await api.updateHotel(hotel.id, { is_active: !hotel.is_active })
      await refresh()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  async function toggleUser(user: UserDto) {
    setBusy(true)
    try {
      setError(null)
      await api.updateUser(user.id, { is_active: !user.is_active })
      await refresh()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  if (loading) return <div className="page">Loading…</div>

  return (
    <div className="page">
      <h2>Admin Panel</h2>
      <p className="muted" style={{ marginTop: -4 }}>
        Create hotels and their users. Each new hotel gets a manager account.
      </p>

      {error && <p style={{ color: 'var(--red-text)' }}>Error: {error}</p>}

      {/* ------------------------------------------------------------ hotels */}
      <div className="card" style={{ marginBottom: 16 }}>
        <div className="row" style={{ justifyContent: 'space-between', marginBottom: 10 }}>
          <strong>Hotels ({hotels.length})</strong>
          <button className="primary" disabled={busy} onClick={() => setShowCreateHotel(true)}>
            + Add hotel
          </button>
        </div>

        {showCreateHotel && (
          <div style={{ background: 'var(--surface-2)', borderRadius: 8, padding: 12, marginBottom: 12 }}>
            <div className="row">
              <input
                placeholder="Hotel name"
                value={newHotel.name}
                onChange={(e) => setNewHotel((h) => ({ ...h, name: e.target.value }))}
                style={{ width: 220 }}
              />
              <input
                placeholder="Hotel code (e.g. 4442)"
                type="number"
                value={newHotel.code}
                onChange={(e) => setNewHotel((h) => ({ ...h, code: e.target.value }))}
                style={{ width: 180 }}
              />
              <button className="primary" disabled={busy} onClick={() => void createHotel()}>
                Create hotel + manager
              </button>
              <button className="ghost" onClick={() => setShowCreateHotel(false)}>
                Cancel
              </button>
            </div>
            <p className="muted" style={{ marginBottom: 0 }}>
              The code becomes the hotel id. A manager account is created with username
              <code> manager </code> and password <code>1234</code>; it must be changed on first
              login.
            </p>
          </div>
        )}

        <div className="table-wrap">
          <table className="hms">
            <thead>
              <tr>
                <th>Code</th>
                <th>Name</th>
                <th>Status</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {hotels.map((h) => (
                <tr key={h.id}>
                  <td style={{ fontFamily: 'monospace' }}>{h.id}</td>
                  <td>{h.name}</td>
                  <td>
                    <span className={`pill ${h.is_active ? 'green' : 'red'}`}>
                      {h.is_active ? 'Active' : 'Disabled'}
                    </span>
                  </td>
                  <td>
                    <button className="ghost" disabled={busy} onClick={() => void toggleHotel(h)}>
                      {h.is_active ? 'Disable' : 'Enable'}
                    </button>
                  </td>
                </tr>
              ))}
              {hotels.length === 0 && (
                <tr>
                  <td colSpan={4} className="muted">
                    No hotels yet.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>

      {/* ------------------------------------------------------------- users */}
      <div className="card">
        <div className="row" style={{ justifyContent: 'space-between', marginBottom: 10 }}>
          <strong>Users ({users.length})</strong>
          <button className="primary" disabled={busy} onClick={() => setShowCreateUser(true)}>
            + Add user
          </button>
        </div>

        {showCreateUser && (
          <div style={{ background: 'var(--surface-2)', borderRadius: 8, padding: 12, marginBottom: 12 }}>
            <div className="row">
              <select
                value={newUser.hotel_id}
                onChange={(e) => setNewUser((u) => ({ ...u, hotel_id: e.target.value }))}
              >
                <option value="">Select hotel</option>
                {hotels.map((h) => (
                  <option key={h.id} value={String(h.id)}>
                    {h.id} — {h.name}
                  </option>
                ))}
              </select>
              <input
                placeholder="Username"
                value={newUser.username}
                onChange={(e) => setNewUser((u) => ({ ...u, username: e.target.value }))}
                style={{ width: 140 }}
              />
              <input
                placeholder="Full name"
                value={newUser.full_name}
                onChange={(e) => setNewUser((u) => ({ ...u, full_name: e.target.value }))}
                style={{ width: 170 }}
              />
              <select
                value={newUser.role}
                onChange={(e) => setNewUser((u) => ({ ...u, role: e.target.value }))}
              >
                <option value="manager">Manager</option>
                <option value="reception">Reception</option>
                <option value="housekeeping">Housekeeping</option>
              </select>
              <input
                placeholder="Initial password"
                type="password"
                value={newUser.password}
                onChange={(e) => setNewUser((u) => ({ ...u, password: e.target.value }))}
                style={{ width: 150 }}
              />
              <button className="primary" disabled={busy} onClick={() => void createUser()}>
                Create user
              </button>
              <button className="ghost" onClick={() => setShowCreateUser(false)}>
                Cancel
              </button>
            </div>
            <p className="muted" style={{ marginBottom: 0 }}>
              New users must change this password on first login. Roles follow the permission matrix
              in the README.
            </p>
          </div>
        )}

        <div className="table-wrap">
          <table className="hms">
            <thead>
              <tr>
                <th>Hotel</th>
                <th>Username</th>
                <th>Full name</th>
                <th>Role</th>
                <th>Status</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {users.map((u) => (
                <tr key={u.id}>
                  <td style={{ fontFamily: 'monospace' }}>{u.hotel_id}</td>
                  <td style={{ fontFamily: 'monospace' }}>{u.username}</td>
                  <td>{u.full_name || '—'}</td>
                  <td>
                    <span
                      className={`pill ${
                        u.role === 'admin' ? 'violet' : u.role === 'manager' ? 'gold' : u.role === 'reception' ? 'green' : 'gray'
                      }`}
                    >
                      {u.role}
                    </span>
                  </td>
                  <td>
                    <span className={`pill ${u.is_active ? 'green' : 'red'}`}>
                      {u.is_active ? 'Active' : 'Disabled'}
                    </span>
                  </td>
                  <td>
                    <button className="ghost" disabled={busy} onClick={() => void toggleUser(u)}>
                      {u.is_active ? 'Disable' : 'Enable'}
                    </button>
                  </td>
                </tr>
              ))}
              {users.length === 0 && (
                <tr>
                  <td colSpan={6} className="muted">
                    No users yet.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
        <p className="muted" style={{ marginBottom: 0 }}>
          Showing users across all hotels (platform admin view).
        </p>
      </div>
    </div>
  )
}
