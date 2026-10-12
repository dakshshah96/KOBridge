<script lang="ts">
  import { onMount } from 'svelte'
  import { getCurrentWebview } from '@tauri-apps/api/webview'
  import { open } from '@tauri-apps/plugin-dialog'
  import { api, errText, type Kind } from './api'
  import { app } from './store.svelte'

  let { kind, exts, title, onsent }: { kind: Kind; exts: string[]; title: string; onsent?: () => void } = $props()

  type Job = { path: string; name: string; state: 'waiting' | 'sending' | 'done' | 'error'; msg?: string }
  let jobs = $state<Job[]>([])
  let over = $state(false)
  let running = false

  const ok = (p: string) => exts.includes(p.split('.').pop()?.toLowerCase() ?? '')

  function add(paths: string[]) {
    for (const path of paths) {
      const name = path.split('/').pop()!
      jobs.unshift(ok(path) ? { path, name, state: 'waiting' } : { path, name, state: 'error', msg: 'formato no soportado' })
    }
    pump()
  }

  // one file at a time over the shared ssh connection
  async function pump() {
    if (running) return
    running = true
    let sent = 0
    for (let j = jobs.findLast((x) => x.state === 'waiting'); j; j = jobs.findLast((x) => x.state === 'waiting')) {
      j.state = 'sending'
      try {
        await api.sendFile(j.path, kind)
        j.state = 'done'
        sent++
      } catch (e) {
        j.state = 'error'
        j.msg = errText(e)
      }
    }
    running = false
    if (sent) onsent?.()
  }

  onMount(() => {
    const un = getCurrentWebview().onDragDropEvent((e) => {
      const t = e.payload.type
      over = t === 'enter' || t === 'over'
      if (t === 'drop' && app.status) add(e.payload.paths)
    })
    return () => { un.then((f) => f()) }
  })

  async function choose() {
    const picked = await open({ multiple: true, filters: [{ name: title, extensions: exts }] })
    if (picked) add(Array.isArray(picked) ? picked : [picked])
  }
  const label = { waiting: 'En cola', sending: 'Enviando…', done: 'Enviado', error: 'Error' }
</script>

<div class="drop" class:over>
  {#if app.status}
    <strong>Arrastrá {title} aquí</strong>
    <span>o</span>
    <button class="btn ghost" onclick={choose}>Elegir archivos…</button>
  {:else}
    <strong>Kindle sin conexión</strong>
    <span>Conectala para enviar archivos.</span>
  {/if}
</div>

{#if jobs.length}
  <div class="list num">
    {#each jobs as j}
      <div class="item">
        <span class="grow ellipsis" title={j.path}>{j.name}</span>
        {#if j.msg}<span class="sub ellipsis" style="max-width:40%" title={j.msg}>{j.msg}</span>{/if}
        <span class="pill" class:ok={j.state === 'done'} class:bad={j.state === 'error'}>{label[j.state]}</span>
      </div>
    {/each}
  </div>
{/if}
