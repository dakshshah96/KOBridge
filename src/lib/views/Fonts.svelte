<script lang="ts">
  import Dropzone from '../Dropzone.svelte'
  import { api, errText, fmtBytes, type Entry } from '../api'
  import { app } from '../store.svelte'
  import { t } from '../i18n.svelte'

  let fonts = $state<Entry[]>([])
  let error = $state('')
  let confirming = $state('')

  const load = async () => {
    try { fonts = (await api.listDir('fonts')).filter((f) => /\.(ttf|otf)$/i.test(f.name)) } catch (e) { error = errText(e) }
  }
  let loaded = false
  $effect(() => { if (app.status && !loaded) { loaded = true; load() } })

  async function remove(name: string) {
    try { await api.deleteRemote(`/mnt/us/fonts/${name}`); confirming = ''; load() } catch (e) { error = errText(e) }
  }
</script>

<div class="stack">
  <div class="head">
    <div>
      <h1>{t('Fonts', 'Fuentes')}</h1>
      <p class="sub"><code>/mnt/us/fonts/</code> · {t('they show up in the KOReader font menu after a restart', 'aparecen en el menú de fuentes de KOReader al reiniciarlo')}</p>
    </div>
  </div>
  {#if error}<div class="banner bad">{error}</div>{/if}

  <Dropzone kind="fonts" title={t('.ttf or .otf files', 'archivos .ttf u .otf')} exts={['ttf', 'otf']} onsent={load} />

  {#if fonts.length}
    <div>
      <p class="label num" style="margin-bottom:8px">{t('On the Kindle', 'En la Kindle')} · {fonts.length}</p>
      <div class="list num">
        {#each fonts as f (f.name)}
          <div class="item">
            <span class="grow ellipsis">{f.name.replace(/\.(ttf|otf)$/i, '')}</span>
            <span class="pill">{fmtBytes(f.size)}</span>
            {#if confirming === f.name}
              <button class="btn danger" onclick={() => remove(f.name)}>{t('Delete from the Kindle', 'Borrar de la Kindle')}</button>
              <button class="btn ghost" onclick={() => (confirming = '')}>{t('Cancel', 'Cancelar')}</button>
            {:else}
              <button class="btn ghost" onclick={() => (confirming = f.name)}>{t('Remove', 'Quitar')}</button>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>
