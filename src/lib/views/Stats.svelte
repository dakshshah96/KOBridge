<script lang="ts">
  import { onMount } from 'svelte'
  import Loading from '../Loading.svelte'
  import { api, errText, fmtDay, fmtWhen, type Stats } from '../api'
  import { t, dec } from '../i18n.svelte'
  import { app } from '../store.svelte'

  let stats = $state<Stats | null>(null)
  let busy = $state(false)
  let opened = $state(false)
  let error = $state('')

  async function load(refresh: boolean) {
    busy = refresh
    error = ''
    try {
      stats = (await api.stats(refresh)) ?? stats
    } catch (e) {
      error = errText(e)
    } finally {
      busy = false
    }
  }
  onMount(async () => {
    await load(false)
    opened = true
  })
  // copy fresh stats once per visit, as soon as the Kindle is reachable
  let fetched = false
  $effect(() => { if (app.status && opened && !fetched) { fetched = true; load(true) } })

  // chart geometry: one slot per day, y scale rounded up to a 30-minute step
  const W = 640, TOP = 8, BASE = 124, LEFT = 30
  const max = $derived(Math.max(30, Math.ceil(Math.max(0, ...(stats?.daily ?? []).map((d) => d.minutes)) / 30) * 30))
  const ticks = $derived(Array.from({ length: max / 30 + 1 }, (_, i) => i * 30).filter((v, _, a) => a.length <= 5 || v % 60 === 0))
  const y = (m: number) => BASE - (m / max) * (BASE - TOP)
  const step = $derived((W - LEFT) / (stats?.daily.length || 30))
  const dayLabel = (iso: string) => fmtDay(new Date(iso + 'T12:00'))
  const peak = $derived(Math.round(Math.max(0, ...(stats?.daily ?? []).map((d) => d.minutes))))
  const fmt = dec
</script>

<div class="stack">
  <div class="head">
    <div>
      <h1>{t('Reading', 'Lectura')}</h1>
      <p class="sub">{stats ? `statistics.sqlite3 ${t('copied', 'copiado')} ${fmtWhen(stats.snapshot_at)}` : t('No local copy of the statistics', 'Sin copia local de las estadísticas')}</p>
    </div>
    <button class="btn ghost" disabled={!app.status || busy} onclick={() => load(true)}>{busy ? t('Updating…', 'Actualizando…') : t('Update', 'Actualizar')}</button>
  </div>

  {#if error}<div class="banner bad">{error}</div>{/if}

  {#if stats}
    <div class="kpis num">
      <div><span class="label">{t('Total time', 'Tiempo total')}</span><strong>{fmt(stats.total_hours)} h</strong></div>
      <div><span class="label">{t('Days with reading', 'Días con lectura')}</span><strong>{stats.days_read}</strong></div>
      <div><span class="label">{t('Books opened', 'Libros abiertos')}</span><strong>{stats.books}</strong></div>
      <div><span class="label">{t('Per reading day', 'Promedio por día leído')}</span><strong>{Math.round(stats.avg_minutes)} min</strong></div>
    </div>

    <div class="chart">
      <div class="row" style="justify-content:space-between"><span class="label">{t('Minutes per day · last 30 days', 'Minutos por día · últimos 30 días')}</span><span class="label num">{t('max', 'máx.')} {peak} min</span></div>
      <svg viewBox="0 0 {W} 146" role="img" aria-label={t('Minutes read per day in the last 30 days', 'Minutos leídos por día en los últimos 30 días')}>
        {#each ticks as tick}
          <line x1={LEFT} x2={W} y1={y(tick)} y2={y(tick)} />
          <text x="0" y={y(tick) + 3}>{tick}</text>
        {/each}
        {#each stats.daily as d, i}
          {#if d.minutes > 0}
            <rect x={LEFT + i * step + 2} y={y(d.minutes)} width={step - 4} height={BASE - y(d.minutes)} rx="2" class:today={i === stats.daily.length - 1}>
              <title>{dayLabel(d.date)}: {Math.round(d.minutes)} min</title>
            </rect>
          {/if}
          {#if (stats.daily.length - 1 - i) % 7 === 0}
            {#if i === stats.daily.length - 1}<text x={W} y={BASE + 17} text-anchor="end">{t('today', 'hoy')}</text>
            {:else}<text x={LEFT + i * step + 2} y={BASE + 17}>{dayLabel(d.date)}</text>{/if}
          {/if}
        {/each}
      </svg>
    </div>

    <table class="num">
      <thead><tr><th>{t('Book', 'Libro')}</th><th>{t('Progress', 'Progreso')}</th><th>{t('Time', 'Tiempo')}</th><th>{t('Last opened', 'Última vez')}</th></tr></thead>
      <tbody>
        {#each stats.recent as b}
          <tr>
            <td><div class="t">{b.title}</div><div class="sub">{b.authors}</div></td>
            <td><div class="row"><div class="bar"><i style="width:{Math.round(b.percent * 100)}%"></i></div>{Math.round(b.percent * 100)} %</div></td>
            <td>{fmt(b.hours)} h</td>
            <td class="sub">{b.last_open ? fmtWhen(b.last_open) : '—'}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else if busy || !opened || app.checking}
    <Loading
      title={busy ? t('Copying statistics from the Kindle', 'Copiando estadísticas de la Kindle') : !opened ? t('Opening saved statistics', 'Abriendo estadísticas guardadas') : t('Waiting for the Kindle', 'Esperando a la Kindle')}
      detail={busy ? t('sqlite3 .backup of statistics.sqlite3 and a copy over SSH', 'sqlite3 .backup de statistics.sqlite3 y copia por SSH') : t('First the app has to find it on the network', 'Primero hay que encontrarla en la red')}
    />
  {:else}
    <div class="empty">{t('Connect the Kindle to copy the KOReader statistics.', 'Conectá la Kindle para copiar las estadísticas de KOReader.')}</div>
  {/if}
</div>

<style>
  .kpis { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 1px; background: var(--rule); border: 1px solid var(--rule); border-radius: 8px; overflow: hidden }
  .kpis div { background: var(--sheet); padding: 14px 16px; display: grid; gap: 4px }
  .kpis strong { font: 600 24px/1 var(--sans) }
  .chart { border: 1px solid var(--rule); border-radius: 8px; padding: 14px 16px 8px }
  .chart svg { width: 100%; height: auto; display: block; margin-top: 8px }
  .chart line { stroke: var(--rule) }
  .chart text { fill: var(--ink-2); font: 10px var(--sans) }
  .chart rect { fill: var(--ink) }
  .chart rect.today { fill: var(--ink-2) }
  table { width: 100%; border-collapse: collapse }
  th { text-align: left; font-weight: 500; font-size: 11px; color: var(--ink-2); text-transform: uppercase; letter-spacing: .06em; padding: 0 10px 8px 0; border-bottom: 1px solid var(--rule) }
  td { padding: 10px 10px 10px 0; border-bottom: 1px solid var(--rule); vertical-align: middle }
  td .sub { margin: 0; font-size: 12px }
  .t { font-family: var(--serif); font-size: 14px }
  .bar { height: 4px; background: var(--rule); border-radius: 2px; width: 110px; overflow: hidden }
  .bar i { display: block; height: 100%; background: var(--ink) }
</style>
