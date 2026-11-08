import { Navigate, Route, Routes, useLocation } from 'react-router-dom'
import { Login } from './pages/Login'
import { ChangePassword } from './pages/ChangePassword'
import { AppShell } from './components/AppShell'
import { Dashboard } from './pages/Dashboard'
import { Board } from './pages/Board'
import { Reservations } from './pages/Reservations'
import { Guests } from './pages/Guests'
import { Rooms } from './pages/Rooms'
import { Housekeeping } from './pages/Housekeeping'
import { Users } from './pages/Users'
import { Channels } from './pages/Channels'
import { Reports } from './pages/Reports'
import { ReportsHub, ReportsForecast, ReportsInhouse, ReportsMainCourante } from './pages/ReportPages'
import { Admin } from './pages/Admin'
import AdminPanel from './pages/AdminPanel'
import Settings from './pages/Settings'
import OTAIntegrations from './pages/OTAIntegrations'
import { useAuth } from './api/auth'
import { desktop, isDesktop } from './api/desktop'
import FirstRunSetup from './pages/FirstRunSetup'
import { useEffect, useState } from 'react'

export default function App() {
  const auth = useAuth()
  const location = useLocation()
  const [setupChecked, setSetupChecked] = useState(!isDesktop)
  const [firstRun, setFirstRun] = useState(false)

  useEffect(() => {
    if (!isDesktop) return
    desktop.getAppPreferences().then((p) => {
      setFirstRun(p.first_run)
      setSetupChecked(true)
      document.documentElement.dataset.theme = p.theme
    }).catch(() => setSetupChecked(true))
  }, [])

  if (!setupChecked) return <main className="page">Preparing HMS Desktop…</main>
  if (firstRun) return <FirstRunSetup onDone={() => setFirstRun(false)} />
  const session = auth.session
  if (!session && location.pathname !== '/login' && location.pathname !== '/change-password') {
    return <Navigate to="/login" replace />
  }
  return (
    <Routes>
      <Route path="/login" element={session ? <Navigate to="/" replace /> : <Login />} />
      <Route path="/change-password" element={<ChangePassword />} />
      <Route element={<AppShell />}>
        <Route path="/" element={<Dashboard />} />
        <Route path="/admin" element={<Admin />} />
        <Route path="/import" element={<AdminPanel />} />
        <Route path="/board" element={<Board />} />
        <Route path="/reservations" element={<Reservations />} />
        <Route path="/guests" element={<Guests />} />
        <Route path="/rooms" element={<Rooms />} />
        <Route path="/housekeeping" element={<Housekeeping />} />
        <Route path="/users" element={<Users />} />
        <Route path="/channels" element={<Channels />} />
        <Route path="/reports" element={<ReportsHub />} />
        <Route path="/reports/analysis" element={<Reports />} />
        <Route path="/reports/forecast" element={<ReportsForecast />} />
        <Route path="/reports/inhouse" element={<ReportsInhouse />} />
        <Route path="/reports/maincourante" element={<ReportsMainCourante />} />
        <Route path="/settings" element={<Settings />} />
        <Route path="/ota-integrations" element={<OTAIntegrations />} />
      </Route>
    </Routes>
  )
}
