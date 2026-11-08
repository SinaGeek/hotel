import { useCallback, useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { api, ApiError } from '../api/client'
import { desktop, isDesktop } from '../api/desktop'
import type { HotelDto, UserDto } from '../types/generated/api'

interface RoomTypeConfig {
  name: string
  count: number
  price: number
}

interface RestaurantConfig {
  name: string
  capacity: number
  mealPlan: string
}

interface HotelExtendedProfile {
  totalRooms: number
  roomTypes: RoomTypeConfig[]
  parkingCapacity: number
  valetParking: boolean
  parkingType: 'free' | 'paid'
  /** Days treated as weekend for weekend fees: 0=Sunday .. 6=Saturday (JS getDay). */
  weekendDays: number[]
  restaurants: RestaurantConfig[]
  facilities: {
    pool: boolean
    gym: boolean
    spa: boolean
    wifi: boolean
    conference: boolean
    elevator: boolean
    roomService: boolean
  }
  contactPhone: string
  contactEmail: string
  address: string
}

const DEFAULT_PROFILE: HotelExtendedProfile = {
  totalRooms: 45,
  roomTypes: [
    { name: 'Standard Single', count: 15, price: 1200 },
    { name: 'Deluxe Double', count: 20, price: 1800 },
    { name: 'Executive Suite', count: 10, price: 3200 },
  ],
  parkingCapacity: 35,
  valetParking: true,
  parkingType: 'free',
  weekendDays: [6, 0], // Saturday + Sunday
  restaurants: [
    { name: 'Main Restaurant', capacity: 90, mealPlan: 'Open Buffet & A La Carte' },
    { name: 'Terrace Bistro', capacity: 40, mealPlan: 'Beverages & Snacks' },
  ],
  facilities: {
    pool: true,
    gym: true,
    spa: true,
    wifi: true,
    conference: true,
    elevator: true,
    roomService: true,
  },
  contactPhone: '+90 242 555 0199',
  contactEmail: 'info@hotel.com',
  address: 'Ataturk Cad. No: 42, Antalya',
}

export function Admin() {
  const navigate = useNavigate()
  const [hotels, setHotels] = useState<HotelDto[]>([])
  const [users, setUsers] = useState<UserDto[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  // 4- Filter row states for Hotels table
  const [hotelCodeFilter, setHotelCodeFilter] = useState('')
  const [hotelNameFilter, setHotelNameFilter] = useState('')
  const [hotelStatusFilter, setHotelStatusFilter] = useState<'all' | 'active' | 'disabled'>('all')

  // Create Hotel Modal
  const [showCreateHotel, setShowCreateHotel] = useState(false)
  const [newHotel, setNewHotel] = useState({ name: '', code: '' })

  // 3- Hotel Profile View
  const [selectedHotel, setSelectedHotel] = useState<HotelDto | null>(null)
  const [hotelCodeEdit, setHotelCodeEdit] = useState('')
  const [hotelNameEdit, setHotelNameEdit] = useState('')
  const [hotelProfile, setHotelProfile] = useState<HotelExtendedProfile>(DEFAULT_PROFILE)
  const [profileSaveSuccess, setProfileSaveSuccess] = useState(false)

  // Filter row states for Users table (inside hotel profile)
  const [userSearchFilter, setUserSearchFilter] = useState('')
  const [userRoleFilter, setUserRoleFilter] = useState('all')
  const [userStatusFilter, setUserStatusFilter] = useState<'all' | 'active' | 'disabled'>('all')

  // User creation & editing inside Hotel Profile
  const [showAddUserModal, setShowAddUserModal] = useState(false)
  const [newUser, setNewUser] = useState({
    username: '',
    full_name: '',
    role: 'reception',
    password: '',
  })
  const [editingUser, setEditingUser] = useState<UserDto | null>(null)
  const [domainEntity, setDomainEntity] = useState<'rooms' | 'room_types' | 'guests' | 'reservations' | 'channels' | 'restaurants'>('rooms')
  const [domainRecords, setDomainRecords] = useState<Array<Record<string, unknown>>>([])
  const [domainRecordId, setDomainRecordId] = useState('')
  const [domainRecordJson, setDomainRecordJson] = useState('{}')
  const [domainMessage, setDomainMessage] = useState<string | null>(null)
  const [editRoles, setEditRoles] = useState<string[]>([])
  // Room type cards + member rooms (desktop, backed by encrypted SQLite).
  const [typeRecords, setTypeRecords] = useState<Array<Record<string, unknown>>>([])
  const [roomRecords, setRoomRecords] = useState<Array<Record<string, unknown>>>([])
  const [ruleRecords, setRuleRecords] = useState<Array<Record<string, unknown>>>([])
  const [expandedType, setExpandedType] = useState<string | null>(null)
  const [roomDrafts, setRoomDrafts] = useState<Record<string, { number: string; name: string; floor: string }>>({})
  const [typeFeeDrafts, setTypeFeeDrafts] = useState<Record<string, { mid: string; weekend: string }>>({})
  const [ruleDraft, setRuleDraft] = useState<Record<string, { from: string; to: string; price: string }>>({})
  const [newType, setNewType] = useState({ name: '', fee: '', weekendFee: '' })
  const [newRoom, setNewRoom] = useState({ number: '', name: '' })
  const [editFullName, setEditFullName] = useState('')
  const [editPassword, setEditPassword] = useState('')

  const refresh = useCallback(async () => {
    try {
      setError(null)
      const hotelsPromise: Promise<HotelDto[]> = isDesktop
        ? desktop.listLocalHotels().then((rows) => rows.map((h) => ({
            id: Number(h.id), name: String(h.name), default_currency: String(h.default_currency || 'TRY'),
            time_zone: String(h.time_zone || 'Europe/Istanbul'), is_active: Boolean(h.is_active), next_change_seq: 0,
          }))).then((rows) => rows.length ? rows : [
            { id: 0, name: 'Platform (all hotels)', default_currency: 'TRY', time_zone: 'Europe/Istanbul', is_active: true, next_change_seq: 0 },
            { id: 4442, name: 'Hotel 4442', default_currency: 'TRY', time_zone: 'Europe/Istanbul', is_active: true, next_change_seq: 0 },
          ])
        : api.listHotels()
      const usersPromise: Promise<UserDto[]> = api.listUsers().catch(async () => {
        if (isDesktop) {
          const lus = await desktop.listLocalUsers().catch(() => [])
          return lus.map((u) => ({
            id: u.user_id,
            hotel_id: u.hotel_id,
            username: u.username,
            full_name: u.full_name || u.username,
            email: null,
            role: u.role,
            is_active: true,
            force_password_change: false,
          }))
        }
        return []
      })
      const [hotelsRes, usersRes] = await Promise.all([hotelsPromise, usersPromise])
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

  // Load hotel extended profile when a hotel is selected
  const openHotelProfile = (h: HotelDto) => {
    setSelectedHotel(h)
    setHotelCodeEdit(String(h.id))
    setHotelNameEdit(h.name)
    setProfileSaveSuccess(false)
    const loadProfile = async () => {
      try {
        const saved = isDesktop
          ? await desktop.getHotelProfile(h.id)
          : JSON.parse(localStorage.getItem(`hms.hotel_profile_${h.id}`) || 'null')
        if (saved) {
          setHotelProfile({ ...DEFAULT_PROFILE, ...(saved as Partial<HotelExtendedProfile>) })
        } else {
          setHotelProfile({ ...DEFAULT_PROFILE, address: `${h.name} Main Campus, Antalya` })
        }
      } catch {
        setHotelProfile(DEFAULT_PROFILE)
      }
    }
    void loadProfile()
  }

  const reloadDomainRecords = async (entity = domainEntity) => {
    if (!selectedHotel || !isDesktop) return
    try {
      setDomainRecords(await desktop.listDomainRecords(entity, selectedHotel.id))
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  useEffect(() => {
    if (selectedHotel && isDesktop) void reloadDomainRecords()
  }, [selectedHotel?.id, domainEntity])

  // ---- Room type cards + member rooms + calendar price overrides (desktop) ----
  const reloadRoomCatalog = useCallback(async () => {
    if (!selectedHotel || !isDesktop) return
    const hid = selectedHotel.id
    try {
      const [types, rms, rules] = await Promise.all([
        desktop.listDomainRecords('room_types', hid),
        desktop.listDomainRecords('rooms', hid),
        desktop.listDomainRecords('price_rules', hid),
      ])
      setTypeRecords(types)
      setRoomRecords(rms)
      setRuleRecords(rules)
      setRoomDrafts(Object.fromEntries(rms.map((r) => [String(r.id), {
        number: String(r.room_number ?? ''), name: String(r.name ?? ''), floor: String(r.floor ?? 0),
      }])))
      setTypeFeeDrafts(Object.fromEntries(types.map((t) => [String(t.id), {
        mid: String((Number(t.base_price_cents) || 0) / 100),
        weekend: String((Number(t.weekend_price_cents) || 0) / 100),
      }])))
      setExpandedType((prev) => (prev && types.some((t) => String(t.id) === prev) ? prev : null))
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }, [selectedHotel?.id])

  useEffect(() => { void reloadRoomCatalog() }, [reloadRoomCatalog])

  const saveTypeCard = async (t: Record<string, unknown>) => {
    if (!selectedHotel) return
    const d = typeFeeDrafts[String(t.id)] ?? { mid: '0', weekend: '' }
    try {
      await desktop.saveDomainRecord('room_types', selectedHotel.id, String(t.id), {
        ...t,
        base_price_cents: Math.round(Number(d.mid || 0) * 100),
        weekend_price_cents: Math.round(Number(d.weekend || 0) * 100),
      })
      await reloadRoomCatalog()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const addTypeCard = async () => {
    if (!selectedHotel || !newType.name.trim()) return
    try {
      await desktop.saveDomainRecord('room_types', selectedHotel.id, crypto.randomUUID(), {
        name: newType.name.trim(),
        capacity: 2,
        base_price_cents: Math.round(Number(newType.fee || 0) * 100),
        weekend_price_cents: Math.round(Number(newType.weekendFee || 0) * 100),
      })
      setNewType({ name: '', fee: '', weekendFee: '' })
      await reloadRoomCatalog()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const restoreRoomRow = async (r: Record<string, unknown>) => {
    if (!selectedHotel) return
    try {
      await desktop.restoreDomainRecord('rooms', selectedHotel.id, String(r.id))
      await reloadRoomCatalog()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const restoreDeletedRooms = async (typeId: string) => {
    if (!selectedHotel) return
    try {
      const all = await desktop.listDomainRecords('rooms', selectedHotel.id)
      for (const r of all.filter((x) => x.deleted_at != null && String(x.room_type_id) === typeId)) {
        await desktop.restoreDomainRecord('rooms', selectedHotel.id, String(r.id))
      }
      await reloadRoomCatalog()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const saveRoomRow = async (r: Record<string, unknown>) => {
    if (!selectedHotel) return
    const d = roomDrafts[String(r.id)] ?? { number: String(r.room_number ?? ''), name: String(r.name ?? ''), floor: '0' }
    if (!d.number.trim()) return
    try {
      await desktop.saveDomainRecord('rooms', selectedHotel.id, String(r.id), {
        ...r, room_number: d.number.trim(), name: d.name.trim(), floor: Number(d.floor || 0),
      })
      await reloadRoomCatalog()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const addRoomToType = async (typeId: string) => {
    if (!selectedHotel || !newRoom.number.trim()) return
    try {
      await desktop.saveDomainRecord('rooms', selectedHotel.id, crypto.randomUUID(), {
        room_type_id: typeId, room_number: newRoom.number.trim(),
        name: newRoom.name.trim() || newRoom.number.trim(), floor: 0, cleanliness: 'clean',
      })
      setNewRoom({ number: '', name: '' })
      await reloadRoomCatalog()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const deleteTypeCard = async (typeId: string) => {
    if (!selectedHotel) return
    try {
      await desktop.deleteDomainRecord('room_types', selectedHotel.id, typeId)
      if (expandedType === typeId) setExpandedType(null)
      await reloadRoomCatalog()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const addPriceRule = async (typeId: string) => {
    if (!selectedHotel) return
    const d = ruleDraft[typeId]
    if (!d || !d.from || !d.to) return
    try {
      await desktop.saveDomainRecord('price_rules', selectedHotel.id, crypto.randomUUID(), {
        room_type_id: typeId, date_from: d.from, date_to: d.to,
        price_cents: Math.round(Number(d.price || 0) * 100), currency: 'TRY',
      })
      setRuleDraft((prev) => ({ ...prev, [typeId]: { from: '', to: '', price: '' } }))
      await reloadRoomCatalog()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const deletePriceRule = async (ruleId: string) => {
    if (!selectedHotel) return
    try {
      await desktop.deleteDomainRecord('price_rules', selectedHotel.id, ruleId)
      await reloadRoomCatalog()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const saveHotelProfile = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!selectedHotel) return
    try {
      const newCode = Number(hotelCodeEdit)
      if (!hotelNameEdit.trim() || !Number.isInteger(newCode) || newCode < 0) {
        throw new Error('Enter a valid hotel name and numeric code.')
      }
      if (isDesktop) {
        await desktop.saveLocalHotel(selectedHotel.id, newCode, String(newCode), hotelNameEdit.trim(), hotelProfile)
      } else {
        await api.updateHotel(selectedHotel.id, { id: newCode, name: hotelNameEdit.trim() })
        localStorage.setItem(`hms.hotel_profile_${selectedHotel.id}`, JSON.stringify(hotelProfile))
      }
      const updated = { ...selectedHotel, id: newCode, name: hotelNameEdit.trim() }
      setSelectedHotel(updated)
      setHotels((prev) => prev.map((h) => h.id === selectedHotel.id ? updated : h))
      setProfileSaveSuccess(true)
      setTimeout(() => setProfileSaveSuccess(false), 3000)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  async function createHotel() {
    const code = Number(newHotel.code)
    if (!newHotel.name.trim() || !Number.isInteger(code) || code <= 0) {
      setError('Enter a hotel name and a positive numeric hotel code.')
      return
    }
    setBusy(true)
    try {
      setError(null)
      await api.createHotel({ id: code, name: newHotel.name.trim() })
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

  async function toggleHotel(id: number, active: boolean) {
    try {
      await api.updateHotel(id, { is_active: !active })
      await refresh()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    }
  }

  async function toggleUser(id: string, active: boolean) {
    try {
      await api.updateUser(id, { is_active: !active })
      await refresh()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    }
  }

  async function createUserForSelectedHotel(e: React.FormEvent) {
    e.preventDefault()
    if (!selectedHotel) return
    setBusy(true)
    try {
      await api.createUser({
        hotel_id: selectedHotel.id,
        username: newUser.username.trim(),
        full_name: newUser.full_name.trim() || newUser.username.trim(),
        role: newUser.role,
        password: newUser.password,
      })
      setNewUser({ username: '', full_name: '', role: 'reception', password: '' })
      setShowAddUserModal(false)
      await refresh()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  function startEditUser(u: UserDto) {
    setEditingUser(u)
    setEditFullName(u.full_name || '')
    setEditRoles(u.role.split(',').map((r) => r.trim()))
    setEditPassword('')
  }

  async function saveEditUser(e: React.FormEvent) {
    e.preventDefault()
    if (!editingUser) return
    if (editRoles.length === 0) {
      setError('User must have at least one role')
      return
    }
    setBusy(true)
    try {
      await api.updateUser(editingUser.id, {
        full_name: editFullName,
        role: editRoles.join(','),
        ...(editPassword ? { password: editPassword } : {}),
      })
      setEditingUser(null)
      await refresh()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  // 4- Filter and limit hotels to 10
  const filteredHotels = hotels
    .filter((h) => {
      const matchCode = !hotelCodeFilter || String(h.id).includes(hotelCodeFilter.trim())
      const matchName = !hotelNameFilter || h.name.toLowerCase().includes(hotelNameFilter.toLowerCase().trim())
      const matchStatus =
        hotelStatusFilter === 'all' ||
        (hotelStatusFilter === 'active' && h.is_active) ||
        (hotelStatusFilter === 'disabled' && !h.is_active)
      return matchCode && matchName && matchStatus
    })
    .slice(0, 10)

  // Filter users inside selected hotel
  const hotelUsers = users.filter((u) => {
    if (!selectedHotel) return false
    if (u.hotel_id !== selectedHotel.id) return false

    const search = userSearchFilter.toLowerCase().trim()
    const matchSearch = !search || u.username.toLowerCase().includes(search) || (u.full_name || '').toLowerCase().includes(search)
    const matchRole = userRoleFilter === 'all' || u.role.toLowerCase().includes(userRoleFilter.toLowerCase())
    const matchStatus =
      userStatusFilter === 'all' ||
      (userStatusFilter === 'active' && u.is_active) ||
      (userStatusFilter === 'disabled' && !u.is_active)

    return matchSearch && matchRole && matchStatus
  })

  return (
    <div className="page" style={{ padding: 24, maxWidth: 1100, margin: '0 auto' }}>
      {/* Header */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 20 }}>
        <div>
          <h1 style={{ margin: 0, fontSize: 26, fontWeight: 700 }}>Platform Administration</h1>
          <p style={{ color: '#64748b', margin: '4px 0 0 0', fontSize: 14 }}>
            {selectedHotel
              ? `Hotel Profile & Management: ${selectedHotel.name} (Code: ${selectedHotel.id})`
              : 'Manage registered hotels, properties, facilities, and staff.'}
          </p>
        </div>
        <div style={{ display: 'flex', gap: 10 }}>
          {selectedHotel ? (
            <button
              type="button"
              className="ghost"
              style={{ padding: '8px 16px', border: '1px solid #cbd5e1', borderRadius: 6, cursor: 'pointer', fontWeight: 600 }}
              onClick={() => setSelectedHotel(null)}
            >
              Back to Hotels List
            </button>
          ) : (
            <button
              type="button"
              className="ghost"
              style={{ padding: '8px 16px', border: '1px solid #cbd5e1', borderRadius: 6, cursor: 'pointer' }}
              onClick={() => navigate(-1)}
            >
              Back
            </button>
          )}
        </div>
      </div>

      {error && (
        <div style={{ padding: 12, backgroundColor: '#fef2f2', color: '#dc2626', borderRadius: 8, marginBottom: 16 }}>
          {error}
        </div>
      )}

      {loading ? (
        <p>Loading platform data...</p>
      ) : selectedHotel ? (
        /* ================= 3- HOTEL PROFILE VIEW ================= */
        <div style={{ display: 'grid', gap: 28 }}>
          {/* Hotel Profile Form */}
          <form
            onSubmit={saveHotelProfile}
            className="card"
            style={{ padding: 24, backgroundColor: 'white', borderRadius: 12, display: 'grid', gap: 20, border: '1px solid #e2e8f0' }}
          >
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', borderBottom: '1px solid #f1f5f9', paddingBottom: 14 }}>
              <div>
                <h2 style={{ margin: 0, fontSize: 20 }}>Properties & Facilities: {hotelNameEdit || selectedHotel.name}</h2>
                <span style={{ fontSize: 13, color: '#64748b' }}>Hotel Code: {hotelCodeEdit || selectedHotel.id}</span>
              </div>
              <button className="primary" type="submit" style={{ padding: '8px 20px', fontWeight: 600 }}>
                Save Hotel Profile
              </button>
            </div>

            {profileSaveSuccess && (
              <div style={{ padding: 10, backgroundColor: '#dcfce7', color: '#15803d', borderRadius: 6, fontWeight: 600 }}>
                Hotel profile & properties saved successfully!
              </div>
            )}

            {/* Basic Info */}
            <div>
              <h3 style={{ fontSize: 16, marginBottom: 10, color: '#334155' }}>Basic Information</h3>
              <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))', gap: 12 }}>
                <div>
                  <label style={{ display: 'block', fontSize: 12, fontWeight: 600, marginBottom: 4 }}>Hotel Name</label>
                  <input
                    value={hotelNameEdit}
                    onChange={(e) => setHotelNameEdit(e.target.value)}
                    style={{ width: '100%', padding: 8 }}
                  />
                </div>
                <div>
                  <label style={{ display: 'block', fontSize: 12, fontWeight: 600, marginBottom: 4 }}>Hotel Code</label>
                  <input
                    value={hotelCodeEdit}
                    onChange={(e) => setHotelCodeEdit(e.target.value)}
                    inputMode="numeric"
                    style={{ width: '100%', padding: 8 }}
                  />
                </div>
                <div>
                  <label style={{ display: 'block', fontSize: 12, fontWeight: 600, marginBottom: 4 }}>Contact Phone</label>
                  <input
                    value={hotelProfile.contactPhone}
                    onChange={(e) => setHotelProfile({ ...hotelProfile, contactPhone: e.target.value })}
                    style={{ width: '100%', padding: 8 }}
                  />
                </div>
                <div>
                  <label style={{ display: 'block', fontSize: 12, fontWeight: 600, marginBottom: 4 }}>Contact Email</label>
                  <input
                    value={hotelProfile.contactEmail}
                    onChange={(e) => setHotelProfile({ ...hotelProfile, contactEmail: e.target.value })}
                    style={{ width: '100%', padding: 8 }}
                  />
                </div>
                <div>
                  <label style={{ display: 'block', fontSize: 12, fontWeight: 600, marginBottom: 4 }}>Address / Location</label>
                  <input
                    value={hotelProfile.address}
                    onChange={(e) => setHotelProfile({ ...hotelProfile, address: e.target.value })}
                    style={{ width: '100%', padding: 8 }}
                  />
                </div>
              </div>
              {/* Weekend definition: which nights get the weekend fee */}
              <div style={{ marginTop: 16 }}>
                <label style={{ display: 'block', fontSize: 12, fontWeight: 600, marginBottom: 6 }}>
                  Weekend Days <span style={{ fontWeight: 400, color: '#64748b' }}>(nights on these days use the weekend fee)</span>
                </label>
                <div style={{ display: 'flex', gap: 14, flexWrap: 'wrap' }}>
                  {['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'].map((day, idx) => {
                    const checked = (hotelProfile.weekendDays ?? []).includes(idx)
                    return (
                      <label key={day} style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 13, cursor: 'pointer' }}>
                        <input
                          type="checkbox"
                          checked={checked}
                          onChange={(e) => {
                            const current = new Set(hotelProfile.weekendDays ?? [])
                            if (e.target.checked) current.add(idx)
                            else current.delete(idx)
                            setHotelProfile({ ...hotelProfile, weekendDays: [...current].sort() })
                          }}
                        />
                        {day}
                      </label>
                    )
                  })}
                </div>
              </div>
            </div>

            {/* Rooms Configuration — desktop uses real SQLite-backed cards */}
            <div>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 10 }}>
                <h3 style={{ fontSize: 16, margin: 0, color: '#334155' }}>Room Inventory & Types</h3>
                <span style={{ fontSize: 13, fontWeight: 600, color: '#2563eb' }}>
                  Total Rooms: {isDesktop ? roomRecords.length : hotelProfile.roomTypes.reduce((acc, r) => acc + Number(r.count || 0), 0)}
                </span>
              </div>

              {isDesktop ? (
                <div style={{ display: 'grid', gap: 14 }}>
                  {/* New room type form */}
                  <div style={{ display: 'grid', gridTemplateColumns: '2fr 1fr 1fr auto', gap: 10, alignItems: 'center' }}>
                    <input
                      placeholder="New Room Type Name"
                      value={newType.name}
                      onChange={(e) => setNewType({ ...newType, name: e.target.value })}
                      style={{ padding: 8 }}
                    />
                    <input
                      type="number"
                      placeholder="Mid Fee (TRY)"
                      value={newType.fee}
                      onChange={(e) => setNewType({ ...newType, fee: e.target.value })}
                      style={{ padding: 8 }}
                    />
                    <input
                      type="number"
                      placeholder="Weekend Fee (TRY)"
                      value={newType.weekendFee}
                      onChange={(e) => setNewType({ ...newType, weekendFee: e.target.value })}
                      style={{ padding: 8 }}
                    />
                    <button type="button" className="ghost" style={{ padding: '7px 14px', border: '1px dashed #cbd5e1', borderRadius: 6, fontSize: 13 }} onClick={() => void addTypeCard()}>
                      + Add Room Type
                    </button>
                  </div>

                  {/* Summary cards: one per room type */}
                  <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(220px, 1fr))', gap: 12 }}>
                    {typeRecords.map((t) => {
                      const tid = String(t.id)
                      const roomsOfType = roomRecords.filter((r) => String(r.room_type_id) === tid)
                      const weekend = Number(t.weekend_price_cents) || 0
                      return (
                        <div
                          key={tid}
                          onClick={() => setExpandedType(expandedType === tid ? null : tid)}
                          style={{
                            border: expandedType === tid ? '2px solid #2563eb' : '1px solid #e2e8f0',
                            borderRadius: 10, padding: 14, cursor: 'pointer',
                            background: expandedType === tid ? '#eff6ff' : 'white',
                            boxShadow: '0 1px 3px rgba(15,23,42,0.06)',
                          }}
                        >
                          <div style={{ fontWeight: 700, fontSize: 14, color: '#1e293b' }}>{String(t.name)}</div>
                          <div style={{ fontSize: 12, color: '#475569', marginTop: 6 }}>
                            {roomsOfType.filter((x) => x.deleted_at == null).length} room(s) in this type
                          </div>
                          <div style={{ fontSize: 12, color: '#475569' }}>Mid fee: {(Number(t.base_price_cents) || 0) / 100} TRY</div>
                          <div style={{ fontSize: 12, color: '#475569' }}>Weekend fee: {weekend ? `${weekend / 100} TRY` : 'same as mid fee'}</div>
                          <div style={{ fontSize: 11, color: '#94a3b8', marginTop: 6 }}>Click to manage rooms, fees and calendar prices</div>
                        </div>
                      )
                    })}
                    {typeRecords.length === 0 && (
                      <span style={{ color: '#94a3b8', fontSize: 13 }}>No room types yet. Add the first room type above.</span>
                    )}
                  </div>

                  {/* Expanded detail for the selected room type */}
                  {expandedType && typeRecords.filter((t) => String(t.id) === expandedType).map((t) => {
                    const tid = String(t.id)
                    const roomsOfType = roomRecords.filter((r) => String(r.room_type_id) === tid)
                    const rules = ruleRecords.filter((r) => String(r.room_type_id) === tid)
                    const feeDraft = typeFeeDrafts[tid] ?? { mid: '0', weekend: '' }
                    const ruleForm = ruleDraft[tid] ?? { from: '', to: '', price: '' }
                    return (
                      <div key={tid} style={{ border: '1px solid #cbd5e1', borderRadius: 10, padding: 16, background: '#f8fafc', display: 'grid', gap: 16 }}>
                        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                          <h4 style={{ margin: 0, fontSize: 15, color: '#1e293b' }}>{String(t.name)} — details</h4>
                          <button
                            type="button"
                            style={{ padding: '4px 10px', backgroundColor: '#fee2e2', color: '#b91c1c', border: 'none', borderRadius: 4, cursor: 'pointer', fontSize: 12 }}
                            onClick={() => void deleteTypeCard(tid)}
                          >
                            Remove Type
                          </button>
                        </div>

                        {/* Fees: mid + weekend, save together */}
                        <div style={{ display: 'grid', gap: 8 }}>
                          <b style={{ fontSize: 13, color: '#334155' }}>Fees</b>
                          <div style={{ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
                            <label style={{ fontSize: 12 }}>
                              Mid (normal) fee
                              <input
                                type="number" value={feeDraft.mid}
                                onChange={(e) => setTypeFeeDrafts((p) => ({ ...p, [tid]: { ...feeDraft, mid: e.target.value } }))}
                                style={{ width: 110, padding: 6, marginLeft: 6 }}
                              />
                              <span style={{ marginLeft: 4, color: '#64748b' }}>TRY</span>
                            </label>
                            <label style={{ fontSize: 12 }}>
                              Weekend fee
                              <input
                                type="number" placeholder="= mid" value={feeDraft.weekend}
                                onChange={(e) => setTypeFeeDrafts((p) => ({ ...p, [tid]: { ...feeDraft, weekend: e.target.value } }))}
                                style={{ width: 110, padding: 6, marginLeft: 6 }}
                              />
                              <span style={{ marginLeft: 4, color: '#64748b' }}>TRY</span>
                            </label>
                            <button type="button" className="primary" style={{ padding: '6px 14px' }} onClick={() => void saveTypeCard(t)}>
                              Save Fees
                            </button>
                          </div>
                          <small style={{ color: '#64748b', fontSize: 11 }}>
                            Weekend fee applies to nights falling on: {(hotelProfile.weekendDays ?? [6, 0]).map((d) => ['Sunday','Monday','Tuesday','Wednesday','Thursday','Friday','Saturday'][d]).join(', ')} (set under Basic Information). Blank weekend fee = same as mid fee.
                          </small>
                        </div>

                        {/* Calendar price overrides: single-day or batch date range */}
                        <div style={{ display: 'grid', gap: 8 }}>
                          <b style={{ fontSize: 13, color: '#334155' }}>Calendar Price Overrides</b>
                          <div style={{ display: 'grid', gap: 4 }}>
                            {rules.map((r) => (
                              <div key={String(r.id)} style={{ display: 'flex', gap: 10, alignItems: 'center', fontSize: 12, padding: '4px 0', borderBottom: '1px solid #e2e8f0' }}>
                                <span style={{ minWidth: 210 }}>{String(r.date_from)} &rarr; {String(r.date_to)}</span>
                                <span>{(Number(r.price_cents) || 0) / 100} TRY</span>
                                <span style={{ color: '#94a3b8' }}>{String(r.date_from) === String(r.date_to) ? '(single night)' : '(batch range)'}</span>
                                <button type="button" className="ghost" style={{ marginLeft: 'auto', padding: '2px 8px', fontSize: 12 }} onClick={() => void deletePriceRule(String(r.id))}>
                                  Remove
                                </button>
                              </div>
                            ))}
                            {rules.length === 0 && <span style={{ color: '#94a3b8', fontSize: 12 }}>No overrides — these nights use the fees above.</span>}
                          </div>
                          <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
                            <input type="date" value={ruleForm.from} onChange={(e) => setRuleDraft((p) => ({ ...p, [tid]: { ...ruleForm, from: e.target.value } }))} style={{ padding: 6 }} />
                            <input type="date" value={ruleForm.to} onChange={(e) => setRuleDraft((p) => ({ ...p, [tid]: { ...ruleForm, to: e.target.value } }))} style={{ padding: 6 }} />
                            <input type="number" placeholder="Price (TRY) / night" value={ruleForm.price} onChange={(e) => setRuleDraft((p) => ({ ...p, [tid]: { ...ruleForm, price: e.target.value } }))} style={{ width: 140, padding: 6 }} />
                            <button type="button" className="ghost" style={{ padding: '6px 14px' }} onClick={() => void addPriceRule(tid)}>
                              Add Override
                            </button>
                          </div>
                          <small style={{ color: '#64748b', fontSize: 11 }}>
                            Same date in both fields = single-night/day-by-day price. A date range = batch change for that period. Overrides beat weekend and mid fees.
                          </small>
                        </div>

                        {/* Rooms in this type: each room has its own name */}
                        <div style={{ display: 'grid', gap: 8 }}>
                          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                            <b style={{ fontSize: 13, color: '#334155' }}>Rooms in this type ({roomsOfType.filter((r) => r.deleted_at == null).length})</b>
                            {roomsOfType.some((r) => r.deleted_at != null) && (
                              <button type="button" className="ghost" style={{ padding: '4px 12px', fontSize: 12 }} onClick={() => void restoreDeletedRooms(tid)}>
                                Restore deleted ({roomsOfType.filter((r) => r.deleted_at != null).length})
                              </button>
                            )}
                          </div>
                          {roomsOfType.map((r) => {
                            const rid = String(r.id)
                            const draft = roomDrafts[rid] ?? { number: String(r.room_number ?? ''), name: String(r.name ?? ''), floor: String(r.floor ?? 0) }
                            const deleted = r.deleted_at != null
                            return (
                              <div
                                key={rid}
                                style={{
                                  display: 'flex', gap: 8, alignItems: 'center',
                                  opacity: deleted ? 0.45 : 1, borderLeft: deleted ? '3px solid #dc2626' : 'none', paddingLeft: deleted ? 6 : 0,
                                }}
                              >
                                <input
                                  value={draft.number}
                                  disabled={deleted}
                                  onChange={(e) => setRoomDrafts((p) => ({ ...p, [rid]: { ...draft, number: e.target.value } }))}
                                  placeholder="Room number"
                                  style={{ width: 110, padding: 6 }}
                                />
                                <span style={{ fontSize: 12, color: '#94a3b8' }}>#</span>
                                <input
                                  value={draft.name}
                                  disabled={deleted}
                                  onChange={(e) => setRoomDrafts((p) => ({ ...p, [rid]: { ...draft, name: e.target.value } }))}
                                  placeholder="Room name (e.g. Double with Balcony)"
                                  style={{ flex: 1, padding: 6 }}
                                />
                                <input
                                  type="number" value={draft.floor}
                                  disabled={deleted}
                                  onChange={(e) => setRoomDrafts((p) => ({ ...p, [rid]: { ...draft, floor: e.target.value } }))}
                                  placeholder="Floor"
                                  style={{ width: 80, padding: 6 }}
                                />
                                {deleted ? (
                                  <button type="button" className="ghost" style={{ padding: '5px 12px' }} onClick={() => void restoreRoomRow(r)}>
                                    Restore
                                  </button>
                                ) : (
                                  <button type="button" className="ghost" style={{ padding: '5px 12px' }} onClick={() => void saveRoomRow(r)}>
                                    Save
                                  </button>
                                )}
                              </div>
                            )
                          })}
                          <div style={{ display: 'flex', gap: 8, alignItems: 'center', borderTop: '1px dashed #cbd5e1', paddingTop: 8 }}>
                            <input
                              value={newRoom.number}
                              onChange={(e) => setNewRoom({ ...newRoom, number: e.target.value })}
                              placeholder="New room number"
                              style={{ width: 110, padding: 6 }}
                            />
                            <input
                              value={newRoom.name}
                              onChange={(e) => setNewRoom({ ...newRoom, name: e.target.value })}
                              placeholder="New room name"
                              style={{ flex: 1, padding: 6 }}
                            />
                            <button type="button" className="primary" style={{ padding: '6px 14px' }} onClick={() => void addRoomToType(tid)}>
                              Add Room
                            </button>
                          </div>
                          <small style={{ color: '#64748b', fontSize: 11 }}>
                            Example: type "Standard Double Bed Room" can contain rooms 101, 201, 301, 401, 205 — each with its own name.
                          </small>
                        </div>
                      </div>
                    )
                  })}
                </div>
              ) : (
                <div style={{ display: 'grid', gap: 10 }}>
                  {hotelProfile.roomTypes.map((rt, idx) => (
                    <div key={idx} style={{ display: 'grid', gridTemplateColumns: '2fr 1fr 1fr auto', gap: 10, alignItems: 'center' }}>
                      <input
                        placeholder="Room Type Name"
                        value={rt.name}
                        onChange={(e) => {
                          const copy = [...hotelProfile.roomTypes]
                          copy[idx].name = e.target.value
                          setHotelProfile({ ...hotelProfile, roomTypes: copy })
                        }}
                        style={{ padding: 8 }}
                      />
                      <input
                        type="number"
                        placeholder="Room Count"
                        value={rt.count}
                        onChange={(e) => {
                          const copy = [...hotelProfile.roomTypes]
                          copy[idx].count = Number(e.target.value)
                          setHotelProfile({ ...hotelProfile, roomTypes: copy })
                        }}
                        style={{ padding: 8 }}
                      />
                      <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                        <input
                          type="number"
                          placeholder="Nightly Rate"
                          value={rt.price}
                          onChange={(e) => {
                            const copy = [...hotelProfile.roomTypes]
                            copy[idx].price = Number(e.target.value)
                            setHotelProfile({ ...hotelProfile, roomTypes: copy })
                          }}
                          style={{ padding: 8, width: '100%' }}
                        />
                        <span style={{ fontSize: 12, color: '#64748b' }}>TRY</span>
                      </div>
                      <button
                        type="button"
                        style={{ padding: '6px 10px', backgroundColor: '#fee2e2', color: '#b91c1c', border: 'none', borderRadius: 4, cursor: 'pointer' }}
                        onClick={() => {
                          const copy = hotelProfile.roomTypes.filter((_, i) => i !== idx)
                          setHotelProfile({ ...hotelProfile, roomTypes: copy })
                        }}
                      >
                        Remove
                      </button>
                    </div>
                  ))}
                  <button
                    type="button"
                    className="ghost"
                    style={{ width: 'fit-content', padding: '6px 14px', border: '1px dashed #cbd5e1', borderRadius: 6, fontSize: 13 }}
                    onClick={() => {
                      setHotelProfile({
                        ...hotelProfile,
                        roomTypes: [...hotelProfile.roomTypes, { name: 'New Room Type', count: 10, price: 1500 }],
                      })
                    }}
                  >
                    + Add Room Type
                  </button>
                </div>
              )}
            </div>

            {/* Parking & Restaurants */}
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 20 }}>
              {/* Parking */}
              <div style={{ border: '1px solid #f1f5f9', padding: 14, borderRadius: 8 }}>
                <h3 style={{ fontSize: 15, margin: '0 0 10px 0', color: '#334155' }}>Parking Facilities</h3>
                <div style={{ display: 'grid', gap: 10 }}>
                  <div>
                    <label style={{ display: 'block', fontSize: 12, fontWeight: 600, marginBottom: 4 }}>Parking Capacity (Vehicles)</label>
                    <input
                      type="number"
                      value={hotelProfile.parkingCapacity}
                      onChange={(e) => setHotelProfile({ ...hotelProfile, parkingCapacity: Number(e.target.value) })}
                      style={{ width: '100%', padding: 8 }}
                    />
                  </div>
                  <div style={{ display: 'flex', gap: 16 }}>
                    <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 13, cursor: 'pointer' }}>
                      <input
                        type="checkbox"
                        checked={hotelProfile.valetParking}
                        onChange={(e) => setHotelProfile({ ...hotelProfile, valetParking: e.target.checked })}
                      />
                      Valet Parking Available
                    </label>
                    <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 13, cursor: 'pointer' }}>
                      <input
                        type="radio"
                        name="parkingPricing"
                        checked={hotelProfile.parkingType === 'free'}
                        onChange={() => setHotelProfile({ ...hotelProfile, parkingType: 'free' })}
                      />
                      Free for Guests
                    </label>
                    <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 13, cursor: 'pointer' }}>
                      <input
                        type="radio"
                        name="parkingPricing"
                        checked={hotelProfile.parkingType === 'paid'}
                        onChange={() => setHotelProfile({ ...hotelProfile, parkingType: 'paid' })}
                      />
                      Paid Parking
                    </label>
                  </div>
                </div>
              </div>

              {/* Restaurants */}
              <div style={{ border: '1px solid #f1f5f9', padding: 14, borderRadius: 8 }}>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 10 }}>
                  <div>
                  <h3 style={{ fontSize: 15, margin: 0, color: '#334155' }}>Restaurants & Dining</h3>
                  <small style={{ color: '#64748b', fontSize: 11 }}>Restaurant-specific specifications and cash flow will be designed later.</small>
                </div>
                  <button
                    type="button"
                    className="ghost"
                    style={{ padding: '2px 8px', fontSize: 12 }}
                    onClick={() => {
                      setHotelProfile({
                        ...hotelProfile,
                        restaurants: [...hotelProfile.restaurants, { name: 'Restaurant / Cafe', capacity: 50, mealPlan: 'All Day Dining' }],
                      })
                    }}
                  >
                    + Add
                  </button>
                </div>
                <div style={{ display: 'grid', gap: 8, maxHeight: 180, overflowY: 'auto' }}>
                  {hotelProfile.restaurants.map((rest, rIdx) => (
                    <div key={rIdx} style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
                      <input
                        placeholder="Name"
                        value={rest.name}
                        onChange={(e) => {
                          const copy = [...hotelProfile.restaurants]
                          copy[rIdx].name = e.target.value
                          setHotelProfile({ ...hotelProfile, restaurants: copy })
                        }}
                        style={{ flex: 2, padding: 6, fontSize: 13 }}
                      />
                      <input
                        type="number"
                        placeholder="Seats"
                        value={rest.capacity}
                        onChange={(e) => {
                          const copy = [...hotelProfile.restaurants]
                          copy[rIdx].capacity = Number(e.target.value)
                          setHotelProfile({ ...hotelProfile, restaurants: copy })
                        }}
                        style={{ width: 65, padding: 6, fontSize: 13 }}
                      />
                      <button
                        type="button"
                        style={{ border: 'none', background: '#fee2e2', color: '#b91c1c', borderRadius: 4, padding: '4px 8px', cursor: 'pointer' }}
                        onClick={() => {
                          const copy = hotelProfile.restaurants.filter((_, i) => i !== rIdx)
                          setHotelProfile({ ...hotelProfile, restaurants: copy })
                        }}
                      >
                        x
                      </button>
                    </div>
                  ))}
                </div>
              </div>
            </div>

            {/* Amenities & Facilities Checkboxes */}
            <div>
              <h3 style={{ fontSize: 15, marginBottom: 8, color: '#334155' }}>Property Utilities & Amenities</h3>
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 16 }}>
                {[
                  { key: 'pool', label: 'Swimming Pool' },
                  { key: 'gym', label: 'Fitness Center / Gym' },
                  { key: 'spa', label: 'Spa & Sauna' },
                  { key: 'wifi', label: 'High-Speed Wi-Fi' },
                  { key: 'conference', label: 'Meeting & Conference Rooms' },
                  { key: 'elevator', label: 'Elevator Access' },
                  { key: 'roomService', label: '24/7 Room Service' },
                ].map((f) => (
                  <label key={f.key} style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 13, cursor: 'pointer' }}>
                    <input
                      type="checkbox"
                      checked={hotelProfile.facilities[f.key as keyof typeof hotelProfile.facilities]}
                      onChange={(e) => {
                        setHotelProfile({
                          ...hotelProfile,
                          facilities: {
                            ...hotelProfile.facilities,
                            [f.key]: e.target.checked,
                          },
                        })
                      }}
                    />
                    <span>{f.label}</span>
                  </label>
                ))}
              </div>
            </div>
          </form>

          {/* Desktop domain records: editable by users with hotel edit access */}
          {isDesktop && (
            <section className="card" style={{ padding: 24, backgroundColor: 'white', borderRadius: 12, border: '1px solid #e2e8f0' }}>
              <h2 style={{ marginTop: 0 }}>Hotel Data Records</h2>
              <p style={{ color: '#64748b', fontSize: 13 }}>
                Edit the hotel's operational records. Reservation model: hotel, room type, room number, dates, guests, payments, other transactions and reservation group.
                Restaurant specifications and cash-flow fields are intentionally deferred for later design.
              </p>
              <div style={{ display: 'flex', gap: 10, alignItems: 'center', marginBottom: 12 }}>
                <select value={domainEntity} onChange={(e) => { setDomainEntity(e.target.value as typeof domainEntity); setDomainRecordId(''); setDomainRecordJson('{}') }}>
                  <option value="rooms">Rooms</option>
                  <option value="room_types">Room Types</option>
                  <option value="guests">Guests</option>
                  <option value="reservations">Reservations</option>
                  <option value="channels">Channels</option>
                  <option value="restaurants">Restaurants</option>
                </select>
                <button type="button" className="ghost" onClick={() => void reloadDomainRecords()}>Refresh</button>
              </div>
              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1.4fr auto', gap: 8, alignItems: 'start' }}>
                <input placeholder="Record ID (blank for new)" value={domainRecordId} onChange={(e) => setDomainRecordId(e.target.value)} />
                <textarea rows={4} value={domainRecordJson} onChange={(e) => setDomainRecordJson(e.target.value)} placeholder='Record JSON, e.g. {"name":"Deluxe","capacity":2}' />
                <button type="button" className="primary" onClick={() => {
                if (!selectedHotel) return
                const run = async () => {
                  try {
                    const parsed = JSON.parse(domainRecordJson) as Record<string, unknown>
                    const id = domainRecordId.trim() || crypto.randomUUID()
                    await desktop.saveDomainRecord(domainEntity, selectedHotel.id, id, parsed)
                    setDomainRecordId(id)
                    setDomainMessage(`${domainEntity} record saved.`)
                    await reloadDomainRecords()
                  } catch (err) {
                    setError(err instanceof Error ? err.message : String(err))
                  }
                }
                void run()
              }}>Save Record</button>
              </div>
              {domainMessage && <p style={{ color: '#15803d' }}>{domainMessage}</p>}
              <div style={{ marginTop: 14, display: 'grid', gap: 6 }}>
                {domainRecords.map((record) => (
                  <div key={String(record.id)} style={{ display: 'flex', gap: 8, alignItems: 'center', padding: 8, border: '1px solid #e2e8f0', borderRadius: 6 }}>
                    <code style={{ flex: 1, fontSize: 12, overflow: 'hidden', textOverflow: 'ellipsis' }}>{JSON.stringify(record)}</code>
                    <button type="button" className="ghost" onClick={() => { setDomainRecordId(String(record.id)); setDomainRecordJson(JSON.stringify(record, null, 2)) }}>Edit</button>
                    <button type="button" className="ghost" onClick={async () => { await desktop.deleteDomainRecord(domainEntity, selectedHotel.id, String(record.id)); await reloadDomainRecords() }}>Delete</button>
                  </div>
                ))}
                {domainRecords.length === 0 && <span style={{ color: '#94a3b8' }}>No local records for this category.</span>}
              </div>
            </section>
          )}

          {/* ================= 4- STAFF & USERS (MOVED INTO HOTEL PROFILE) ================= */}
          <section className="card" style={{ padding: 24, backgroundColor: 'white', borderRadius: 12, border: '1px solid #e2e8f0' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
              <div>
                <h2 style={{ margin: 0, fontSize: 20 }}>Hotel Staff & Users ({selectedHotel.name})</h2>
                <span style={{ fontSize: 13, color: '#64748b' }}>Accounts authorized for hotel ID {selectedHotel.id}</span>
              </div>
              <button className="primary" onClick={() => setShowAddUserModal(true)}>
                Add User to Hotel
              </button>
            </div>

            {/* Filter row for users */}
            <div style={{ display: 'grid', gridTemplateColumns: '2fr 1fr 1fr', gap: 10, marginBottom: 14, padding: 10, backgroundColor: '#f8fafc', borderRadius: 8 }}>
              <input
                placeholder="Filter username or name..."
                value={userSearchFilter}
                onChange={(e) => setUserSearchFilter(e.target.value)}
                style={{ padding: 8, fontSize: 13 }}
              />
              <select
                value={userRoleFilter}
                onChange={(e) => setUserRoleFilter(e.target.value)}
                style={{ padding: 8, fontSize: 13 }}
              >
                <option value="all">All Roles</option>
                <option value="manager">Manager</option>
                <option value="reception">Reception</option>
                <option value="accountant">Accountant</option>
                <option value="housekeeping">Housekeeping</option>
              </select>
              <select
                value={userStatusFilter}
                onChange={(e) => setUserStatusFilter(e.target.value as 'all' | 'active' | 'disabled')}
                style={{ padding: 8, fontSize: 13 }}
              >
                <option value="all">All Statuses</option>
                <option value="active">Active</option>
                <option value="disabled">Disabled</option>
              </select>
            </div>

            {/* Users Table */}
            <div style={{ border: '1px solid #e2e8f0', borderRadius: 8, overflow: 'hidden' }}>
              <table style={{ width: '100%', borderCollapse: 'collapse' }}>
                <thead>
                  <tr style={{ backgroundColor: '#f8fafc', borderBottom: '1px solid #e2e8f0', textAlign: 'left' }}>
                    <th style={{ padding: '10px 14px' }}>Username</th>
                    <th style={{ padding: '10px 14px' }}>Full Name</th>
                    <th style={{ padding: '10px 14px' }}>Role</th>
                    <th style={{ padding: '10px 14px' }}>Status</th>
                    <th style={{ padding: '10px 14px' }}>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {hotelUsers.length === 0 ? (
                    <tr>
                      <td colSpan={5} style={{ padding: 20, textAlign: 'center', color: '#94a3b8' }}>
                        No staff members found matching filter criteria.
                      </td>
                    </tr>
                  ) : (
                    hotelUsers.map((u) => (
                      <tr key={u.id} style={{ borderBottom: '1px solid #e2e8f0' }}>
                        <td style={{ padding: '10px 14px', fontWeight: 600 }}>{u.username}</td>
                        <td style={{ padding: '10px 14px' }}>{u.full_name}</td>
                        <td style={{ padding: '10px 14px' }}>
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
                            {u.role}
                          </span>
                        </td>
                        <td style={{ padding: '10px 14px' }}>
                          <span
                            style={{
                              padding: '2px 8px',
                              borderRadius: 12,
                              fontSize: 12,
                              backgroundColor: u.is_active ? '#dcfce7' : '#fee2e2',
                              color: u.is_active ? '#15803d' : '#b91c1c',
                            }}
                          >
                            {u.is_active ? 'Active' : 'Disabled'}
                          </span>
                        </td>
                        <td style={{ padding: '10px 14px' }}>
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
                              onClick={() => startEditUser(u)}
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
                                onClick={() => toggleUser(u.id, u.is_active)}
                              >
                                {u.is_active ? 'Disable' : 'Enable'}
                              </button>
                            )}
                          </div>
                        </td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>
          </section>

          {/* Add User Modal */}
          {showAddUserModal && (
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
                onSubmit={createUserForSelectedHotel}
                className="card"
                style={{ width: 440, padding: 24, backgroundColor: 'white', borderRadius: 12, display: 'grid', gap: 14 }}
              >
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                  <h3 style={{ margin: 0 }}>Add Staff Member ({selectedHotel.name})</h3>
                  <button type="button" className="ghost" onClick={() => setShowAddUserModal(false)}>
                    Close
                  </button>
                </div>
                <input
                  placeholder="Username"
                  value={newUser.username}
                  onChange={(e) => setNewUser({ ...newUser, username: e.target.value })}
                  required
                  style={{ padding: 8 }}
                />
                <input
                  placeholder="Full Name"
                  value={newUser.full_name}
                  onChange={(e) => setNewUser({ ...newUser, full_name: e.target.value })}
                  style={{ padding: 8 }}
                />
                <select
                  value={newUser.role}
                  onChange={(e) => setNewUser({ ...newUser, role: e.target.value })}
                  style={{ padding: 8 }}
                >
                  <option value="manager">Manager</option>
                  <option value="reception">Reception</option>
                  <option value="accountant">Accountant</option>
                  <option value="housekeeping">Housekeeping</option>
                </select>
                <input
                  placeholder="Password"
                  type="password"
                  value={newUser.password}
                  onChange={(e) => setNewUser({ ...newUser, password: e.target.value })}
                  required
                  style={{ padding: 8 }}
                />
                <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 4 }}>
                  <button type="button" className="ghost" onClick={() => setShowAddUserModal(false)}>
                    Cancel
                  </button>
                  <button className="primary" type="submit" disabled={busy}>
                    {busy ? 'Creating...' : 'Create Account'}
                  </button>
                </div>
              </form>
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
                zIndex: 9000,
              }}
            >
              <form
                onSubmit={saveEditUser}
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
                    Assigned Roles:
                  </label>
                  <div style={{ display: 'flex', flexWrap: 'wrap', gap: 12 }}>
                    {['manager', 'reception', 'accountant', 'housekeeping'].map((r) => (
                      <label key={r} style={{ display: 'flex', alignItems: 'center', gap: 6, cursor: 'pointer', fontSize: 13 }}>
                        <input
                          type="checkbox"
                          checked={editRoles.includes(r)}
                          onChange={() => {
                            if (editRoles.includes(r)) {
                              if (editRoles.length > 1) setEditRoles(editRoles.filter((x) => x !== r))
                            } else {
                              setEditRoles([...editRoles, r])
                            }
                          }}
                        />
                        <span style={{ textTransform: 'capitalize' }}>{r}</span>
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
                    Cancel
                  </button>
                  <button type="submit" className="primary" disabled={busy}>
                    {busy ? 'Saving...' : 'Save User'}
                  </button>
                </div>
              </form>
            </div>
          )}
        </div>
      ) : (
        /* ================= MAIN VIEW: HOTELS TABLE (10 RECENT) WITH FILTER ROW ================= */
        <div style={{ display: 'grid', gap: 24 }}>
          <section className="card" style={{ padding: 24, backgroundColor: 'white', borderRadius: 12, border: '1px solid #e2e8f0' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
              <div>
                <h2 style={{ margin: 0, fontSize: 20 }}>Hotels & Properties</h2>
                <span style={{ fontSize: 13, color: '#64748b' }}>Showing up to 10 recently active hotels. Click a hotel name to open its profile.</span>
              </div>
              <button className="primary" onClick={() => setShowCreateHotel((v) => !v)}>
                {showCreateHotel ? 'Cancel' : 'Add Hotel'}
              </button>
            </div>

            {showCreateHotel && (
              <form
                onSubmit={(e) => {
                  e.preventDefault()
                  void createHotel()
                }}
                style={{
                  display: 'grid',
                  gap: 12,
                  padding: 16,
                  backgroundColor: '#f8fafc',
                  borderRadius: 8,
                  marginBottom: 16,
                  border: '1px solid #e2e8f0',
                }}
              >
                <div style={{ display: 'flex', gap: 12 }}>
                  <input
                    placeholder="Hotel Name (e.g. Grand Resort)"
                    value={newHotel.name}
                    onChange={(e) => setNewHotel({ ...newHotel, name: e.target.value })}
                    required
                    style={{ flex: 2, padding: 8 }}
                  />
                  <input
                    placeholder="Hotel Code (e.g. 5001)"
                    value={newHotel.code}
                    onChange={(e) => setNewHotel({ ...newHotel, code: e.target.value })}
                    inputMode="numeric"
                    required
                    style={{ flex: 1, padding: 8 }}
                  />
                </div>
                <button className="primary" type="submit" disabled={busy} style={{ width: 'fit-content' }}>
                  {busy ? 'Creating...' : 'Create Hotel'}
                </button>
              </form>
            )}

            {/* Filter row for Hotels table */}
            <div style={{ display: 'grid', gridTemplateColumns: '120px 2fr 160px', gap: 10, marginBottom: 14, padding: 10, backgroundColor: '#f8fafc', borderRadius: 8 }}>
              <input
                placeholder="Code..."
                value={hotelCodeFilter}
                onChange={(e) => setHotelCodeFilter(e.target.value)}
                style={{ padding: 8, fontSize: 13 }}
              />
              <input
                placeholder="Filter hotel name..."
                value={hotelNameFilter}
                onChange={(e) => setHotelNameFilter(e.target.value)}
                style={{ padding: 8, fontSize: 13 }}
              />
              <select
                value={hotelStatusFilter}
                onChange={(e) => setHotelStatusFilter(e.target.value as 'all' | 'active' | 'disabled')}
                style={{ padding: 8, fontSize: 13 }}
              >
                <option value="all">All Statuses</option>
                <option value="active">Active</option>
                <option value="disabled">Disabled</option>
              </select>
            </div>

            {/* Hotels Table */}
            <div style={{ border: '1px solid #e2e8f0', borderRadius: 8, overflow: 'hidden' }}>
              <table style={{ width: '100%', borderCollapse: 'collapse' }}>
                <thead>
                  <tr style={{ backgroundColor: '#f8fafc', borderBottom: '1px solid #e2e8f0', textAlign: 'left' }}>
                    <th style={{ padding: '12px 16px', width: 100 }}>Code</th>
                    <th style={{ padding: '12px 16px' }}>Hotel Name (Click to open profile)</th>
                    <th style={{ padding: '12px 16px', width: 140 }}>Status</th>
                    <th style={{ padding: '12px 16px', width: 140 }}>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {filteredHotels.length === 0 ? (
                    <tr>
                      <td colSpan={4} style={{ padding: 24, textAlign: 'center', color: '#94a3b8' }}>
                        No hotels found matching filter criteria.
                      </td>
                    </tr>
                  ) : (
                    filteredHotels.map((h) => (
                      <tr key={h.id} style={{ borderBottom: '1px solid #e2e8f0' }}>
                        <td style={{ padding: '12px 16px', fontWeight: 600 }}>{h.id}</td>
                        <td style={{ padding: '12px 16px' }}>
                          <button
                            type="button"
                            onClick={() => openHotelProfile(h)}
                            style={{
                              background: 'none',
                              border: 'none',
                              padding: 0,
                              color: '#2563eb',
                              fontWeight: 600,
                              fontSize: 14,
                              cursor: 'pointer',
                              textDecoration: 'underline',
                              textAlign: 'left',
                            }}
                            title="Click to view and edit hotel profile"
                          >
                            {h.name}
                          </button>
                        </td>
                        <td style={{ padding: '12px 16px' }}>
                          <span
                            style={{
                              padding: '2px 8px',
                              borderRadius: 12,
                              fontSize: 12,
                              backgroundColor: h.is_active ? '#dcfce7' : '#fee2e2',
                              color: h.is_active ? '#15803d' : '#b91c1c',
                            }}
                          >
                            {h.is_active ? 'Active' : 'Disabled'}
                          </span>
                        </td>
                        <td style={{ padding: '12px 16px' }}>
                          {h.id !== 0 && (
                            <button
                              style={{
                                padding: '4px 10px',
                                fontSize: 12,
                                backgroundColor: h.is_active ? '#fee2e2' : '#dcfce7',
                                color: h.is_active ? '#b91c1c' : '#15803d',
                                border: 'none',
                                borderRadius: 4,
                                cursor: 'pointer',
                              }}
                              onClick={() => toggleHotel(h.id, h.is_active)}
                            >
                              {h.is_active ? 'Disable' : 'Enable'}
                            </button>
                          )}
                        </td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>
          </section>
        </div>
      )}
    </div>
  )
}
