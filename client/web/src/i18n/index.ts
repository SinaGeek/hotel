// Lightweight i18next setup with English + Turkish (todo F4).
// `en` is the source of truth; `tr` (./tr.ts) must keep exact key parity, which
// `assertKeyParity` checks at startup so a missing translation fails loudly
// instead of silently rendering English or a raw key.
const LANG_KEY = 'hms.lang'
export type Lang = 'en' | 'tr'
export const LANGS: Lang[] = ['en', 'tr']
export const LANG_LABEL: Record<Lang, string> = { en: 'English', tr: 'Türkçe' }

/** Flat dotted paths of every leaf, sorted — the parity contract. */
function leafPaths(obj: Record<string, unknown>, prefix = ''): string[] {
  const out: string[] = []
  for (const [k, v] of Object.entries(obj)) {
    const path = prefix ? `${prefix}.${k}` : k
    if (v !== null && typeof v === 'object') out.push(...leafPaths(v as Record<string, unknown>, path))
    else out.push(path)
  }
  return out.sort()
}

/** Returns the keys missing on either side (empty = in sync). */
export function keyDrift(
  a: Record<string, unknown>,
  b: Record<string, unknown>,
): { missingInB: string[]; missingInA: string[] } {
  const pa = leafPaths(a)
  const pb = new Set(leafPaths(b))
  const sa = new Set(pa)
  return {
    missingInB: pa.filter((k) => !pb.has(k)),
    missingInA: leafPaths(b).filter((k) => !sa.has(k)),
  }
}

function assertKeyParity() {
  const { missingInB, missingInA } = keyDrift(
    en as unknown as Record<string, unknown>,
    tr as unknown as Record<string, unknown>,
  )
  if (missingInB.length || missingInA.length) {
    // Dev-time guard: never ship a half-translated bundle silently.
    console.error('[i18n] English/Turkish key drift', { missingInTurkish: missingInB, missingInEnglish: missingInA })
  }
}

export function currentLang(): Lang {
  const stored = localStorage.getItem(LANG_KEY)
  return stored === 'tr' || stored === 'en' ? stored : 'en'
}

export async function setLang(lang: Lang) {
  localStorage.setItem(LANG_KEY, lang)
  document.documentElement.lang = lang
  await i18n.changeLanguage(lang)
}
import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import { tr } from './tr'

const en = {
  app: { title: 'HMS Hotel Management' },
  login: {
    hotelId: 'Hotel code',
    username: 'Username',
    password: 'Password',
    signIn: 'Sign in',
    mustChangePassword: 'You must change your password on first login',
  },
  changePassword: {
    title: 'Change password',
    old: 'Current password',
    new: 'New password (min 8 chars)',
    submit: 'Change & sign in',
  },
  nav: {
    dashboard: 'Dashboard',
    board: 'Monthly Room',
    bookings: 'Bookings',
    list: 'List',
    guests: 'Guests',
    facility: 'Facility',
    rooms: 'Rooms',
    updateStatus: 'Update status',
    housekeeping: 'Housekeeping',
    reports: 'Reports',
    tools: 'Tools',
    staff: 'Staff',
    channels: 'Channels',
    users: 'Users',
    logout: 'Sign out',
    menu: 'Menu',
  },
  ch: {
    subtitle: 'Channel accounts connected to the hotel',
    shown: 'shown',
    search: 'Search channels',
    all: 'All statuses',
    active: 'Active',
    inactive: 'Inactive',
    channel: 'Channel',
    kind: 'Kind',
  },
  shell: {
    hotelCode: 'Hotel Code: {{code}}',
    notifications: 'Notifications',
    language: 'Language',
    darkMode: 'Dark mode',
    lightMode: 'Light mode',
    newRecord: 'New record',
  },
  update: {
    available: 'Update {{version}} available',
    mandatory: 'Update {{version}} required',
    install: 'Install',
    installing: 'Installing…',
    installed: 'Installed — restart to finish',
    blockedTitle: 'Update required before syncing',
    blockedMsg:
      'The server no longer accepts this app version ({{reason}}). Install the update to resume syncing; your local data and any queued changes are preserved.',
  },
  sync: {
    syncNow: 'Sync now',
    syncing: 'Syncing…',
    syncNowHint: 'Push queued changes and pull the latest from the server',
    synced: 'Synced {{when}}',
    syncedPending: 'Synced; {{pending}} operation(s) waiting',
    allSynced: 'All changes are synced with the server',
    conflicts: '{{n}} unresolved booking conflict(s)',
    offline: 'Offline — {{pending}} operation(s) queued',
    offlineTitle: 'Cannot reach the server; your changes are queued and will sync automatically',
    offlineReason: 'Last sync error: {{reason}}',
    justNow: 'just now',
    minutesAgo: '{{n}} min ago',
    hoursAgo: '{{n}} h ago',
    daysAgo: '{{n}} d ago',
    unknown: 'Waiting for first sync…',
    unknownTitle: 'No successful sync yet; the app will keep working locally',
  },
  board: {
    calendar: 'Calendar',
    list: 'List',
    seven: '7 days',
    fifteen: '15 days',
    month: 'Month',
    today: 'Today',
    nights: 'Nights',
    roomsCount: '{{n}} rooms',
    clickToBook: 'Click to book',
    occupancy: 'Occupancy',
    available: 'avail',
    cheapest: 'from',
    noStays: 'Booked',
    legend: {
      reservation: 'Reservation',
      conflict: 'Conflict',
      cancelled: 'Cancelled',
      dragHint: 'Drag across empty days to add a record',
    },
  },
  res: {
    list: 'Reservations',
    guest: 'Guest',
    room: 'Room',
    checkin: 'Check-in',
    checkout: 'Check-out',
    status: 'Status',
    total: 'Total',
    channel: 'Channel',
    adults: 'Adults',
    children: 'Children',
    voucher: 'Voucher #',
    currency: 'Currency',
    create: 'Create reservation',
    cancel: 'Cancel',
    confirmed: 'Confirmed',
    checked_in: 'In house',
    checked_out: 'Checked out',
    cancelled: 'Cancelled',
    conflict: 'Conflict',
  },
  pay: {
    title: 'Payments',
    amount: 'Amount',
    cash: 'Cash',
    creditCard: 'Credit card',
    bank: 'Bank transfer',
    agency: 'Agency',
    record: 'Record payment',
    refund: 'Refund',
  },
  folio: {
    title: 'Extras (folio)',
    category: 'Category',
    category_room: 'Room',
    category_minibar: 'Minibar',
    category_spa: 'Spa',
    category_restaurant: 'Restaurant',
    category_extra: 'Other',
    desc: 'Description',
    unitPrice: 'Unit price',
    qty: 'Qty',
    add: 'Add',
  },
  guests: { title: 'Guests', search: 'Search...', fullName: 'Full name', nationality: 'Nationality', phone: 'Phone', create: 'Add guest' },
  bk: {
    add: '+ Add Booking',
    search: 'Search...',
    view: 'View',
    tabs: {
      active: 'Active',
      upcoming: 'Upcoming',
      conflict: 'Conflicts',
      archive: 'Archive',
      cancelled: 'Cancelled',
      all: 'All',
    },
    reassign: 'Reassign Room',
    reassignTitle: 'Resolve Booking Conflict',
    reassignDesc: 'This booking has a conflict due to an overlapping stay on the requested room. Please select a free room below to confirm the reservation.',
    suggestedRooms: 'Suggested Free Rooms',
    sameRoomType: 'Same room type (recommended)',
    noRoomsAvailable: 'No available rooms found for these dates.',
    reassignSuccess: 'Reservation successfully reassigned to Room {{room}}.',
    reassignBtn: 'Reassign to Room {{room}}',
    conflictAlertTitle: 'New Booking Conflicts Detected',
    conflictAlertMsg: '{{n}} booking(s) require a new room due to overlapping stays.',
    conflictBanner: '{{n}} booking conflict(s) need room reassignment.',
    resolveConflicts: 'Resolve Conflicts',
    orderNewest: 'Newest',
    orderFixed: 'The list is ordered newest first by the server',
    clear: 'Clear',
    records: '{{n}} records',
    balance: 'Balance',
    noRows: 'No records found.',
  },
  gu: {
    download: 'Download',
    add: '+ Add Guest',
    quickLabel: 'Quick views',
    search: 'Search...',
    fullName: 'Full name',
    nationality: 'Nationality',
    phone: 'Phone',
    plate: 'License plate',
    birth: 'Birth date',
    category: 'Category',
    country: 'All countries',
    primary: 'Primary',
    standard: 'Standard',
    quick: {
      all: 'All',
      birthday: 'Birthday',
      garage: 'Garage',
    },
  },
  dash: {
    search: 'Search room or guest',
    suggest: 'Your suggestion',
    sendSuggest: 'Send a suggestion',
    suggestThanks: 'Thanks! Feedback noted.',
    chip: {
      departToday: 'Departing Today',
      departTomorrow: 'Departing Tomorrow',
      longTerm: 'Long Term',
      willCheckin: 'Will Check-in',
      outs: 'Out of Service',
      dirty: 'Dirty Room',
    },
    roomsUnit: 'rooms',
    leavesOn: 'Leaves {{d}}',
    makeRes: 'Make Reservation',
    noMatch: 'No records found.',
    stat: {
      occupied: 'Occupied Rooms',
      guests: 'Guests Today',
      vacant: 'Vacant Rooms',
      total: 'Total Rooms',
    },
  },
  fac: {
    roomCount: '{{n}} rooms',
    floor: 'Floor',
    floorN: 'Floor {{n}}',
    filterStatus: 'All statuses',
    filterFloor: 'All floors',
    filterCleaning: 'All cleaning states',
    cleaningStatus: 'Status',
    hotelFallback: 'Hotel',
    downloadTip: 'Download the current housekeeping list as CSV',
  },
  rooms: {
    title: 'Rooms',
    type: 'Type',
    number: 'Room #',
    cleanliness: 'Cleanliness',
    dirty: 'Dirty',
    cleaning: 'Cleaning',
    clean: 'Clean',
    inspected: 'Inspected',
    out_of_order: 'Out of order',
    update: 'Update',
  },
  rep: {
    title: 'Reports',
    hubSubtitle: 'Operational and financial reports. Reports marked Soon are wired to reference groups but need their Phase 7 data endpoints.',
    soon: 'Soon',
    back: 'Back to Reports',
    view: 'View',
    table: 'Table',
    chart: 'Chart',
    date: 'Date',
    days: 'days',
    linkMainCourante: 'Main Courante',
    linkForecast: 'Occupancy Forecast',
    linkInhouse: 'In-House',
    group: {
      daily: 'Daily',
      bookings: 'Bookings',
      rooms: 'Rooms & Rates',
      guests: 'Guests',
      finance: 'Finance',
      system: 'System',
    },
    maincourante: { title: 'Main Courante' },
    forecast: {
      title: 'Occupancy Forecast',
      start: 'Start',
      date: 'Date',
      arrivals: "Today's Arrivals",
      vacant: 'Vacant',
      guests: 'Guests',
      occupancy: 'Occupancy',
      occupied: 'Occupied',
    },
    inhouse: {
      title: 'In-House',
      totalBookings: 'Total bookings',
      guests: 'Guests',
      roomRate: 'Room rate',
      balance: "Today's balance",
    },
    analysis: 'Revenue Analysis',
    mealList: 'Meal List',
    nationality: 'Nationality Breakdown',
    channelAnalysis: 'Channel Analysis',
    collectionSummary: 'Collection Summary',
    expenseSummary: 'Expense Summary',
    freeRooms: 'Free Rooms',
    willCheckin: 'Expected Arrivals',
    checkin: 'Completed Arrivals',
    willCheckout: 'Expected Departures',
    earlyExit: 'Early Departures',
    dateChange: 'Date Changes',
    deleted: 'Deleted Bookings',
    unselectedRoom: 'Bookings Without a Room',
    transfer: 'Transfers',
    roomChange: 'Room Changes',
    roomOccupancy: 'Room Occupancy Rates',
    roomClean: 'Housekeeping',
    periodPrice: 'Period Rates',
    priceChange: 'Rate Changes',
    guestDaily: 'Daily Guest List',
    collection: 'Collection Report',
    expense: 'Expense Report',
    extra: 'Extras Report',
    invoice: 'Invoice Report',
    bankAccounts: 'Bank Accounts',
    bankTransactions: 'Bank Transactions',
    agencyBalance: 'Agency Balances',
    agencyRate: 'Agency Performance',
    log: 'Activity Log',
    mc: {
      dailyRate: 'Daily rate',
      prevBalance: "Yesterday's balance",
      balance: "Today's balance",
      roomRate: 'Room rate',
    },
  },
  reports: {
    title: 'Reports',
    noAccess: 'You do not have permission to view reports.',
    from: 'From',
    to: 'To',
    refresh: 'Refresh',
    today: 'Today',
    last7: 'Last 7 days',
    last30: 'Last 30 days',
    last90: 'Last 90 days',
    roomRevenue: 'Room revenue',
    extraRevenue: 'Extras',
    collected: 'Payments collected',
    occupancy: 'Occupancy',
    adr: 'ADR',
    revpar: 'RevPAR',
    byDay: 'Revenue by day',
    byMonth: 'Revenue by month',
    month: 'Month',
    accrued: 'Accrued',
    basis: 'Revenue basis',
    byChannel: 'Earnings by channel',
    byChannelChart: 'Revenue by channel',
    byRoom: 'Revenue by room',
    channel: 'Channel',
    date: 'Date',
    occupiedNights: 'Occupied nights',
    noData: 'No revenue in this period.',
    offlineTitle: 'Offline report from this device',
    offlineDetail: 'Values use the last synced local data. Room totals are spread evenly across stay nights; sync to reconcile with the server.',
    exportCsv: 'Export CSV',
    printPdf: 'Print / PDF',
  },
  users: {
    title: 'Users',
    role: 'Role',
    admin: 'Admin',
    manager: 'Manager',
    reception: 'Reception',
    accountant: 'Accountant',
    housekeeping: 'Housekeeping',
    password: 'Password',
    fullName: 'Full name',
    create: 'Add user',
    matrix: 'Permission matrix',
    active: 'Active',
    passive: 'Disabled',
  },
  common: { loading: 'Loading...', error: 'Error', save: 'Save', cancel: 'Cancel', back: 'Back' },
}

i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, tr: { translation: tr } },
  lng: currentLang(),
  fallbackLng: 'en',
  interpolation: { escapeValue: false },
})
document.documentElement.lang = currentLang()
assertKeyParity()

export default i18n
