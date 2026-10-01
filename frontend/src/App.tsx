import { Route, Routes } from 'react-router'

import { Button } from '@/components/ui/button'

function Home() {
  return (
    <main className="flex min-h-svh flex-col items-center justify-center gap-4">
      <h1 className="text-2xl font-semibold">stowarzysz</h1>
      <Button>It works</Button>
    </main>
  )
}

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<Home />} />
    </Routes>
  )
}
