<script lang="ts">
  import { api, errText, KO, type Entry } from '../api'
  import { app } from '../store.svelte'

  const DIR = `${KO}/patches`
  const TEMPLATE = `-- KOReader user patch. Los archivos 2-*.lua corren después de cargar la UI.
-- Guía: https://github.com/koreader/koreader/wiki/User-patches
local logger = require("logger")
logger.info("patch cargado")
`

  let patches = $state<Entry[]>([])
  let current = $state('')
  let text = $state('')
  let saved = $state('')
  let error = $state('')
  let changed = $state(false)
  let creating = $state(false)
  let newName = $state('2-')
  let confirming = $state(false)

  const enabled = (n: string) => n.endsWith('.lua')
  const base = (n: string) => n.replace(/\.disabled$/, '')
  const dirty = $derived(text !== saved)
  const live = $derived(patches.filter((p) => enabled(p.name)).length)

  async function guard(f: () => Promise<void>) {
    error = ''
    try { await f() } catch (e) { error = errText(e) }
  }
  const load = () => guard(async () => {
    patches = (await api.listDir('patches')).filter((p) => /\.lua(\.disabled)?$/.test(p.name))
    if (!patches.some((p) => p.name === current)) select(patches[0]?.name ?? '')
  })
  let loaded = false
  $effect(() => { if (app.status && !loaded) { loaded = true; load() } })

  const select = (name: string) => guard(async () => {
    current = name
    confirming = false
    text = saved = name ? await api.readRemote(`${DIR}/${name}`) : ''
  })
  const save = () => guard(async () => {
    await api.writeRemote(`${DIR}/${current}`, text)
    saved = text
    changed = true
  })
  const toggle = (name: string) => guard(async () => {
    const to = enabled(name) ? `${name}.disabled` : base(name)
    await api.renameRemote(`${DIR}/${name}`, `${DIR}/${to}`)
    if (current === name) current = to
    changed = true
    await load()
  })
  const create = () => guard(async () => {
    let name = newName.trim().replace(/\.lua$/, '')
    if (!/^[\w.-]+$/.test(name)) throw 'Usá solo letras, números, guiones y puntos en el nombre.'
    name += '.lua'
    await api.writeRemote(`${DIR}/${name}`, TEMPLATE)
    creating = false
    newName = '2-'
    changed = true
    await load()
    await select(name)
  })
  const remove = () => guard(async () => {
    await api.deleteRemote(`${DIR}/${current}`)
    changed = true
    current = ''
    await load()
  })
</script>

<div class="stack">
  <div class="head">
    <div>
      <h1>Patches</h1>
      <p class="sub num"><code>koreader/patches/</code> · {live} activos de {patches.length}</p>
    </div>
    <button class="btn ghost" disabled={!app.status} onclick={() => (creating = !creating)}>Nuevo patch</button>
  </div>

  {#if error}<div class="banner bad">{error}</div>{/if}
  {#if changed}<div class="banner warn">Reiniciá KOReader en la Kindle para aplicar los cambios.</div>{/if}

  {#if creating}
    <form class="field" onsubmit={(e) => { e.preventDefault(); create() }}>
      <input bind:value={newName} placeholder="2-mi-patch" aria-label="Nombre del patch" spellcheck="false" />
      <button class="btn">Crear</button>
    </form>
  {/if}

  {#if !app.status}
    <div class="empty">Conectá la Kindle para ver sus patches.</div>
  {:else}
    <div class="split">
      <div class="list">
        {#each patches as p (p.name)}
          <div class="item" class:on={p.name === current}>
            <button class="pick grow ellipsis" onclick={() => select(p.name)}><code>{base(p.name)}</code></button>
            <input type="checkbox" class="tog" checked={enabled(p.name)} onchange={() => toggle(p.name)} aria-label="Activar {base(p.name)}" />
          </div>
        {:else}
          <div class="item sub">No hay patches.</div>
        {/each}
      </div>
      {#if current}
        <div class="editor">
          <div class="bar">
            <code class="ellipsis">{current}</code>
            <div class="row">
              {#if confirming}
                <button class="btn danger" onclick={remove}>Borrar de la Kindle</button>
                <button class="btn ghost" onclick={() => (confirming = false)}>Cancelar</button>
              {:else}
                <button class="btn ghost" onclick={() => (confirming = true)}>Borrar</button>
              {/if}
              <button class="btn" disabled={!dirty} onclick={save}>Guardar</button>
            </div>
          </div>
          <textarea bind:value={text} spellcheck="false" aria-label="Código del patch"></textarea>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .split { display: grid; grid-template-columns: minmax(0, 260px) minmax(0, 1fr); gap: 16px; align-items: start }
  .item { padding: 6px 12px 6px 6px }
  .item.on { background: var(--paper) }
  .pick { appearance: none; border: 0; background: none; color: var(--ink); text-align: left; padding: 6px; border-radius: 5px }
  .editor { height: calc(100vh - 210px); min-height: 360px }
</style>
