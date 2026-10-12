<script lang="ts">
  import { onMount } from 'svelte'
  import { open } from '@tauri-apps/plugin-dialog'
  import { api, errText, fmtBytes, fmtDuration, fmtWhen, type BackupFile, type BackupSize } from '../api'
  import { app, runBackup, saveSettings } from '../store.svelte'
  import { t, dec } from '../i18n.svelte'

  let files = $state<BackupFile[]>([])
  const load = async () => (files = await api.backupList())
  onMount(load)

  // refresh the history whenever a run (manual or automatic) finishes
  $effect(() => { if (app.backup.last) load() })

  async function pickDir() {
    const dir = await open({ directory: true, defaultPath: app.settings?.backup_dir })
    if (typeof dir === 'string') { await saveSettings({ backup_dir: dir }); load() }
  }
  const s = $derived(app.settings)

  let size = $state<BackupSize | null>(null)
  let measuring = $state(false)
  let sizeError = $state('')
  async function measure() {
    measuring = true
    sizeError = ''
    try { size = await api.backupSize() } catch (e) { sizeError = errText(e) } finally { measuring = false }
  }
  const mbps = (b: number) => dec(b / 1024 / 1024, 2)
</script>

<div class="stack">
  <div class="head">
    <div>
      <h1>{t('Backup', 'Respaldo')}</h1>
      <p class="sub num">
        {#if files[0]}{t('Last backup', 'Último respaldo')} {fmtWhen(files[0].modified)} · {fmtBytes(files[0].size)}{:else}{t('No backups yet', 'Todavía no hay respaldos')}{/if}
      </p>
    </div>
    <button class="btn" disabled={!app.status || app.backup.busy} onclick={runBackup}>
      {app.backup.busy ? `${t('Backing up…', 'Respaldando…')} ${fmtBytes(app.backup.bytes)}` : t('Back up now', 'Respaldar ahora')}
    </button>
  </div>

  {#if app.backup.error}<div class="banner bad">{app.backup.error}</div>{/if}

  {#if s}
    <div class="list">
      <div class="item">
        <div class="grow"><div>{t('Settings, statistics, patches and highlights', 'Ajustes, estadísticas, patches y subrayados')}</div><div class="sub">{t('koreader/settings, settings.reader.lua and every .sdr folder', 'koreader/settings, settings.reader.lua y cada carpeta .sdr')}</div></div>
        <span class="pill">{t('always', 'siempre')}</span>
      </div>
      <div class="item">
        <div class="grow">
          <div>{t('Books', 'Libros')}</div>
          {#if size}
            <div class="sub num">
              {size.books_files} {t('files', 'archivos')} · {fmtBytes(size.books_bytes)} {t('in', 'en')} /mnt/us/books<br />
              {t('Full backup', 'Respaldo completo')} {fmtBytes(size.full_bytes)} ≈ {fmtDuration(size.full_bytes / size.bytes_per_sec)}
              · {t('without books', 'sin libros')} {fmtBytes(size.base_bytes)} ≈ {fmtDuration(size.base_bytes / size.bytes_per_sec)}
              · {t('measured at', 'medido a')} {mbps(size.bytes_per_sec)} MB/s
            </div>
          {:else if sizeError}
            <div class="sub" style="color:var(--bad)">{sizeError}</div>
          {:else}
            <div class="sub">{t('All of /mnt/us/books', '/mnt/us/books completo')}</div>
          {/if}
        </div>
        <button class="btn ghost" disabled={!app.status || measuring || app.backup.busy} onclick={measure}>
          {measuring ? t('Measuring…', 'Midiendo…') : size ? t('Measure again', 'Volver a medir') : t('Measure size', 'Calcular tamaño')}
        </button>
        <input type="checkbox" class="tog" checked={s.backup_books} aria-label={t('Include books', 'Incluir libros')}
          onchange={(e) => saveSettings({ backup_books: e.currentTarget.checked })} />
      </div>
      <label class="item">
        <div class="grow"><div>{t('Back up on connect', 'Respaldar al conectar')}</div><div class="sub">{t('Once a day, the first time the Kindle shows up on the network', 'Una vez al día, la primera vez que la Kindle aparece en la red')}</div></div>
        <input type="checkbox" class="tog" checked={s.auto_backup} onchange={(e) => saveSettings({ auto_backup: e.currentTarget.checked })} />
      </label>
      <div class="item">
        <div class="grow"><div>{t('Folder', 'Carpeta')}</div><code class="sub ellipsis selectable">{s.backup_dir}</code></div>
        <button class="btn ghost" onclick={pickDir}>{t('Change…', 'Cambiar…')}</button>
      </div>
    </div>
  {/if}

  {#if files.length}
    <div>
      <p class="label" style="margin-bottom:8px">{t('History', 'Historial')}</p>
      <div class="list">
        {#each files as f (f.path)}
          <div class="item num">
            <span class="grow">{f.name}</span>
            <span class="sub">{fmtWhen(f.modified)}</span>
            <span class="pill">{fmtBytes(f.size)}</span>
            <button class="btn ghost" onclick={() => api.reveal(f.path)}>{t('Show in Finder', 'Mostrar en Finder')}</button>
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>
