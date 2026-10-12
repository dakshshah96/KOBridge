<script lang="ts">
  import Dropzone from '../Dropzone.svelte'
  import { app, refreshStatus } from '../store.svelte'
  import { t, dec } from '../i18n.svelte'

  const gb = (kb: number) => dec(kb / 1024 / 1024)
</script>

<div class="stack">
  <div class="head">
    <div>
      <h1>{t('Send books', 'Enviar libros')}</h1>
      <p class="sub">{t('Destination', 'Destino')} <code>/mnt/us/books/</code>{#if app.status}{' · '}{gb(app.status.free_kb)} {t('GB free', 'GB libres')}{/if}</p>
    </div>
  </div>
  <Dropzone kind="books" title={t('EPUB, PDF, MOBI or CBZ', 'EPUB, PDF, MOBI o CBZ')} exts={['epub', 'pdf', 'mobi', 'azw3', 'fb2', 'cbz', 'djvu', 'txt']} onsent={refreshStatus} />
  <p class="sub">{t('KOReader shows them in its file browser when you open the folder again.', 'KOReader los muestra en el explorador de archivos al volver a abrir la carpeta.')}</p>
</div>
