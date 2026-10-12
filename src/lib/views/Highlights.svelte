<script lang="ts">
  import { onMount } from 'svelte'
  import { open } from '@tauri-apps/plugin-dialog'
  import Loading from '../Loading.svelte'
  import { api, errText, fmtWhen, type HighlightSet } from '../api'
  import { app, saveSettings } from '../store.svelte'

  let set = $state<HighlightSet>({ books: [], errors: [], fetched_at: 0 })
  let selected = $state(0)
  let phase = $state<'cache' | 'fetching' | 'idle'>('cache')
  let exporting = $state(false)
  let fresh = $state('')
  let msg = $state<{ kind: 'ok' | 'bad'; text: string; reveal?: boolean } | null>(null)

  const count = (s: HighlightSet) => s.books.reduce((n, b) => n + b.notes.length, 0)

  async function refresh() {
    phase = 'fetching'
    msg = null
    const before = set.fetched_at ? count(set) : -1
    try {
      set = await api.highlights(true)
      selected = Math.min(selected, Math.max(0, set.books.length - 1))
      const added = before >= 0 ? count(set) - before : 0
      fresh = added > 0 ? ` · ${added} subrayados nuevos` : ''
    } catch (e) {
      msg = { kind: 'bad', text: errText(e) }
    } finally {
      phase = 'idle'
    }
  }

  onMount(async () => {
    set = await api.highlights(false).catch(() => set)
    phase = 'idle'
  })
  // read the Kindle once per visit, as soon as it is reachable
  let fetched = false
  $effect(() => { if (app.status && phase === 'idle' && !fetched) { fetched = true; refresh() } })

  async function exportAll() {
    exporting = true
    try {
      const n = await api.highlightsExport(set.books)
      msg = { kind: 'ok', text: `Exporté ${n} libros a ${app.settings?.vault_dir}`, reveal: true }
    } catch (e) {
      msg = { kind: 'bad', text: errText(e) }
    } finally {
      exporting = false
    }
  }
  async function pickVault() {
    const dir = await open({ directory: true, defaultPath: app.settings?.vault_dir })
    if (typeof dir === 'string') await saveSettings({ vault_dir: dir })
  }

  const book = $derived(set.books[selected])
  const vaultName = $derived(app.settings?.vault_dir.split('/').slice(-2).join('/') ?? '')
  const waitingForKindle = $derived(!set.books.length && phase === 'idle' && !app.status && app.checking)
</script>

<div class="stack">
  <div class="head">
    <div>
      <h1>Subrayados</h1>
      <p class="sub num">
        {#if set.books.length}{set.books.length} libros · {count(set)} subrayados · {/if}destino
        <button class="link" onclick={pickVault} title={app.settings?.vault_dir}>{vaultName}</button>
      </p>
    </div>
    <div class="row">
      <button class="btn ghost" disabled={!app.status || phase !== 'idle'} onclick={refresh}>
        {phase === 'fetching' ? 'Leyendo…' : 'Actualizar'}
      </button>
      <button class="btn" disabled={!set.books.length || exporting} onclick={exportAll}>
        {exporting ? 'Exportando…' : 'Exportar a Obsidian'}
      </button>
    </div>
  </div>

  {#if msg}
    <div class="banner {msg.kind}">
      <span class="selectable">{msg.text}</span>
      <span class="row">
        {#if msg.reveal && app.settings}<button class="link" onclick={() => api.reveal(app.settings!.vault_dir)}>Mostrar en Finder</button>{/if}
        <button class="link" onclick={() => (msg = null)}>Cerrar</button>
      </span>
    </div>
  {/if}
  {#if set.errors.length}<div class="banner warn">No pude leer {set.errors.length} sidecars: {set.errors[0]}</div>{/if}

  {#if !set.books.length && (phase !== 'idle' || waitingForKindle)}
    <Loading
      title={phase === 'cache' ? 'Abriendo subrayados guardados' : waitingForKindle ? 'Esperando a la Kindle' : 'Leyendo subrayados de la Kindle'}
      detail={phase === 'fetching' ? 'Buscando sidecars con notas en /mnt/us/books y /mnt/us/documents' : 'Primero hay que encontrarla en la red'}
      rows={7}
    />
  {:else if set.books.length}
    {#if phase === 'fetching'}
      <p class="sub refreshing"><span class="mini" aria-hidden="true"></span>Buscando subrayados nuevos en la Kindle…</p>
    {/if}
    <div class="split">
      <div class="books list">
        {#each set.books as b, i (b.sidecar)}
          <button class="book" aria-current={i === selected} onclick={() => (selected = i)}>
            <span class="ellipsis">{b.title}</span><span class="n num">{b.notes.length}</span>
          </button>
        {/each}
      </div>
      {#if book}
        <article class="selectable">
          <h2>{book.title}</h2>
          <p class="meta num">{book.authors}{book.authors ? ' · ' : ''}{Math.round(book.percent * 100)} % leído · {book.notes.length} subrayados</p>
          {#each book.notes as n}
            <section class="quote">
              <p><mark>{n.text}</mark></p>
              <span class="ref num">{[n.chapter, n.page ? `p. ${n.page}` : null, n.datetime?.slice(0, 10)].filter(Boolean).join(' · ')}</span>
              {#if n.note}<p class="note"><span class="label">Nota</span> {n.note}</p>{/if}
            </section>
          {/each}
        </article>
      {/if}
    </div>
    {#if set.fetched_at && phase === 'idle'}<p class="sub num">Leído de la Kindle {fmtWhen(set.fetched_at)}{fresh}</p>{/if}
  {:else}
    <div class="empty">
      {app.status ? 'KOReader no tiene subrayados todavía.' : 'Conectá la Kindle para leer los subrayados de KOReader.'}
    </div>
  {/if}
</div>

<style>
  .split { display: grid; grid-template-columns: minmax(0, 250px) minmax(0, 1fr); gap: 16px; align-items: start }
  .books { padding: 6px; display: grid; gap: 1px; max-height: calc(100vh - 220px); overflow: auto }
  .book { appearance: none; border: 0; background: none; font: 14px/1.3 var(--serif); color: var(--ink-2); padding: 8px 10px; border-radius: 6px; display: flex; justify-content: space-between; gap: 10px; text-align: left; align-items: center }
  .book:hover { color: var(--ink); background: var(--paper) }
  .book[aria-current='true'] { color: var(--ink); background: var(--rule) }
  .n { font: 12px var(--sans); color: var(--ink-2) }
  article { border: 1px solid var(--rule); border-radius: 8px; padding: 24px 28px; font: 15px/1.6 var(--serif); max-height: calc(100vh - 220px); overflow: auto }
  h2 { font: 600 19px/1.2 var(--serif); margin: 0 }
  .meta { font: 12px var(--sans); color: var(--ink-2); margin: 4px 0 24px }
  .quote { margin: 0 0 26px; max-width: 64ch }
  .quote p { margin: 0; white-space: pre-line }
  mark { background: var(--mark); color: inherit; padding: 1px 2px; box-decoration-break: clone; -webkit-box-decoration-break: clone }
  .ref { display: block; font: 11px var(--sans); color: var(--ink-2); margin-top: 8px }
  .note { font: 13px/1.5 var(--sans) !important; margin-top: 8px !important }
  .note .label { margin-right: 6px }
  .refreshing { display: flex; align-items: center; gap: 8px; margin: -8px 0 0 }
  .mini { width: 10px; height: 10px; border-radius: 50%; border: 1.5px solid var(--rule); border-top-color: var(--ink); animation: r 0.8s linear infinite }
  @keyframes r { to { transform: rotate(360deg) } }
  @media (prefers-reduced-motion: reduce) { .mini { animation: none } }
</style>
