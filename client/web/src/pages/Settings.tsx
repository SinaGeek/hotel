import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { desktop, isDesktop } from '../api/desktop'

export default function Settings() {
  const navigate = useNavigate()
  const [hubUrl, setHubUrl] = useState('')
  const [hubToken, setHubToken] = useState('')
  const [hasToken, setHasToken] = useState(false)
  const [offlineMode, setOfflineMode] = useState(false)
  const [apiBase, setApiBase] = useState('')
  const [syncMode, setSyncMode] = useState<'auto' | 'manual'>('auto')
  const [loading, setLoading] = useState(false)
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null)

  useEffect(() => {
    if (!isDesktop) return
    desktop.getHubConfig().then((cfg) => {
      setHubUrl(cfg.hub_url || '')
      setHasToken(cfg.has_token)
    }).catch(console.error)
    desktop.getOfflineMode().then(setOfflineMode).catch(console.error)
    desktop.getAppPreferences().then((p) => {
      if (p.api_base) setApiBase(p.api_base)
      setSyncMode(p.sync_mode === 'manual' ? 'manual' : 'auto')
    }).catch(console.error)
  }, [])

  const handleSave = async () => {
    setLoading(true)
    setMessage(null)
    try {
      await desktop.setHubConfig(
        hubUrl.trim() || null,
        hubToken.trim() || null
      )
      setMessage({ type: 'success', text: 'Hub configuration saved' })
      if (hubToken.trim()) {
        setHasToken(true)
        setHubToken('')
      }
    } catch (e) {
      setMessage({ type: 'error', text: String(e) })
    } finally {
      setLoading(false)
    }
  }

  const handleClear = async () => {
    setLoading(true)
    setMessage(null)
    try {
      await desktop.setHubConfig(null, null)
      setHubUrl('')
      setHubToken('')
      setHasToken(false)
      setMessage({ type: 'success', text: 'Hub configuration cleared' })
    } catch (e) {
      setMessage({ type: 'error', text: String(e) })
    } finally {
      setLoading(false)
    }
  }

  if (!isDesktop) {
    return (
      <div className="p-6">
        <h1 className="text-2xl font-semibold mb-4">Settings</h1>
        <p className="text-gray-500">Settings are only available in the desktop app.</p>
      </div>
    )
  }

  return (
    <div className="p-6 max-w-2xl">
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 20 }}>
        <h1 className="text-2xl font-semibold m-0">Settings</h1>
        <button
          type="button"
          className="ghost"
          style={{ padding: '6px 14px', border: '1px solid #cbd5e1', borderRadius: 6, cursor: 'pointer' }}
          onClick={() => navigate(-1)}
        >
          Back
        </button>
      </div>

      <div className="bg-white rounded-lg shadow p-6 mb-6">
        <h2 className="text-lg font-medium mb-4">Remote server</h2>
        <p className="text-sm text-gray-600 mb-4">
          The desktop app works fully offline on its own encrypted database.
          These settings only configure the optional connection used for background/manual sync.
          Authentication stays in the login (hotel code, username, password) — it is not stored here.
        </p>
        <div className="mb-3">
          <label className="block text-sm font-medium text-gray-700 mb-1">Server address (API base)</label>
          <input
            type="text"
            className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
            placeholder="https://your-hms.example.com/api/v1"
            value={apiBase}
            onChange={(e) => setApiBase(e.target.value)}
            disabled={loading}
          />
          <p className="text-xs text-gray-500 mt-1">Saved locally; takes effect on the next sync. Changing it never contacts the server.</p>
        </div>
        <button
          className="px-4 py-2 bg-blue-600 text-white rounded-md disabled:opacity-50 mb-5"
          disabled={loading}
          onClick={async () => {
            setLoading(true)
            setMessage(null)
            try {
              await desktop.setApiBase(apiBase.trim())
              setApiBase(apiBase.trim())
              setMessage({ type: 'success', text: 'Server address saved' })
            } catch (e) {
              setMessage({ type: 'error', text: String(e) })
            } finally {
              setLoading(false)
            }
          }}
        >
          Save server address
        </button>

        <div>
          <label className="block text-sm font-medium text-gray-700 mb-2">Sync scheduling</label>
          <label className="flex items-center gap-2 mb-1">
            <input
              type="radio"
              name="syncMode"
              checked={syncMode === 'auto'}
              disabled={loading}
              onChange={async () => {
                setLoading(true)
                try { await desktop.setSyncMode('auto'); setSyncMode('auto'); setMessage({ type: 'success', text: 'Automatic sync enabled (background worker runs)' }) }
                catch (e) { setMessage({ type: 'error', text: String(e) }) }
                finally { setLoading(false) }
              }}
            />
            <span>Automatic — background sync while online</span>
          </label>
          <label className="flex items-center gap-2">
            <input
              type="radio"
              name="syncMode"
              checked={syncMode === 'manual'}
              disabled={loading}
              onChange={async () => {
                setLoading(true)
                try { await desktop.setSyncMode('manual'); setSyncMode('manual'); setMessage({ type: 'success', text: 'Manual sync — only "Sync now" transfers data' }) }
                catch (e) { setMessage({ type: 'error', text: String(e) }) }
                finally { setLoading(false) }
              }}
            />
            <span>Manual — sync only when you click "Sync now"</span>
          </label>
        </div>
      </div>

      <div className="bg-white rounded-lg shadow p-6 mb-6">
        <h2 className="text-lg font-medium mb-4">Server connection</h2>
        <p className="text-sm text-gray-600 mb-4">
          Online mode uses the configured server. If it cannot be reached, the desktop continues from its local encrypted mirror and queues writes until reconnection.
        </p>
        <label className="flex items-center gap-3 mb-3">
          <input
            type="checkbox"
            checked={offlineMode}
            disabled={loading}
            onChange={async (e) => {
              const enabled = e.target.checked
              setLoading(true)
              try {
                await desktop.setOfflineMode(enabled)
                setOfflineMode(enabled)
                setMessage({ type: 'success', text: enabled ? 'Offline mode enabled. Writes stay on this device until you switch back online.' : 'Online mode enabled. Pending writes will sync when the server responds.' })
              } catch (err) {
                setMessage({ type: 'error', text: String(err) })
              } finally {
                setLoading(false)
              }
            }}
          />
          <span className="font-medium">Use this device offline permanently until switched back</span>
        </label>
        <p className="text-xs text-amber-700 mb-6">
          Offline mode does not erase the mirror or queue. Other devices will not receive changes until online sync is re-enabled.
        </p>
      </div>

      <div className="bg-white rounded-lg shadow p-6 mb-6">
        <h2 className="text-lg font-medium mb-4">LAN Hub Configuration</h2>
        <p className="text-sm text-gray-600 mb-4">
          Connect to a local LAN hub so multiple reception PCs can share offline state.
          Leave blank to sync directly with the cloud server.
        </p>

        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Hub URL
            </label>
            <input
              type="text"
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
              placeholder="http://192.168.1.50:8089"
              value={hubUrl}
              onChange={(e) => setHubUrl(e.target.value)}
              disabled={loading}
            />
            <p className="text-xs text-gray-500 mt-1">
              Example: http://192.168.1.50:8089
            </p>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Hub Token
            </label>
            <input
              type="password"
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
              placeholder={hasToken ? '(saved)' : 'Bearer token'}
              value={hubToken}
              onChange={(e) => setHubToken(e.target.value)}
              disabled={loading}
            />
            <p className="text-xs text-gray-500 mt-1">
              {hasToken ? 'Token is saved. Enter a new token to replace it.' : 'Authentication token provided by hub administrator'}
            </p>
          </div>

          {message && (
            <div
              className={`p-3 rounded-md text-sm ${
                message.type === 'success'
                  ? 'bg-green-50 text-green-800'
                  : 'bg-red-50 text-red-800'
              }`}
            >
              {message.text}
            </div>
          )}

          <div className="flex gap-3 pt-2">
            <button
              onClick={handleSave}
              disabled={loading}
              className="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:opacity-50"
            >
              {loading ? 'Saving...' : 'Save'}
            </button>
            <button
              onClick={handleClear}
              disabled={loading}
              className="px-4 py-2 bg-gray-200 text-gray-700 rounded-md hover:bg-gray-300 disabled:opacity-50"
            >
              Clear
            </button>
          </div>
        </div>
      </div>

      <div className="bg-white rounded-lg shadow p-6 mb-6">
        <h2 className="text-lg font-medium mb-4">Bulk Import</h2>
        <p className="text-sm text-gray-600 mb-4">
          Load pre-existing data from your legacy system into the local database.
          All imports are idempotent — re-running updates existing rows instead of duplicating.
          Default users (admin, hotel 4442 staff) are already provisioned on first run.
        </p>

        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Guest Register CSV (guest-daily_*.csv)
            </label>
            <input
              type="text"
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 mb-2"
              placeholder="C:\path\to\guest-daily_2026-06-01_2028-11-30.csv"
              id="guestsPath"
            />
            <button
              className="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:opacity-50"
              disabled={loading}
              onClick={async () => {
                setLoading(true)
                setMessage(null)
                try {
                  const path = (document.getElementById('guestsPath') as HTMLInputElement).value
                  const result = await desktop.bulkImport('guests', path)
                  setMessage({ type: 'success', text: `Guests: ${result.inserted} inserted, ${result.updated} updated, ${result.skipped} skipped${result.error_count ? `, ${result.error_count} errors` : ''}` })
                } catch (e) {
                  setMessage({ type: 'error', text: String(e) })
                } finally {
                  setLoading(false)
                }
              }}
            >
              Import Guests
            </button>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Collections Ledger CSV (collection_*.csv)
            </label>
            <input
              type="text"
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 mb-2"
              placeholder="C:\path\to\collection_2026-06-01_2028-11-01.csv"
              id="collectionsPath"
            />
            <button
              className="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:opacity-50"
              disabled={loading}
              onClick={async () => {
                setLoading(true)
                setMessage(null)
                try {
                  const path = (document.getElementById('collectionsPath') as HTMLInputElement).value
                  const result = await desktop.bulkImport('collections', path)
                  setMessage({ type: 'success', text: `Collections: ${result.inserted} inserted, ${result.updated} updated, ${result.skipped} skipped${result.error_count ? `, ${result.error_count} errors` : ''}` })
                } catch (e) {
                  setMessage({ type: 'error', text: String(e) })
                } finally {
                  setLoading(false)
                }
              }}
            >
              Import Collections
            </button>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Reservations Workbook (Reservation.xlsx)
            </label>
            <input
              type="text"
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 mb-2"
              placeholder="C:\path\to\Reservation.xlsx"
              id="reservationsPath"
            />
            <input
              type="text"
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 mb-2"
              placeholder="Anchor date (YYYY-MM-DD, default: 2026-06-01)"
              id="reservationsAnchor"
            />
            <p className="text-xs text-gray-500 mb-2">
              The xlsx has no dates; unmatched rows get sequential dates starting from the anchor.
            </p>
            <button
              className="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:opacity-50"
              disabled={loading}
              onClick={async () => {
                setLoading(true)
                setMessage(null)
                try {
                  const path = (document.getElementById('reservationsPath') as HTMLInputElement).value
                  const anchor = (document.getElementById('reservationsAnchor') as HTMLInputElement).value || '2026-06-01'
                  const result = await desktop.bulkImport('reservations', path, anchor)
                  setMessage({ type: 'success', text: `Reservations: ${result.inserted} inserted, ${result.updated} updated, ${result.skipped} skipped${result.error_count ? `, ${result.error_count} errors` : ''}` })
                } catch (e) {
                  setMessage({ type: 'error', text: String(e) })
                } finally {
                  setLoading(false)
                }
              }}
            >
              Import Reservations
            </button>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              User Credentials (userpass.md)
            </label>
            <input
              type="text"
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 mb-2"
              placeholder="C:\path\to\userpass.md"
              id="usersPath"
            />
            <p className="text-xs text-gray-500 mb-2">
              Additional users to provision (default users are already created on first run).
            </p>
            <button
              className="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:opacity-50"
              disabled={loading}
              onClick={async () => {
                setLoading(true)
                setMessage(null)
                try {
                  const path = (document.getElementById('usersPath') as HTMLInputElement).value
                  const result = await desktop.bulkImport('users', path)
                  setMessage({ type: 'success', text: `Users: ${result.inserted} inserted, ${result.updated} updated${result.error_count ? `, ${result.error_count} errors` : ''}` })
                } catch (e) {
                  setMessage({ type: 'error', text: String(e) })
                } finally {
                  setLoading(false)
                }
              }}
            >
              Import Users
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
