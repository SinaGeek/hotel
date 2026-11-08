import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { desktop, isDesktop } from '../api/desktop'
import { useAuth } from '../api/auth'

export default function AdminPanel() {
  const navigate = useNavigate()
  const auth = useAuth()
  const [loading, setLoading] = useState(false)
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null)
  const [guestsPath, setGuestsPath] = useState('')
  const [collectionsPath, setCollectionsPath] = useState('')
  const [reservationsPath, setReservationsPath] = useState('')
  const [reservationsAnchor, setReservationsAnchor] = useState('2026-06-01')
  const [usersPath, setUsersPath] = useState('')

  // Admin (hotel 0) or Hotel Manager can access bulk import
  const isAllowed = auth.user && (auth.user.hotel_id === 0 || auth.user.role.includes('admin') || auth.user.role.includes('manager'))
  if (!isAllowed) {
    return (
      <div className="min-h-screen bg-gradient-to-br from-gray-50 to-gray-100 p-8">
        <div className="max-w-4xl mx-auto">
          <div className="bg-white rounded-2xl shadow-xl p-8">
            <h1 className="text-3xl font-bold text-gray-900 mb-4">Bulk Import</h1>
            <p className="text-red-600">Access denied. Bulk import is only available for managers and platform admins.</p>
          </div>
        </div>
      </div>
    )
  }

  if (!isDesktop) {
    return (
      <div className="min-h-screen bg-gradient-to-br from-gray-50 to-gray-100 p-8">
        <div className="max-w-4xl mx-auto">
          <div className="bg-white rounded-2xl shadow-xl p-8">
            <h1 className="text-3xl font-bold text-gray-900 mb-4">Admin Panel</h1>
            <p className="text-gray-500">Admin panel is only available in the desktop app.</p>
          </div>
        </div>
      </div>
    )
  }

  const pickFile = async (fileTypes: Array<[string, string[]]>, setter: (path: string) => void) => {
    try {
      const selected = await desktop.pickFile(fileTypes)
      if (selected) setter(selected)
    } catch (e) {
      console.error('File picker error:', e)
      setMessage({ type: 'error', text: `File picker error: ${e}` })
    }
  }

  const handleImport = async (
    kind: 'guests' | 'collections' | 'reservations' | 'users',
    path: string,
    anchor?: string
  ) => {
    if (!path) {
      setMessage({ type: 'error', text: 'Please select a file first' })
      return
    }
    setLoading(true)
    setMessage(null)
    try {
      const result = await desktop.bulkImport(kind, path, anchor)
      setMessage({
        type: 'success',
        text: `${result.dataset}: ${result.inserted} inserted, ${result.updated} updated, ${result.skipped} skipped${result.error_count ? ` — ${result.error_count} errors (check console)` : ''}`
      })
      if (result.errors.length > 0) console.warn('Import errors:', result.errors)
    } catch (e) {
      setMessage({ type: 'error', text: String(e) })
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="min-h-screen bg-gradient-to-br from-blue-50 via-indigo-50 to-purple-50 p-12">
      <div className="max-w-6xl mx-auto">
        {/* Header */}
        <div className="bg-white rounded-2xl shadow-2xl p-10 mb-10">
          <div className="flex items-center justify-between">
            <div>
              <h1 className="text-5xl font-bold bg-gradient-to-r from-blue-600 to-purple-600 bg-clip-text text-transparent mb-3">
                Bulk Data Import
              </h1>
              <p className="text-gray-600 text-lg">Legacy Data Migration & Database Population</p>
            </div>
            <div style={{ display: 'flex', gap: 12 }}>
              <button
                className="px-6 py-4 bg-white text-gray-700 rounded-xl hover:bg-gray-100 transition-all duration-200 font-semibold shadow-md border border-gray-200"
                onClick={() => navigate(-1)}
              >
                Back
              </button>
              <button
                className="px-8 py-4 bg-gradient-to-r from-gray-100 to-gray-200 text-gray-700 rounded-xl hover:from-gray-200 hover:to-gray-300 transition-all duration-200 font-semibold shadow-md"
                onClick={() => {
                  auth.signOut()
                  navigate('/login')
                }}
              >
                Sign out
              </button>
            </div>
          </div>
        </div>

        {/* Message Banner */}
        {message && (
          <div
            className={`mb-10 p-8 rounded-2xl shadow-2xl ${
              message.type === 'success'
                ? 'bg-gradient-to-r from-green-50 to-emerald-50 border-2 border-green-300'
                : 'bg-gradient-to-r from-red-50 to-pink-50 border-2 border-red-300'
            }`}
          >
            <p className={`text-lg font-semibold ${message.type === 'success' ? 'text-green-800' : 'text-red-800'}`}>
              {message.text}
            </p>
          </div>
        )}

        {/* Import Cards */}
        <div className="space-y-8">
          {/* 1. Guests */}
          <div className="bg-white rounded-2xl shadow-2xl p-10 hover:shadow-3xl transition-all duration-300">
            <div className="flex items-start gap-6">
              <div className="flex-shrink-0 w-16 h-16 bg-gradient-to-br from-blue-500 to-blue-600 rounded-2xl flex items-center justify-center text-white font-bold text-2xl shadow-xl">
                1
              </div>
              <div className="flex-1 space-y-6">
                <div>
                  <h3 className="text-2xl font-bold text-gray-900 mb-3">Guest Register CSV</h3>
                  <p className="text-gray-600 text-base">
                    Import guests and reservations from guest-daily_*.csv (semicolon-delimited)
                  </p>
                </div>
                <div className="space-y-4">
                  <input
                    type="text"
                    className="w-full px-6 py-4 border-2 border-gray-200 rounded-xl focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent transition-all text-base bg-gray-50"
                    placeholder="No file selected..."
                    value={guestsPath}
                    readOnly
                  />
                  <div className="flex gap-4">
                    <button
                      className="flex-1 px-8 py-4 bg-gradient-to-r from-gray-100 to-gray-200 text-gray-700 rounded-xl hover:from-gray-200 hover:to-gray-300 transition-all duration-200 font-semibold shadow-md text-base"
                      onClick={() => pickFile([['CSV Files', ['csv']]], setGuestsPath)}
                    >
                      Browse Files
                    </button>
                    <button
                      className="flex-1 px-8 py-4 bg-gradient-to-r from-blue-600 to-blue-700 text-white rounded-xl hover:from-blue-700 hover:to-blue-800 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-200 font-semibold shadow-xl text-base"
                      disabled={loading || !guestsPath}
                      onClick={() => handleImport('guests', guestsPath)}
                    >
                      {loading ? 'Importing...' : 'Import Guests'}
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </div>

          {/* 2. Collections */}
          <div className="bg-white rounded-2xl shadow-2xl p-10 hover:shadow-3xl transition-all duration-300">
            <div className="flex items-start gap-6">
              <div className="flex-shrink-0 w-16 h-16 bg-gradient-to-br from-green-500 to-green-600 rounded-2xl flex items-center justify-center text-white font-bold text-2xl shadow-xl">
                2
              </div>
              <div className="flex-1 space-y-6">
                <div>
                  <h3 className="text-2xl font-bold text-gray-900 mb-3">Collections Ledger CSV</h3>
                  <p className="text-gray-600 text-base">
                    Import payment collections from collection_*.csv (semicolon-delimited)
                  </p>
                </div>
                <div className="space-y-4">
                  <input
                    type="text"
                    className="w-full px-6 py-4 border-2 border-gray-200 rounded-xl focus:outline-none focus:ring-2 focus:ring-green-500 focus:border-transparent transition-all text-base bg-gray-50"
                    placeholder="No file selected..."
                    value={collectionsPath}
                    readOnly
                  />
                  <div className="flex gap-4">
                    <button
                      className="flex-1 px-8 py-4 bg-gradient-to-r from-gray-100 to-gray-200 text-gray-700 rounded-xl hover:from-gray-200 hover:to-gray-300 transition-all duration-200 font-semibold shadow-md text-base"
                      onClick={() => pickFile([['CSV Files', ['csv']]], setCollectionsPath)}
                    >
                      Browse Files
                    </button>
                    <button
                      className="flex-1 px-8 py-4 bg-gradient-to-r from-green-600 to-green-700 text-white rounded-xl hover:from-green-700 hover:to-green-800 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-200 font-semibold shadow-xl text-base"
                      disabled={loading || !collectionsPath}
                      onClick={() => handleImport('collections', collectionsPath)}
                    >
                      {loading ? 'Importing...' : 'Import Collections'}
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </div>

          {/* 3. Reservations */}
          <div className="bg-white rounded-2xl shadow-2xl p-10 hover:shadow-3xl transition-all duration-300">
            <div className="flex items-start gap-6">
              <div className="flex-shrink-0 w-16 h-16 bg-gradient-to-br from-purple-500 to-purple-600 rounded-2xl flex items-center justify-center text-white font-bold text-2xl shadow-xl">
                3
              </div>
              <div className="flex-1 space-y-6">
                <div>
                  <h3 className="text-2xl font-bold text-gray-900 mb-3">Reservations Workbook (Excel)</h3>
                  <p className="text-gray-600 text-base">
                    Import from Reservation.xlsx — unmatched rows get sequential dates from anchor
                  </p>
                </div>
                <div className="space-y-4">
                  <input
                    type="text"
                    className="w-full px-6 py-4 border-2 border-gray-200 rounded-xl focus:outline-none focus:ring-2 focus:ring-purple-500 focus:border-transparent transition-all text-base bg-gray-50"
                    placeholder="No file selected..."
                    value={reservationsPath}
                    readOnly
                  />
                  <input
                    type="text"
                    className="w-full px-6 py-4 border-2 border-gray-200 rounded-xl focus:outline-none focus:ring-2 focus:ring-purple-500 focus:border-transparent transition-all text-base bg-white"
                    placeholder="Anchor date (YYYY-MM-DD)"
                    value={reservationsAnchor}
                    onChange={(e) => setReservationsAnchor(e.target.value)}
                  />
                  <div className="flex gap-4">
                    <button
                      className="flex-1 px-8 py-4 bg-gradient-to-r from-gray-100 to-gray-200 text-gray-700 rounded-xl hover:from-gray-200 hover:to-gray-300 transition-all duration-200 font-semibold shadow-md text-base"
                      onClick={() => pickFile([['Excel Files', ['xlsx', 'xls']]], setReservationsPath)}
                    >
                      Browse Files
                    </button>
                    <button
                      className="flex-1 px-8 py-4 bg-gradient-to-r from-purple-600 to-purple-700 text-white rounded-xl hover:from-purple-700 hover:to-purple-800 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-200 font-semibold shadow-xl text-base"
                      disabled={loading || !reservationsPath}
                      onClick={() => handleImport('reservations', reservationsPath, reservationsAnchor)}
                    >
                      {loading ? 'Importing...' : 'Import Reservations'}
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </div>

          {/* 4. Users */}
          <div className="bg-white rounded-2xl shadow-2xl p-10 hover:shadow-3xl transition-all duration-300">
            <div className="flex items-start gap-6">
              <div className="flex-shrink-0 w-16 h-16 bg-gradient-to-br from-amber-500 to-amber-600 rounded-2xl flex items-center justify-center text-white font-bold text-2xl shadow-xl">
                4
              </div>
              <div className="flex-1 space-y-6">
                <div>
                  <h3 className="text-2xl font-bold text-gray-900 mb-3">User Credentials (Optional)</h3>
                  <p className="text-gray-600 text-base">
                    Import additional users from userpass.md (default users already exist)
                  </p>
                </div>
                <div className="space-y-4">
                  <input
                    type="text"
                    className="w-full px-6 py-4 border-2 border-gray-200 rounded-xl focus:outline-none focus:ring-2 focus:ring-amber-500 focus:border-transparent transition-all text-base bg-gray-50"
                    placeholder="No file selected..."
                    value={usersPath}
                    readOnly
                  />
                  <div className="flex gap-4">
                    <button
                      className="flex-1 px-8 py-4 bg-gradient-to-r from-gray-100 to-gray-200 text-gray-700 rounded-xl hover:from-gray-200 hover:to-gray-300 transition-all duration-200 font-semibold shadow-md text-base"
                      onClick={() => pickFile([['Markdown Files', ['md']]], setUsersPath)}
                    >
                      Browse Files
                    </button>
                    <button
                      className="flex-1 px-8 py-4 bg-gradient-to-r from-amber-600 to-amber-700 text-white rounded-xl hover:from-amber-700 hover:to-amber-800 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-200 font-semibold shadow-xl text-base"
                      disabled={loading || !usersPath}
                      onClick={() => handleImport('users', usersPath)}
                    >
                      {loading ? 'Importing...' : 'Import Users'}
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* Instructions */}
        <div className="mt-10 bg-gradient-to-r from-blue-50 to-indigo-50 rounded-2xl shadow-2xl p-10 border-2 border-blue-200">
          <h2 className="text-3xl font-bold text-gray-900 mb-6">
            Import Order
          </h2>
          <ol className="space-y-5 text-gray-700 text-base">
            <li className="flex items-start gap-4">
              <span className="flex-shrink-0 w-8 h-8 bg-blue-600 text-white rounded-full flex items-center justify-center font-bold shadow-md">
                1
              </span>
              <span className="pt-1">
                Import <strong className="text-blue-700">Guests & Reservations</strong> first (creates guests,
                reservations, rooms)
              </span>
            </li>
            <li className="flex items-start gap-4">
              <span className="flex-shrink-0 w-8 h-8 bg-green-600 text-white rounded-full flex items-center justify-center font-bold shadow-md">
                2
              </span>
              <span className="pt-1">
                Import <strong className="text-green-700">Collections</strong> second (attaches payments to reservations)
              </span>
            </li>
            <li className="flex items-start gap-4">
              <span className="flex-shrink-0 w-8 h-8 bg-purple-600 text-white rounded-full flex items-center justify-center font-bold shadow-md">
                3
              </span>
              <span className="pt-1">
                Import <strong className="text-purple-700">Reservations workbook</strong> third (fills gaps, claims server
                rows)
              </span>
            </li>
            <li className="flex items-start gap-4">
              <span className="flex-shrink-0 w-8 h-8 bg-amber-600 text-white rounded-full flex items-center justify-center font-bold shadow-md">
                4
              </span>
              <span className="pt-1">
                Import <strong className="text-amber-700">Users</strong> last (optional, defaults already exist)
              </span>
            </li>
          </ol>
          <div className="mt-8 bg-white rounded-xl p-6 border-2 border-blue-200 shadow-md">
            <p className="text-gray-700 text-base leading-relaxed">
              <span className="text-2xl mr-2">✨</span>
              All imports are <strong>idempotent</strong> — re-running updates existing rows instead of duplicating. You
              can safely re-import files.
            </p>
          </div>
        </div>
      </div>
    </div>
  )
}
