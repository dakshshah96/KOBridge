<script lang="ts">
  import { onMount } from 'svelte'
  import { open } from '@tauri-apps/plugin-dialog'
  import Loading from '../Loading.svelte'
  import { api, errText, fmtWhen, today, type HighlightSet, type HBook, type Model, type EnrichEvent } from '../api'
  import { app, saveSettings } from '../store.svelte'

  let set = $state<HighlightSet>({ books: [], errors: [], fetched_at: 0 })
  let selected = $state(0)
  let phase = $state<'cache' | 'fetching' | 'idle'>('cache')
  let exporting = $state(false)
  let fresh = $state('')
  let msg = $state<{ kind: 'ok' | 'bad'; text: string; reveal?: boolean; anki?: string } | null>(null)

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

  let vaults = $state<string[]>([])
  onMount(async () => {
    api.obsidianVaults().then((v) => (vaults = v)).catch(() => {})
    set = await api.highlights(false).catch(() => set)
    phase = 'idle'
  })
  // read the Kindle once per visit, as soon as it is reachable
  let fetched = false
  $effect(() => { if (app.status && phase === 'idle' && !fetched) { fetched = true; refresh() } })

  async function exportAll() {
    exporting = true
    try {
      const r = await api.highlightsExport(set.books)
      const parts = [`${r.created} notas nuevas`, `${r.updated} actualizadas`, `${r.added} subrayados agregados`]
      const legacy = r.legacy ? ` Renombré ${r.legacy} notas de la versión anterior a "(anterior)".` : ''
      msg = { kind: 'ok', text: `Exporté a ${r.dir}: ${parts.join(', ')}.${legacy}`, reveal: true }
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
  async function onVault(e: Event & { currentTarget: HTMLSelectElement }) {
    const v = e.currentTarget.value
    e.currentTarget.value = app.settings?.vault_dir ?? ''
    if (v === '__other') await pickVault()
    else await saveSettings({ vault_dir: v })
  }
  async function createVault() {
    try {
      const v = await api.obsidianCreateVault()
      vaults = [...vaults, v]
      await saveSettings({ vault_dir: `${v}/KOReader` })
    } catch (e) {
      msg = { kind: 'bad', text: errText(e) }
    }
  }
  const tail = (p: string) => p.split('/').slice(-2).join(' / ')
  const vaultOptions = $derived.by(() => {
    const opts = vaults.map((v) => ({ value: `${v}/KOReader`, label: `${v.split('/').pop()} / KOReader` }))
    const cur = app.settings?.vault_dir
    return cur && !opts.some((o) => o.value === cur) ? [{ value: cur, label: tail(cur) }, ...opts] : opts
  })

  // ---- Claude ----
  const MODELS: { id: Model; name: string; hint: string }[] = [
    { id: 'haiku', name: 'Haiku', hint: 'más rápido' },
    { id: 'sonnet', name: 'Sonnet', hint: 'equilibrado' },
    { id: 'opus', name: 'Opus', hint: 'más profundo' },
    { id: 'fable', name: 'Fable', hint: 'el más nuevo' },
  ]
  const model = $derived<Model>(app.settings?.ai_model || 'sonnet')
  let picked = $state<string[]>([])
  const toggle = (id: string) => (picked = picked.includes(id) ? picked.filter((x) => x !== id) : [...picked, id])

  type Job = { title: string; state: 'waiting' | 'running' | 'done' | 'error'; text: string }
  let jobs = $state<Job[]>([])
  let running = $state<'' | 'enrich' | 'anki'>('')

  async function run(kind: 'enrich' | 'anki') {
    const books: HBook[] = picked.length ? set.books.filter((b) => picked.includes(b.sidecar)) : book ? [book] : []
    if (!books.length) return
    jobs = books.map((b) => ({ title: b.title, state: 'waiting', text: '' }))
    running = kind
    msg = null
    const onEvent = (ev: EnrichEvent) => {
      const j = jobs.find((j) => j.title === ev.book && (j.state === 'waiting' || j.state === 'running'))
      if (!j) return
      if (ev.kind === 'start') { j.state = 'running'; j.text = kind === 'anki' ? 'Escribiendo tarjetas' : 'Abriendo la nota' }
      else if (ev.kind === 'tool') j.text = ev.text
      else if (ev.kind === 'done') { j.state = 'done'; j.text = ev.text }
      else { j.state = 'error'; j.text = ev.text }
    }
    try {
      if (kind === 'enrich') await api.enrichStart(books, model, today(), onEvent)
      else {
        const file = await api.ankiStart(books, model, today(), onEvent)
        if (file) msg = { kind: 'ok', text: `Guardé las tarjetas en ${file}. Anki las importa al mazo KOReader y actualiza las que ya tenías.`, anki: file }
      }
    } catch (e) {
      msg = { kind: 'bad', text: errText(e) }
    } finally {
      for (const j of jobs) if (j.state === 'waiting' || j.state === 'running') { j.state = 'error'; j.text = 'Cancelado' }
      running = ''
    }
  }
  const jobLabel = { waiting: 'En cola', running: 'Trabajando…', done: 'Listo', error: 'Error' }

  const book = $derived(set.books[selected])
  const waitingForKindle = $derived(!set.books.length && phase === 'idle' && !app.status && app.checking)
</script>

<div class="stack">
  <div class="head">
    <div>
      <h1>Subrayados</h1>
      <p class="sub num">
        {#if set.books.length}{set.books.length} libros · {count(set)} subrayados ·&nbsp;{/if}destino
        <select class="vault" value={app.settings?.vault_dir} onchange={onVault} title={app.settings?.vault_dir} aria-label="Carpeta de Obsidian">
          {#each vaultOptions as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
          <option value="__other">Otra carpeta…</option>
        </select>
        {#if !vaults.length}<button class="link" onclick={createVault}>Crear bóveda</button>{/if}
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
        {#if msg.anki}{@const f = msg.anki}<button class="link" onclick={() => api.ankiOpen(f).catch((e) => (msg = { kind: 'bad', text: errText(e) }))}>Abrir en Anki</button>
          <button class="link" onclick={() => api.reveal(f)}>Mostrar en Finder</button>{/if}
        {#if msg.reveal && app.settings}<button class="link" onclick={() => api.reveal(app.settings!.vault_dir)}>Mostrar en Finder</button>{/if}
        <button class="link" onclick={() => (msg = null)}>Cerrar</button>
      </span>
    </div>
  {/if}
  {#if set.books.length}
    <div class="claude">
      <div class="intro">
        <p class="label">Claude</p>
        <p class="sub">Enriquecer agrega a la nota un resumen, etiquetas y enlaces a tus otras notas. Tarjetas escribe preguntas y respuestas para Anki.</p>
      </div>
      <div class="models" role="radiogroup" aria-label="Modelo de Claude">
        {#each MODELS as m (m.id)}
          <button class="model" role="radio" aria-checked={model === m.id} disabled={!!running} onclick={() => saveSettings({ ai_model: m.id })}>
            <strong>{m.name}</strong><span>{m.hint}</span>
          </button>
        {/each}
      </div>
      <div class="row">
        {#if running}<button class="btn ghost" onclick={() => api.enrichStop()}>Cancelar</button>{/if}
        <button class="btn ghost" disabled={!!running || (!picked.length && !book)} onclick={() => run('anki')}>
          {running === 'anki' ? 'Escribiendo…' : 'Tarjetas Anki'}
        </button>
        <button class="btn" disabled={!!running || (!picked.length && !book)} onclick={() => run('enrich')}>
          {running === 'enrich' ? 'Enriqueciendo…' : picked.length ? `Enriquecer ${picked.length} ${picked.length === 1 ? 'libro' : 'libros'}` : 'Enriquecer este libro'}
        </button>
      </div>
    </div>
    {#if jobs.length}
      <div class="list num">
        {#each jobs as j}
          <div class="item">
            <span class="grow ellipsis">{j.title}</span>
            {#if j.text}<span class="sub ellipsis" style="max-width:45%; margin:0" title={j.text}>{j.text}</span>{/if}
            <span class="pill" class:ok={j.state === 'done'} class:bad={j.state === 'error'}>{jobLabel[j.state]}</span>
          </div>
        {/each}
      </div>
    {/if}
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
          <div class="book" aria-current={i === selected}>
            <input type="checkbox" checked={picked.includes(b.sidecar)} onchange={() => toggle(b.sidecar)} aria-label={`Elegir ${b.title}`} />
            <button onclick={() => (selected = i)}><span class="ellipsis">{b.title}</span><span class="n num">{b.notes.length}</span></button>
          </div>
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
  .book { display: flex; align-items: center; gap: 8px; padding-left: 8px; min-width: 0; border-radius: 6px; color: var(--ink-2) }
  .book input { margin: 0; accent-color: var(--ink); flex: none }
  .book button { appearance: none; border: 0; background: none; font: 14px/1.3 var(--serif); color: inherit; padding: 8px 10px 8px 0; display: flex; justify-content: space-between; gap: 10px; text-align: left; align-items: center; flex: 1; min-width: 0 }
  .book button .ellipsis { flex: 1; min-width: 0 }
  .book:hover { color: var(--ink); background: var(--paper) }
  .book[aria-current='true'] { color: var(--ink); background: var(--rule) }
  .n { font: 12px var(--sans); color: var(--ink-2) }
  .vault { font: inherit; color: var(--ink); background: transparent; border: 1px solid var(--rule); border-radius: 6px; padding: 2px 6px; max-width: 280px }
  .claude { display: flex; flex-wrap: wrap; align-items: center; gap: 12px 20px; border: 1px solid var(--rule); border-radius: 8px; padding: 14px 16px }
  .intro { flex: 1 1 200px; min-width: 0 }
  .intro .sub { margin-top: 4px; font-size: 12.5px }
  .models { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); padding: 3px; gap: 3px; border: 1px solid var(--rule); border-radius: 9px; background: var(--paper) }
  .model { appearance: none; border: 0; background: none; border-radius: 6px; padding: 7px 12px; display: grid; gap: 2px; text-align: center; color: var(--ink-2); min-width: 78px }
  .model strong { font: 600 13px/1.2 var(--sans) }
  .model span { font: 11px/1.2 var(--sans) }
  .model:hover:not(:disabled) { color: var(--ink) }
  .model[aria-checked='true'] { background: var(--sheet); color: var(--ink); box-shadow: 0 1px 2px rgb(0 0 0 / .12) }
  .model:disabled { opacity: .6 }
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
