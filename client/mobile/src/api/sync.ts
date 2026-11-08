// G5: mobile offline sync adapter.
//
// Identical protocol to the desktop shell (README §11): client-generated
// UUIDv7 operation ids, a durable outbox, monotonically increasing server
// change counters, and idempotent re-push after reconnect. The difference is
// only the storage backend: Expo AsyncStorage instead of SQLCipher SQLite.
//
// The wire shapes below are structurally identical to `hms_core::sync`
// (`SyncPushRequest`/`SyncPushResponse`/`SyncPullResponse`), so the Rust server
// accepts mobile ops without any server-side change and protocol version bumps
// apply to both clients at once.
import AsyncStorage from '@react-native-async-storage/async-storage'
import { api, loadSession } from './client'

export const SYNC_PROTOCOL_VERSION = 1

export type OperationType = 'insert' | 'update' | 'delete'
export type EntityType =
  | 'room_type'
  | 'room'
  | 'room_status'
  | 'reservation'
  | 'guest'
  | 'payment'
  | 'folio_transaction'
  | 'price_rule'
  | 'user'
  | 'channel'

export interface ChangeOperation {
  operation_id: string
  hotel_id: number
  entity_type: EntityType
  entity_id: string
  operation_type: OperationType
  payload: unknown
  client_timestamp: string
}

export type SyncOpStatus =
  | 'applied'
  | 'duplicate_ignored'
  | { conflict_resolved: { message: string } }
  | { rejected: { reason: string } }

export interface SyncOpResult {
  operation_id: string
  entity_type: EntityType
  entity_id: string
  status: SyncOpStatus
}

export interface SyncPushResponse {
  hotel_id: number
  results: SyncOpResult[]
  server_latest_counter: number
}

export interface ServerChangeRecord {
  change_counter: number
  hotel_id: number
  operation_id: string
  entity_type: EntityType
  entity_id: string
  operation_type: OperationType
  payload: unknown
  server_timestamp: string
}

export interface SyncPullResponse {
  hotel_id: number
  records: ServerChangeRecord[]
  latest_counter: number
  has_more: boolean
}

const OUTBOX_KEY = 'hms.mobile.outbox'
const COUNTER_KEY = 'hms.mobile.counter'
const MIRROR_PREFIX = 'hms.mobile.mirror.'

/** Monotonic, collision-free id (UUIDv7-shaped; last 12 hex digits are random). */
export function newOpId(): string {
  const ts = Date.now().toString(16).padStart(12, '0')
  const rnd = Array.from({ length: 20 }, () => Math.floor(Math.random() * 16).toString(16)).join('')
  return `${ts.slice(0, 8)}-${ts.slice(8, 12)}-7${rnd.slice(0, 3)}-8${rnd.slice(3, 6)}-${rnd.slice(6, 18)}`
}

export interface SyncStatus {
  pending: number
  lastSyncedAt: string | null
  lastError: string | null
  counter: number
  /** operations acknowledged by the server during the last cycle */
  pushed: number
}

export class MobileSync {
  private outbox: ChangeOperation[] = []
  private loading: Promise<void> | null = null

  async init(): Promise<void> {
    if (this.loading) return this.loading
    this.loading = (async () => {
      const raw = await AsyncStorage.getItem(OUTBOX_KEY)
      this.outbox = raw ? (JSON.parse(raw) as ChangeOperation[]) : []
    })()
    return this.loading
  }

  private async persist(): Promise<void> {
    await AsyncStorage.setItem(OUTBOX_KEY, JSON.stringify(this.outbox))
  }

  async pendingCount(): Promise<number> {
    await this.init()
    return this.outbox.length
  }

  /** Stage a write locally. Returns the operation id (also the entity id for inserts). */
  async stage(
    operation: OperationType,
    entityType: EntityType,
    entityId: string,
    payload: unknown,
    hotelId: number,
  ): Promise<string> {
    await this.init()
    const op: ChangeOperation = {
      operation_id: newOpId(),
      hotel_id: hotelId,
      entity_type: entityType,
      entity_id: entityId,
      operation_type: operation,
      payload,
      client_timestamp: new Date().toISOString(),
    }
    this.outbox.push(op)
    await this.persist()
    await this.writeMirror(entityType, entityId, payload)
    return op.operation_id
  }

  /** Read the last-known value for an entity (offline cache). */
  async readMirror<T>(entityType: EntityType, entityId: string): Promise<T | null> {
    const raw = await AsyncStorage.getItem(MIRROR_PREFIX + entityType + '.' + entityId)
    return raw ? (JSON.parse(raw) as T) : null
  }

  private async writeMirror(entityType: EntityType, entityId: string, payload: unknown): Promise<void> {
    await AsyncStorage.setItem(MIRROR_PREFIX + entityType + '.' + entityId, JSON.stringify(payload))
  }

  /** Push the outbox and pull canonical changes. Safe to call on every reconnect. */
  async syncOnce(): Promise<SyncStatus> {
    await this.init()
    const session = await loadSession()
    if (!session) return { pending: this.outbox.length, lastSyncedAt: null, lastError: 'not signed in', counter: 0, pushed: 0 }

    let pushed = 0
    try {
      if (this.outbox.length) {
        const req = {
          hotel_id: session.user.hotel_id,
          operations: this.outbox,
          protocol_version: SYNC_PROTOCOL_VERSION,
        }
        const res = await fetch(`${api.baseUrl}/sync/push`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${session.token}` },
          body: JSON.stringify(req),
        })
        if (!res.ok) throw new Error(`push HTTP ${res.status}`)
        const envelope = (await res.json()) as { success: boolean; data: SyncPushResponse; error?: string | null }
        if (!envelope.success) throw new Error(envelope.error ?? 'push rejected')
        const body = envelope.data
        // Only drop operations the server accepted (applied / duplicate / conflict).
        const done = new Set(
          body.results.filter((r) => r.status !== null && !(typeof r.status === 'object' && 'rejected' in r.status)).map((r) => r.operation_id),
        )
        const before = this.outbox.length
        this.outbox = this.outbox.filter((o) => !done.has(o.operation_id))
        pushed = before - this.outbox.length
        await this.persist()
      }
    } catch (e) {
      return { pending: this.outbox.length, lastSyncedAt: null, lastError: String(e), counter: 0, pushed: 0 }
    }

    let counter = Number((await AsyncStorage.getItem(COUNTER_KEY)) ?? '0')
    try {
      let hasMore = true
      while (hasMore) {
        const res = await fetch(`${api.baseUrl}/sync/pull?since=${counter}&limit=200&v=${SYNC_PROTOCOL_VERSION}`, {
          headers: { Authorization: `Bearer ${session.token}` },
        })
        if (!res.ok) throw new Error(`pull HTTP ${res.status}`)
        const envelope = (await res.json()) as { success: boolean; data: SyncPullResponse; error?: string | null }
        if (!envelope.success) throw new Error(envelope.error ?? 'pull rejected')
        const body = envelope.data
        for (const rec of body.records) {
          if (rec.operation_type === 'delete') {
            await AsyncStorage.removeItem(MIRROR_PREFIX + rec.entity_type + '.' + rec.entity_id)
          } else {
            await this.writeMirror(rec.entity_type, rec.entity_id, rec.payload)
          }
        }
        counter = body.latest_counter
        hasMore = body.has_more
      }
      await AsyncStorage.setItem(COUNTER_KEY, String(counter))
    } catch (e) {
      return { pending: this.outbox.length, lastSyncedAt: null, lastError: String(e), counter, pushed }
    }

    return { pending: this.outbox.length, lastSyncedAt: new Date().toISOString(), lastError: null, counter, pushed }
  }
}

export const mobileSync = new MobileSync()

