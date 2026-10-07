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

const queryClient = new QueryClient()

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
