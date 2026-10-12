<script lang="ts">
  import { api, errText, KO } from '../api'
  import { app, refreshStatus } from '../store.svelte'
  import { t } from '../i18n.svelte'

  const files = $derived([
    { name: 'defaults.custom.lua', hint: t('Defaults that KOReader reads at startup. They apply the next time you open it.', 'Valores por defecto que KOReader lee al arrancar. Se aplican la próxima vez que lo abras.') },
    { name: 'settings.reader.lua', hint: t('All KOReader preferences. You can only save it while KOReader is closed, because KOReader overwrites it on exit.', 'Todas las preferencias de KOReader. Solo se puede guardar con KOReader cerrado, porque al cerrar lo sobrescribe.') },
  ])
  let current = $state('defaults.custom.lua')
  let text = $state('')
  let saved = $state('')
  let error = $state('')
  let note = $state('')

  const file = $derived(files.find((f) => f.name === current)!)
  const locked = $derived(current === 'settings.reader.lua' && !!app.status?.reader_running)
  const dirty = $derived(text !== saved)

  async function open(name: string) {
    error = note = ''
    current = name
    try { text = saved = await api.readRemote(`${KO}/${name}`) } catch (e) { error = errText(e) }
  }
  async function save() {
    error = note = ''
    try {
      await api.writeRemote(`${KO}/${current}`, text)
      saved = text
      note = t(`Saved. The previous version is in ${current}.kob.bak`, `Guardado. La versión anterior quedó en ${current}.kob.bak`)
    } catch (e) {
      error = errText(e)
    }
  }
  let loaded = false
  $effect(() => { if (app.status && !loaded) { loaded = true; open(current) } })
</script>

<div class="stack">
  <div class="head">
    <div>
      <h1>{t('Settings', 'Ajustes')}</h1>
      <p class="sub">{file.hint}</p>
    </div>
  </div>

  {#if error}<div class="banner bad">{error}</div>{/if}
  {#if note}<div class="banner ok">{note}</div>{/if}
  {#if locked}
    <div class="banner warn">
      <span>{t('KOReader is open, so this file is read-only. Close it on the Kindle and keep the SSH server running.', 'KOReader está abierto, así que este archivo queda en solo lectura. Cerralo en la Kindle con el servidor SSH activo.')}</span>
      <button class="link" onclick={refreshStatus}>{t('Check again', 'Volver a revisar')}</button>
    </div>
  {/if}

  {#if !app.status}
    <div class="empty">{t('Connect the Kindle to edit its settings.', 'Conectá la Kindle para editar su configuración.')}</div>
  {:else}
    <div class="editor">
      <div class="bar">
        <div class="row" role="tablist">
          {#each files as f}
            <button class="btn" class:ghost={f.name !== current} role="tab" aria-selected={f.name === current} onclick={() => open(f.name)}>{f.name}</button>
          {/each}
        </div>
        <button class="btn" disabled={!dirty || locked} onclick={save}>{t('Save', 'Guardar')}</button>
      </div>
      <textarea bind:value={text} readonly={locked} spellcheck="false" aria-label={t(`Contents of ${current}`, `Contenido de ${current}`)}
        placeholder={current === 'defaults.custom.lua' ? t('-- empty: KOReader uses its factory defaults', '-- vacío: KOReader usa sus valores de fábrica') : ''}></textarea>
    </div>
  {/if}
</div>

<style>
  .editor { height: calc(100vh - 230px); min-height: 360px }
</style>
