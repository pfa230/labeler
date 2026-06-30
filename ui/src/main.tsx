import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import './theme.css'
import { App } from './app/App'
import { ToastProvider } from './app/toast'
import type { AuthState } from './api/auth'

const queryClient = new QueryClient()

// A 401 (client.ts dispatches `labeler:unauthenticated`) means the session is gone. Mark auth as
// logged-out in the cache so the RedirectIfAuthed guard on /login renders Login instead of bouncing
// back to / on stale `authed:true`. A subsequent /auth/me refetch corrects a spurious 401. See #103.
window.addEventListener('labeler:unauthenticated', () => {
  queryClient.setQueryData<AuthState>(['auth'], (prev) => ({
    ...(prev ?? { needsSetup: false }),
    authed: false,
  }))
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <ToastProvider>
        <App />
      </ToastProvider>
    </QueryClientProvider>
  </StrictMode>,
)
