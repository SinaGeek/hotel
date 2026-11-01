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
import { useAuth } from './api/auth'

export default function App() {
  const auth = useAuth()
  const location = useLocation()
  const session = auth.session
  if (!session && location.pathname !== '/login' && location.pathname !== '/change-password') {
    return <Navigate to="/login" replace />
  }
  // forced password change: only the change page is reachable (README §8)
  if (session?.user.force_password_change && location.pathname !== '/change-password') {
    return <Navigate to="/change-password" replace />
  }
  return (
    <Routes>
      <Route path="/login" element={session ? <Navigate to="/" replace /> : <Login />} />
      <Route path="/change-password" element={<ChangePassword />} />
      <Route element={<AppShell />}>
        <Route path="/" element={<Dashboard />} />
        <Route path="/admin" element={<Admin />} />
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
      </Route>
    </Routes>
  )
}
