/**
 * The answers given on this device, on their way to the tutor.
 *
 * Every answer is written here first and delivered from here, whether the
 * tutor is a room away or the learner is underground (ADR 0011): one way for
 * an answer to travel, so the one that waited a day takes the same road as
 * the one that arrived in a second. The store is IndexedDB rather than
 * localStorage: it survives the browser reclaiming space, and the service
 * worker can read it for the number on the icon.
 *
 * Answers are delivered oldest first. An answer the tutor has taken stays
 * here, marked landed, until a fresh Today has been read after it: until
 * then the copy of Today the device holds still lists its card as waiting,
 * and the sitting must not ask it again.
 */

/** One answer, as the device keeps it until the tutor has it. */
export interface Sending {
  /** Insertion order, assigned by the store: the delivery order. */
  id?: number
  /** The identity the answer travels under, minted here. */
  answerId: string
  /** The API path it goes to, without `/api`. */
  path: string
  /** The body as it will be sent, carrying its identity and its moment. */
  body: unknown
  /** The card it answers: `formula:<id>:<direction>` or `word:<id>`. */
  card: string
  /** When it was given, by this device's clock. */
  answeredAt: number
  /** When the tutor took it, by this device's clock; absent until then. */
  landedAt?: number
}

const DATABASE = 'hilvan'
const STORE = 'outbox'
const VERSION = 1

/**
 * Opens the store, creating it on first use. A connection per call: a held
 * one blocks a version change in another tab, and opening an existing
 * database costs a fraction of the write that follows.
 */
function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DATABASE, VERSION)
    request.onupgradeneeded = () => {
      const db = request.result
      if (!db.objectStoreNames.contains(STORE)) {
        db.createObjectStore(STORE, { keyPath: 'id', autoIncrement: true })
      }
    }
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error ?? new Error('the answers on this device could not be opened'))
  })
}

async function withStore<T>(mode: IDBTransactionMode, run: (store: IDBObjectStore) => IDBRequest): Promise<T> {
  const db = await open()
  try {
    return await new Promise<T>((resolve, reject) => {
      const request = run(db.transaction(STORE, mode).objectStore(STORE))
      request.onsuccess = () => resolve(request.result as T)
      request.onerror = () => reject(request.error ?? new Error('the answers on this device could not be used'))
    })
  } finally {
    db.close()
  }
}

/**
 * Answers held in this page because the browser would not keep them - a
 * private window, site data blocked. Retried like any other while the page
 * is open, and the screen says that this is all the keeping they get. Once
 * the store has refused, everything after goes here too, so the order stays
 * the order the answers were given in. Negative ids never meet the store's.
 */
const memory: Sending[] = []
let refused = false
let memoryId = 0

/** Adds an answer to the end of the line. */
export async function enqueue(sending: Sending): Promise<number> {
  if (!refused) {
    try {
      return await withStore<number>('readwrite', (store) => store.add(sending))
    } catch {
      refused = true
    }
  }
  memoryId -= 1
  memory.push({ ...sending, id: memoryId })
  return memoryId
}

/** Everything held, landed or not, oldest first. */
export async function held(): Promise<Sending[]> {
  let stored: Sending[] = []
  try {
    stored = await withStore<Sending[]>('readonly', (store) => store.getAll())
  } catch {
    // Nothing on the device to read; what is in the page still counts.
  }
  return [...stored, ...memory]
}

/** The answers the tutor does not have yet, oldest first. */
export async function pending(): Promise<Sending[]> {
  return (await held()).filter((sending) => sending.landedAt === undefined)
}

/** How many answers live only in this page, and go when it closes. */
export const unkept = (): number => memory.filter((sending) => sending.landedAt === undefined).length

/** Records that the tutor has taken an answer. */
export async function land(sending: Sending, at: number): Promise<void> {
  const landed = { ...sending, landedAt: at }
  if (sending.id !== undefined && sending.id < 0) {
    const index = memory.findIndex((kept) => kept.id === sending.id)
    if (index !== -1) memory[index] = landed
    return
  }
  await withStore('readwrite', (store) => store.put(landed))
}

/** Drops an answer: one the tutor refused, which no retry would change. */
export async function forget(sending: Sending): Promise<void> {
  if (sending.id === undefined) return
  if (sending.id < 0) {
    const index = memory.findIndex((kept) => kept.id === sending.id)
    if (index !== -1) memory.splice(index, 1)
    return
  }
  await withStore('readwrite', (store) => store.delete(sending.id!))
}

/**
 * Drops the answers a Today read at `readAt` already counts: landed before
 * the read began, by this device's clock, so the read could not have missed
 * them.
 */
export async function settle(readAt: number): Promise<void> {
  for (const sending of await held()) {
    if (sending.landedAt !== undefined && sending.landedAt < readAt) await forget(sending)
  }
}

/** Empties the store: the device signed out, and the learner's answers go with it. */
export async function clear(): Promise<void> {
  memory.length = 0
  try {
    await withStore('readwrite', (store) => store.clear())
  } catch {
    // Nothing was kept on the device to clear.
  }
}

/**
 * An identity for one answer, minted here, so an answer delivered twice -
 * the reply to the first lost on the way back - lands once. `randomUUID` is
 * missing on a page served over plain HTTP, which a stand on a home network
 * can be, so there is a fallback rather than a crash.
 */
export function mintId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') return crypto.randomUUID()
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 12)}`
}
