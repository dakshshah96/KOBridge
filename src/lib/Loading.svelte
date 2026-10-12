<script lang="ts">
  import { onMount } from 'svelte'

  let { title, detail, rows = 5 }: { title: string; detail: string; rows?: number } = $props()

  let secs = $state(0)
  onMount(() => {
    const t = setInterval(() => secs++, 1000)
    return () => clearInterval(t)
  })
</script>

<div class="loading" role="status" aria-live="polite">
  <div class="msg">
    <span class="spin" aria-hidden="true"></span>
    <div>
      <strong>{title}</strong>
      <p class="sub num">{detail}{secs >= 2 ? ` · ${secs} s` : ''}</p>
    </div>
  </div>
  <div class="ghost" aria-hidden="true">
    {#each Array(rows) as _, i}
      <div class="line" style="width:{[92, 78, 85, 64, 88, 70, 81][i % 7]}%"></div>
    {/each}
  </div>
</div>

<style>
  .loading { display: grid; gap: 22px }
  .msg { display: flex; gap: 12px; align-items: center }
  .msg strong { font-weight: 600 }
  .msg .sub { margin: 2px 0 0 }
  .spin { width: 16px; height: 16px; flex: none; border-radius: 50%; border: 2px solid var(--rule); border-top-color: var(--ink); animation: r 0.8s linear infinite }
  .ghost { display: grid; gap: 14px }
  .line { height: 12px; border-radius: 3px; background: var(--paper); animation: fade 1.4s ease-in-out infinite alternate }
  .line:nth-child(2n) { animation-delay: .2s }
  @keyframes r { to { transform: rotate(360deg) } }
  @keyframes fade { from { opacity: 1 } to { opacity: .45 } }
  @media (prefers-reduced-motion: reduce) { .spin, .line { animation: none } }
</style>
