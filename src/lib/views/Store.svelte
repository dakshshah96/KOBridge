<script lang="ts">
  import { api, errText, fmtBytes, today, type Catalog, type Owned, type StoreItem, type StoreKind, type StoreStep } from '../api'
  import { app, refreshStatus } from '../store.svelte'
  import { t, dec } from '../i18n.svelte'

  const PAGE = 150
  let catalog = $state.raw<Catalog | null>(null)
  let owned = $state.raw<Map<string, Owned>>(new Map())
  let registryError = $state('')
  let tab = $state<StoreKind>('plugin')
  let query = $state('')
  let sort = $state<'stars' | 'date'>('stars')
  let onlyInstalled = $state(false)
  let shown = $state(PAGE)
  let loading = $state(false)
  let error = $state('')
  let done = $state('')
  let job = $state<{ id: string; step: StoreStep } | null>(null)
  let confirming = $state('')

  const key = (kind: StoreKind, id: string) => `${kind}:${id}`
  const tabs = $derived([
    { id: 'plugin' as const, label: 'Plugins', items: catalog?.plugins ?? [] },
    { id: 'patch' as const, label: 'Patches', items: catalog?.patches ?? [] },
    { id: 'font' as const, label: t('Fonts', 'Fuentes'), items: catalog?.fonts ?? [] },
  ])

  async function loadCatalog(refresh: boolean) {
    loading = true
    error = ''
    try { catalog = await api.storeCatalog(refresh) } catch (e) { error = errText(e) } finally { loading = false }
    if (catalog && app.status) loadInstalled()
  }
  /** Keeps an install error on screen: the refresh after a failed run often fails the same way. */
  async function loadInstalled() {
    try {
      const s = await api.storeInstalled()
      owned = new Map(s.items.map((o) => [key(o.kind, o.id), o]))
      registryError = s.registry_error ?? ''
    } catch (e) { error ||= errText(e) }
  }
  let started = false
  $effect(() => { if (!started) { started = true; loadCatalog(false) } })
  // the Kindle showed up after the catalog loaded
  let hadStatus = false
  $effect(() => { const on = !!app.status; if (on && !hadStatus && catalog) loadInstalled(); hadStatus = on })

  const list = $derived.by(() => {
    const items = tabs.find((x) => x.id === tab)!.items
    const q = query.trim().toLowerCase()
    const v = items.filter((i) =>
      (!onlyInstalled || owned.has(key(tab, i.id))) &&
      (!q || i.name.toLowerCase().includes(q) || i.owner.toLowerCase().includes(q) || i.description.toLowerCase().includes(q)))
    return sort === 'stars' ? [...v].sort((a, b) => b.stars - a.stars) : [...v].sort((a, b) => b.pushed_at.localeCompare(a.pushed_at))
  })
  $effect(() => { void tab; void query; void onlyInstalled; void sort; shown = PAGE })
  const updates = $derived([...owned.values()].filter((o) => o.kind === tab && o.update).length)

  const stamp = () => {
    const d = new Date()
    const p = (n: number) => String(n).padStart(2, '0')
    return `${today()}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`
  }
  async function run(op: 'install' | 'uninstall', item: StoreItem) {
    error = ''
    done = ''
    confirming = ''
    job = { id: item.id, step: { step: 'backup', done: 0, total: 0 } }
    try {
      await api.storeRun(op, tab, item.id, stamp(), (s) => { if (job) job = { ...job, step: s } })
      done = op === 'install'
        ? t(`Installed ${item.name}. Restart KOReader to load it.`, `Instalé ${item.name}. Reiniciá KOReader para cargarlo.`)
        : t(`Removed ${item.name}. Restart KOReader to finish.`, `Quité ${item.name}. Reiniciá KOReader para terminar.`)
    } catch (e) {
      error = errText(e)
    } finally {
      job = null
      await loadInstalled()
      refreshStatus()
    }
  }

  const stepText = (s: StoreStep) =>
    s.step === 'backup' ? t(`Backing up, ${fmtBytes(s.done)}`, `Respaldando, ${fmtBytes(s.done)}`)
    : s.step === 'download' ? t(`Downloading, ${fmtBytes(s.done)}`, `Bajando, ${fmtBytes(s.done)}`) + (s.total ? ` / ${fmtBytes(s.total)}` : '')
    : s.step === 'send' ? t('Sending to the Kindle', 'Enviando a la Kindle')
    : t('Done', 'Listo')
  const stars = (n: number) => (n >= 1000 ? `${dec(n / 1000)}k` : String(n))
  const blocked = $derived(!app.status || app.status.reader_running || app.backup.busy || !!job)
</script>

<div class="stack">
  <div class="head">
    <div>
      <h1>{t('Store', 'Tienda')}</h1>
      <p class="sub num">
        {t('Storefront catalog', 'Catálogo de Storefront')}{#if catalog}{' · '}{catalog.plugins.length} plugins · {catalog.patches.length} patches · {catalog.fonts.length} {t('fonts', 'fuentes')}{/if}
      </p>
    </div>
    <button class="btn ghost" disabled={loading} onclick={() => loadCatalog(true)}>{loading ? t('Updating…', 'Actualizando…') : t('Update catalog', 'Actualizar catálogo')}</button>
  </div>

  {#if app.status?.reader_running}<div class="banner warn">{t('KOReader is open. Close it on the Kindle to install or remove anything, because it rewrites Storefront\'s list of installs when it exits.', 'KOReader está abierto. Cerralo en la Kindle para instalar o quitar algo, porque reescribe la lista de instalaciones de Storefront al cerrar.')}</div>{/if}
  {#if !app.status}<div class="banner warn">{t('Connect the Kindle to see what is installed.', 'Conectá la Kindle para ver qué está instalado.')}</div>{/if}
  {#if catalog?.offline}<div class="banner warn"><span class="selectable">{t(`Showing the catalog from ${catalog.generated_at.slice(0, 10)}, because the download failed: ${catalog.offline}`, `Muestro el catálogo del ${catalog.generated_at.slice(0, 10)}, porque falló la descarga: ${catalog.offline}`)}</span></div>{/if}
  {#if registryError}<div class="banner warn"><span class="selectable">{t(`Storefront_installs.lua couldn't be read, so updates aren't shown: ${registryError}`, `No pude leer Storefront_installs.lua, así que no muestro actualizaciones: ${registryError}`)}</span></div>{/if}
  {#if error}<div class="banner bad"><span class="selectable">{error}</span><button class="link" onclick={() => (error = '')}>{t('Close', 'Cerrar')}</button></div>{/if}
  {#if done}<div class="banner ok"><span>{done}</span><button class="link" onclick={() => (done = '')}>{t('Close', 'Cerrar')}</button></div>{/if}

  <div class="bar">
    <div class="seg" role="tablist">
      {#each tabs as x (x.id)}
        <button role="tab" aria-selected={tab === x.id} onclick={() => (tab = x.id)}>{x.label}</button>
      {/each}
    </div>
    <div class="field grow"><input bind:value={query} placeholder={t('Search by name, author or description', 'Buscar por nombre, autor o descripción')} aria-label={t('Search', 'Buscar')} spellcheck="false" /></div>
    <div class="seg" role="radiogroup" aria-label={t('Sort', 'Orden')}>
      <button role="radio" aria-checked={sort === 'stars'} onclick={() => (sort = 'stars')}>{t('Stars', 'Estrellas')}</button>
      <button role="radio" aria-checked={sort === 'date'} onclick={() => (sort = 'date')}>{t('Updated', 'Actualizados')}</button>
    </div>
    <button class="btn ghost" aria-pressed={onlyInstalled} class:on={onlyInstalled} onclick={() => (onlyInstalled = !onlyInstalled)}>
      {t('Installed', 'Instalados')}{#if updates}{' · '}{updates} {t(updates === 1 ? 'update' : 'updates', updates === 1 ? 'actualización' : 'actualizaciones')}{/if}
    </button>
  </div>

  {#if !catalog && loading}
    <p class="sub">{t('Downloading the catalog…', 'Bajando el catálogo…')}</p>
  {:else if catalog && !list.length}
    <p class="sub">{t('Nothing matches.', 'No hay resultados.')}</p>
  {:else if catalog}
    <div class="list">
      {#each list.slice(0, shown) as item (item.id)}
        {@const o = owned.get(key(tab, item.id))}
        <div class="item" class:busy={job?.id === item.id}>
          <div class="grow info">
            <div class="title">
              <button class="name" title={item.url} onclick={() => api.openUrl(item.url).catch((e) => (error = errText(e)))}>{item.name}</button>
              <span class="sub">{item.owner}{#if tab === 'patch' && item.repo} · {item.repo}{/if}</span>
            </div>
            {#if item.description}<div class="sub ellipsis">{item.description}</div>{/if}
            {#if job?.id === item.id}
              <div class="sub num">{stepText(job.step)}</div>
              <progress max={job.step.total || undefined} value={job.step.total ? job.step.done : undefined}></progress>
            {/if}
          </div>
          <span class="pill num" title={t('GitHub stars', 'Estrellas en GitHub')}>★ {stars(item.stars)}</span>
          {#if o?.version}<span class="pill num">{o.version}</span>{/if}
          {#if confirming === item.id}
            <button class="btn danger" disabled={blocked} onclick={() => run('uninstall', item)}>{t('Remove from the Kindle', 'Quitar de la Kindle')}</button>
            <button class="btn ghost" onclick={() => (confirming = '')}>{t('Cancel', 'Cancelar')}</button>
          {:else if o}
            {#if o.update}<button class="btn" disabled={blocked} onclick={() => run('install', item)}>{t('Update', 'Actualizar')}</button>{/if}
            {#if o.removable}<button class="btn ghost" disabled={blocked} onclick={() => (confirming = item.id)}>{t('Remove', 'Quitar')}</button>
            {:else if !o.update}<button class="btn ghost" disabled={blocked} onclick={() => run('install', item)} title={t('Installed outside the store', 'Instalado fuera de la tienda')}>{t('Reinstall', 'Reinstalar')}</button>{/if}
          {:else}
            <button class="btn ghost" disabled={blocked} onclick={() => run('install', item)}>{t('Install', 'Instalar')}</button>
          {/if}
        </div>
      {/each}
    </div>
    {#if list.length > shown}
      <button class="btn ghost more num" onclick={() => (shown += PAGE)}>{t(`Show ${Math.min(PAGE, list.length - shown)} more of ${list.length - shown}`, `Mostrar ${Math.min(PAGE, list.length - shown)} más de ${list.length - shown}`)}</button>
    {/if}
    <p class="sub">{t('Every install, update or removal first makes a full backup, which shows up in Backup.', 'Cada instalación, actualización o borrado hace primero un respaldo completo, que aparece en Respaldo.')}</p>
  {/if}
</div>

<style>
  .bar { display: flex; gap: 8px; align-items: center; flex-wrap: wrap }
  .bar .field { min-width: 200px }
  .seg { display: flex; gap: 2px; padding: 2px; border: 1px solid var(--rule); border-radius: 8px; background: var(--paper) }
  .seg button { appearance: none; border: 0; background: none; font: inherit; font-size: 12.5px; color: var(--ink-2); padding: 6px 12px; border-radius: 6px }
  .seg button:hover { color: var(--ink) }
  .seg button[aria-selected='true'], .seg button[aria-checked='true'] { background: var(--sheet); color: var(--ink); box-shadow: 0 1px 2px rgb(0 0 0 / .12) }
  .btn.on { background: var(--paper); border-color: var(--ink-2) }
  .item.busy { background: var(--paper) }
  .info { display: grid; gap: 3px; min-width: 0 }
  .info .sub { margin: 0 }
  .title { display: flex; gap: 8px; align-items: baseline; min-width: 0 }
  .name { appearance: none; border: 0; background: none; padding: 0; font: inherit; color: var(--ink); font-weight: 500; text-align: left; cursor: pointer }
  .name:hover { text-decoration: underline }
  progress { width: 100%; height: 4px; accent-color: var(--ink) }
  .more { justify-self: start }
</style>
