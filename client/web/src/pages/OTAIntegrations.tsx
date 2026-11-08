import { useEffect, useState } from 'react'
import { api } from '../api/client'

interface OTAConfig {
  id: string
  provider: string
  enabled: boolean
  hotel_code: string | null
  base_url: string | null
  last_sync_at: string | null
  last_success_at: string | null
  status: 'disconnected' | 'connected' | 'syncing' | 'error'
  error_message: string | null
}

export default function OTAIntegrations() {
  const [configs, setConfigs] = useState<OTAConfig[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const loadConfigs = async () => {
    try {
      setLoading(true)
      setError(null)
      const response = await api.otaConfigs()
      setConfigs(response as OTAConfig[])
    } catch (e: any) {
      setError(e.message || 'Failed to load OTA configurations')
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    loadConfigs()
  }, [])

  const handleToggle = async (configId: string, currentEnabled: boolean) => {
    try {
      await api.updateOtaConfig(configId, { enabled: !currentEnabled })
      await loadConfigs()
    } catch (e: any) {
      alert('Failed to toggle: ' + (e.message || 'Unknown error'))
    }
  }

  const handleCreateConfig = async (_provider: 'booking_com' | 'expedia') => {
    alert('OTA setup is not enabled yet. The current server does not safely encrypt provider credentials or perform live synchronization. Do not enter Booking.com or Expedia credentials until certified adapters and secret storage are deployed.')
    return
    /* disabled until encrypted credential storage and certified API adapters are deployed
    const hotelCode = prompt(`Enter your ${provider === 'booking_com' ? 'Booking.com Hotel ID' : 'Expedia Property ID'}:`)
    if (!hotelCode) return

    const username = prompt(`Enter ${provider === 'booking_com' ? 'Machine Account Username' : 'API Key'}:`)
    if (!username) return

    const password = prompt(`Enter ${provider === 'booking_com' ? 'Password' : 'API Secret'}:`)
    if (!password) return

    try {
      const credentials: OTACredentials =
        provider === 'booking_com'
          ? { type: 'booking_com', username, password }
          : { type: 'expedia', api_key: username, api_secret: password }

      await api.createOtaConfig({
        provider,
        enabled: false,
        credentials,
        hotel_code: hotelCode,
      })
      await loadConfigs()
      alert(`${provider === 'booking_com' ? 'Booking.com' : 'Expedia'} configuration created successfully!`)
    } catch (e: any) {
      alert('Failed to create config: ' + (e.message || 'Unknown error'))
    }
    */
  }


  const bookingComConfig = configs.find((c) => c.provider === 'booking_com')
  const expediaConfig = configs.find((c) => c.provider === 'expedia')

  if (loading) {
    return (
      <div className="p-6">
        <div className="text-center py-8">Loading OTA configurations...</div>
      </div>
    )
  }

  return (
    <div className="p-6">
      <div className="mb-6">
        <h1 className="text-2xl font-semibold mb-2">OTA Channel Integrations</h1>
        <p className="text-gray-600">
          Booking.com and Expedia integrations are not active yet. This page explains the onboarding path; it does not connect to either provider or import reservations.
        </p>
        {error && (
          <div className="mt-3 p-3 bg-red-50 border border-red-200 rounded text-red-800">
            <strong>Error:</strong> {error}
          </div>
        )}
        <div className="mt-3 p-3 bg-yellow-50 border border-yellow-200 rounded text-sm text-yellow-800">
          <strong>Not connected:</strong> Do not enter provider secrets. Current test and sync controls are disabled; live access requires official partner approval, secure credential storage, certified API adapters, room mapping, and end-to-end verification.
        </div>
      </div>

      <div className="grid gap-6">
        {/* Booking.com */}
        <div className="bg-white rounded-lg shadow border border-gray-200 p-6">
          <div className="flex items-start justify-between mb-4">
            <div className="flex items-center gap-3">
              <div className="text-4xl">🏨</div>
              <div>
                <h2 className="text-xl font-semibold">Booking.com</h2>
                <p className="text-sm text-gray-500">OTA XML 2003B + B.XML</p>
              </div>
            </div>
            {bookingComConfig && (
              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  checked={bookingComConfig.enabled}
                  onChange={() => handleToggle(bookingComConfig.id, bookingComConfig.enabled)}
                  className="w-5 h-5"
                />
                <span className="text-sm font-medium">
                  {bookingComConfig.enabled ? 'Enabled' : 'Disabled'}
                </span>
              </label>
            )}
          </div>

          {bookingComConfig ? (
            <>
              <div className="grid grid-cols-3 gap-4 mb-4">
                <div>
                  <div className="text-sm font-medium text-gray-700 mb-1">Status</div>
                  <div className="flex items-center gap-2">
                    <span
                      className={`inline-block w-2 h-2 rounded-full ${
                        bookingComConfig.status === 'connected'
                          ? 'bg-green-500'
                          : bookingComConfig.status === 'syncing'
                          ? 'bg-yellow-500 animate-pulse'
                          : bookingComConfig.status === 'error'
                          ? 'bg-red-500'
                          : 'bg-gray-400'
                      }`}
                    />
                    <span className="text-sm capitalize">{bookingComConfig.status}</span>
                  </div>
                </div>
                <div>
                  <div className="text-sm font-medium text-gray-700 mb-1">Hotel ID</div>
                  <div className="text-sm text-gray-600">{bookingComConfig.hotel_code || 'Not set'}</div>
                </div>
                <div>
                  <div className="text-sm font-medium text-gray-700 mb-1">Last Sync</div>
                  <div className="text-sm text-gray-600">
                    {bookingComConfig.last_sync_at
                      ? new Date(bookingComConfig.last_sync_at).toLocaleString()
                      : 'Never'}
                  </div>
                </div>
              </div>

              {bookingComConfig.error_message && (
                <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded text-sm text-red-800">
                  <strong>Error:</strong> {bookingComConfig.error_message}
                </div>
              )}

              <p className="text-sm text-amber-800">Provider operations are disabled; this saved status is not evidence of a live connection.</p>

              <div className="mt-4 p-3 bg-gray-50 rounded text-xs">
                <strong>Endpoints:</strong>
                <ul className="mt-1 space-y-1 text-gray-700">
                  <li>• Base: https://supply-xml.booking.com (rates/availability)</li>
                  <li>• Secure: https://secure-supply-xml.booking.com (reservations)</li>
                  <li>• Protocol: OTA XML 2003B + B.XML</li>
                  <li>• Pull: GET /ota/OTA_HotelResNotif (new reservations)</li>
                  <li>• Pull: GET /ota/OTA_HotelResModifyNotif (modifications)</li>
                  <li>• Acknowledge: POST with reservation ID</li>
                </ul>
              </div>
            </>
          ) : (
            <div className="text-center py-4">
              <p className="text-gray-600 mb-3">Not configured</p>
              <button
                onClick={() => handleCreateConfig('booking_com')}
                className="px-4 py-2 bg-gray-400 text-white rounded cursor-not-allowed"
              >
                Setup unavailable — integration pending
              </button>
            </div>
          )}
        </div>

        {/* Expedia */}
        <div className="bg-white rounded-lg shadow border border-gray-200 p-6">
          <div className="flex items-start justify-between mb-4">
            <div className="flex items-center gap-3">
              <div className="text-4xl">✈️</div>
              <div>
                <h2 className="text-xl font-semibold">Expedia Group</h2>
                <p className="text-sm text-gray-500">GraphQL + REST APIs</p>
              </div>
            </div>
            {expediaConfig && (
              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  checked={expediaConfig.enabled}
                  onChange={() => handleToggle(expediaConfig.id, expediaConfig.enabled)}
                  className="w-5 h-5"
                />
                <span className="text-sm font-medium">
                  {expediaConfig.enabled ? 'Enabled' : 'Disabled'}
                </span>
              </label>
            )}
          </div>

          {expediaConfig ? (
            <>
              <div className="grid grid-cols-3 gap-4 mb-4">
                <div>
                  <div className="text-sm font-medium text-gray-700 mb-1">Status</div>
                  <div className="flex items-center gap-2">
                    <span
                      className={`inline-block w-2 h-2 rounded-full ${
                        expediaConfig.status === 'connected'
                          ? 'bg-green-500'
                          : expediaConfig.status === 'syncing'
                          ? 'bg-yellow-500 animate-pulse'
                          : expediaConfig.status === 'error'
                          ? 'bg-red-500'
                          : 'bg-gray-400'
                      }`}
                    />
                    <span className="text-sm capitalize">{expediaConfig.status}</span>
                  </div>
                </div>
                <div>
                  <div className="text-sm font-medium text-gray-700 mb-1">Property ID</div>
                  <div className="text-sm text-gray-600">{expediaConfig.hotel_code || 'Not set'}</div>
                </div>
                <div>
                  <div className="text-sm font-medium text-gray-700 mb-1">Last Sync</div>
                  <div className="text-sm text-gray-600">
                    {expediaConfig.last_sync_at
                      ? new Date(expediaConfig.last_sync_at).toLocaleString()
                      : 'Never'}
                  </div>
                </div>
              </div>

              {expediaConfig.error_message && (
                <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded text-sm text-red-800">
                  <strong>Error:</strong> {expediaConfig.error_message}
                </div>
              )}

              <p className="text-sm text-amber-800">Provider operations are disabled; this saved status is not evidence of a live connection.</p>

              <div className="mt-4 p-3 bg-gray-50 rounded text-xs">
                <strong>Endpoints:</strong>
                <ul className="mt-1 space-y-1 text-gray-700">
                  <li>• Base: https://api.expediagroup.com/supply/lodging</li>
                  <li>• Protocol: GraphQL + REST</li>
                  <li>• Query: reservationRetrieval (pull reservations)</li>
                  <li>• Mutation: confirmReservation, updateRates, updateAvailability</li>
                  <li>• Requires: PCI compliance for production</li>
                </ul>
              </div>
            </>
          ) : (
            <div className="text-center py-4">
              <p className="text-gray-600 mb-3">Not configured</p>
              <button
                onClick={() => handleCreateConfig('expedia')}
                className="px-4 py-2 bg-gray-400 text-white rounded cursor-not-allowed"
              >
                Setup unavailable — integration pending
              </button>
            </div>
          )}
        </div>
      </div>

      <div className="mt-8 p-6 bg-blue-50 border border-blue-200 rounded">
        <h2 className="text-lg font-semibold mb-3">🚀 Production Onboarding Steps</h2>
        <ol className="space-y-2 text-sm">
          <li className="flex items-start gap-2">
            <span className="font-bold">1.</span>
            <div>
              <strong>Apply for partnership:</strong> Contact Booking.com Connectivity Support or
              Expedia Partner Central
            </div>
          </li>
          <li className="flex items-start gap-2">
            <span className="font-bold">2.</span>
            <div>
              <strong>Sign agreements:</strong> NDA, license agreement, PCI compliance (Expedia)
            </div>
          </li>
          <li className="flex items-start gap-2">
            <span className="font-bold">3.</span>
            <div>
              <strong>Get test credentials:</strong> Sandbox environment for integration development
            </div>
          </li>
          <li className="flex items-start gap-2">
            <span className="font-bold">4.</span>
            <div>
              <strong>Develop & test:</strong> Implement reservation pull, rate push, error handling
            </div>
          </li>
          <li className="flex items-start gap-2">
            <span className="font-bold">5.</span>
            <div>
              <strong>Certification:</strong> Pass OTA validation tests (30+ scenarios)
            </div>
          </li>
          <li className="flex items-start gap-2">
            <span className="font-bold">6.</span>
            <div>
              <strong>Go live:</strong> Receive production credentials and activate properties
            </div>
          </li>
        </ol>
      </div>
    </div>
  )
}
