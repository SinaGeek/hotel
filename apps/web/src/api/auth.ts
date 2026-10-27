// Auth state: session (token + user + permissions) in localStorage/signals.
import { createContext, useContext } from 'react'
import { clearSession, loadSession, saveSession, type Session } from './client'

export function useAuth() {
  const session = loadSession()
  return {
    session,
    user: session?.user ?? null,
    permissions: session?.permissions ?? [],
    signIn(session: Session) { saveSession(session) },
    signOut() { clearSession() },
  }
}

// Kept for future context-based auth state (React context).
export const AuthContext = createContext<Session | null>(loadSession())
export const useAuthContext = () => useContext(AuthContext)
