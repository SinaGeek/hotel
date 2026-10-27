import { Navigate, Route, Routes, useLocation } from 'react-router-dom'
import { Login } from './pages/Login'
import { AppShell } from './components/AppShell'
import { Dashboard } from './pages/Dashboard'
import { useAuth } from './api/auth'

export default function App() {
  const auth = useAuth()
  const location = useLocation()
  if (!auth.user && location.pathname !== '/login') {
    return <Navigate to="/login" replace />
  }
  return (
    <Routes>
      <Route path="/login" element={<Login />} />
      <Route element={<AppShell />}>
        <Route path="/" element={<Dashboard />} />
      </Route>
    </Routes>
  )
}
