import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { overlayScrollbars } from 'dowel-ui'

import { App } from '@/App'
import { startWorker } from '@/offline'
import { initTheme } from '@/theme'
import '@/styles.css'

// Before the first paint, so the app never flashes the theme it was not set to.
initTheme()
// A phone or a Mac draws its own bar that takes no width; elsewhere the
// theme's strip stays.
overlayScrollbars()
startWorker()

const root = document.getElementById('root')
if (!root) {
  throw new Error('#root is missing from index.html')
}

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
