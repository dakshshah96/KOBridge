<script lang="ts">
  import { onDestroy, tick } from 'svelte'
  import { api, errText } from '../api'
  import { app } from '../store.svelte'

  const MAX = 2000
  let lines = $state<string[]>([])
  let following = $state(false)
  let filter = $state('')
  let error = $state('')
  let box: HTMLPreElement
  let stick = true

  async function start() {
    error = ''
    lines = []
    try {
      await api.logStart(async (l) => {
        lines.push(l)
        if (lines.length > MAX) lines.splice(0, lines.length - MAX)
        if (stick) { await tick(); box?.scrollTo(0, box.scrollHeight) }
      })
      following = true
    } catch (e) {
      error = errText(e)
    }
  }
  const stop = () => { following = false; api.logStop() }
  let started = false
  $effect(() => { if (app.status && !started) { started = true; start() } })
  onDestroy(() => api.logStop())

  const shown = $derived(filter ? lines.filter((l) => l.toLowerCase().includes(filter.toLowerCase())) : lines)
  const level = (l: string) => (/ (ERROR|FATAL)\b|stack traceback/.test(l) ? 'err' : / WARN\b/.test(l) ? 'warn' : '')

  let msg = $state('')
  let sent = $state('')
  async function send() {
    sent = error = ''
    try { await api.screenMessage(msg); sent = 'Mensaje enviado a la pantalla.' } catch (e) { error = errText(e) }
  }
</script>

<div class="stack">
  <div class="head">
    <div>
      <h1>Registro</h1>
      <p class="sub"><code>koreader/crash.log</code> en vivo</p>
    </div>
    <div class="row">
      <span class="pill" class:ok={following}>{following ? 'Siguiendo' : 'Detenido'}</span>
      {#if following}<button class="btn ghost" onclick={stop}>Pausar</button>
      {:else}<button class="btn ghost" disabled={!app.status} onclick={start}>Seguir</button>{/if}
    </div>
  </div>

  {#if error}<div class="banner bad">{error}</div>{/if}

  <div class="field"><input bind:value={filter} placeholder="Filtrar, por ejemplo: Storefront o ERROR" aria-label="Filtrar el registro" spellcheck="false" /></div>
  <pre class="log num selectable" bind:this={box} onscroll={() => (stick = box.scrollTop + box.clientHeight >= box.scrollHeight - 8)}>{#each shown as l}<span class={level(l)}>{l}</span>
{/each}</pre>

  <div>
    <p class="label" style="margin-bottom:8px">Mensaje en la pantalla de la Kindle</p>
    <form class="field" onsubmit={(e) => { e.preventDefault(); send() }}>
      <input bind:value={msg} maxlength="200" placeholder="Escribí algo para mostrar con fbink" aria-label="Mensaje" />
      <button class="btn" disabled={!app.status || !msg.trim()}>Mostrar</button>
    </form>
    {#if sent}<p class="sub">{sent} KOReader lo tapa en el siguiente refresco de pantalla.</p>{/if}
  </div>
</div>

<style>
  .log { margin: 0; height: calc(100vh - 330px); min-height: 220px; overflow: auto; padding: 12px 14px; font: 11.5px/1.55 var(--mono); background: var(--paper); border: 1px solid var(--rule); border-radius: 8px; white-space: pre-wrap; word-break: break-word }
  .err { color: var(--bad) }
  .warn { color: var(--warn) }
</style>
