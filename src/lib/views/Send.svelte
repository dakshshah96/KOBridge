<script lang="ts">
  import Dropzone from '../Dropzone.svelte'
  import { app, refreshStatus } from '../store.svelte'

  const gb = (kb: number) => (kb / 1024 / 1024).toFixed(1).replace('.', ',')
</script>

<div class="stack">
  <div class="head">
    <div>
      <h1>Enviar libros</h1>
      <p class="sub">Destino <code>/mnt/us/books/</code>{#if app.status} · {gb(app.status.free_kb)} GB libres{/if}</p>
    </div>
  </div>
  <Dropzone kind="books" title="EPUB, PDF, MOBI o CBZ" exts={['epub', 'pdf', 'mobi', 'azw3', 'fb2', 'cbz', 'djvu', 'txt']} onsent={refreshStatus} />
  <p class="sub">KOReader los muestra en el explorador de archivos al volver a abrir la carpeta.</p>
</div>
