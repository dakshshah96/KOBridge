import { api, errText, today, type BackupFile, type Settings, type Status } from './api'

export const app = $state({
  status: null as Status | null,
  offlineReason: '',
  offlineHint: '',
  checking: true,
  settings: null as Settings | null,
  backup: { busy: false, bytes: 0, error: '', last: null as BackupFile | null },
})

/** Turn ssh's stderr into a next step; keep the raw text for details. */
function explain(raw: string) {
  if (/resolve hostname kindle|No route to host|Network is unreachable/i.test(raw))
    return 'La Kindle no responde en ninguna de sus IPs conocidas. Si la Terminal sí llega, dale permiso a KOBridge en Ajustes del Sistema → Privacidad y seguridad → Red local.'
  if (/timed out/i.test(raw)) return 'La Kindle no respondió a tiempo. Puede que su Wi-Fi esté dormido: tocá la pantalla y reintentá.'
  if (/Connection refused/i.test(raw)) return 'La Kindle está en la red pero el servidor SSH de KOReader está apagado.'
  if (/Permission denied/i.test(raw)) return 'La Kindle rechazó la llave SSH.'
  return 'No pude conectar con la Kindle.'
}

export async function refreshStatus() {
  app.checking = true
  try {
    app.status = await api.status()
    app.offlineReason = ''
  } catch (e) {
    app.status = null
    app.offlineReason = errText(e)
    app.offlineHint = explain(app.offlineReason)
  } finally {
    app.checking = false
  }
}

export async function loadSettings() {
  app.settings = await api.getSettings()
}

export async function saveSettings(patch: Partial<Settings>) {
  if (!app.settings) return
  app.settings = { ...app.settings, ...patch }
  await api.setSettings(app.settings)
}

const stamp = () => {
  const d = new Date()
  const p = (n: number) => String(n).padStart(2, '0')
  return `${today()}-${p(d.getHours())}${p(d.getMinutes())}`
}

export async function runBackup() {
  if (app.backup.busy || !app.status) return
  app.backup = { ...app.backup, busy: true, bytes: 0, error: '' }
  try {
    app.backup.last = await api.backupRun(stamp(), (n) => (app.backup.bytes = n))
  } catch (e) {
    app.backup.error = errText(e)
  } finally {
    app.backup.busy = false
  }
}

/** Once a day, the first time the Kindle shows up. */
export async function maybeAutoBackup() {
  const s = app.settings
  if (!s?.auto_backup || !app.status || s.last_auto_backup === today()) return
  // one attempt per day; a failed run shows its error and waits for a manual retry
  await saveSettings({ last_auto_backup: today() })
  await runBackup()
}
