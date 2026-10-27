import { createContext, useContext } from 'react'
import { clearSession, loadSession, saveSession, type SessionUser } from './client'

export function useAuth() {
  const user = loadSession()
  return {
    user,
    signIn(user: SessionUser) { saveSession(user); },
    signOut() { clearSession(); },
  }
}

// Kept for future context-based auth state (React context).
export const AuthContext = createContext<SessionUser | null>(loadSession())
export const useAuthContext = () => useContext(AuthContext)
