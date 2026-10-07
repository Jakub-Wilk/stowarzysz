import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router'

import App from './App.tsx'
import { AuthProvider } from './features/auth/AuthProvider.tsx'
import './index.css'
import { setBaseColor } from './themes/color.ts'
import { ThemeProvider } from './themes/ThemeProvider.tsx'

// No UI for the base color yet; change it from the console, e.g. `setBaseColor(150)`.
window.setBaseColor = setBaseColor

// No client-side caching: data is always stale (refetched on mount, focus and reconnect) and
// dropped as soon as no screen uses it, so a revisited screen loads fresh instead of showing old data.
const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 0, gcTime: 0 } },
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ThemeProvider>
      <QueryClientProvider client={queryClient}>
        <AuthProvider>
          <BrowserRouter>
            <App />
          </BrowserRouter>
        </AuthProvider>
      </QueryClientProvider>
    </ThemeProvider>
  </StrictMode>,
)
