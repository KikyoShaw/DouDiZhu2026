<script setup lang="ts">
withDefaults(defineProps<{ name: string; size?: number }>(), { size: 20 })
const paths: Record<string, string> = {
  play: 'm9 5 11 7-11 7V5Z',
  pause: 'M8 5v14M16 5v14',
  refresh: 'M20 7v5h-5M4 17v-5h5M5.6 8a7 7 0 0 1 11.6-3L20 8M4 16l2.8 3A7 7 0 0 0 18.4 16',
  users: 'M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M16 3a4 4 0 0 1 0 8M22 21v-2a4 4 0 0 0-3-3.87M13 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0Z',
  settings: 'm9 3-1 3-3 1-2 3 2 2v3l2 3 3 1 2 2 3-2 3-1 2-3v-3l2-2-2-3-3-1-1-3H9ZM15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z',
  volume: 'm11 4-6 5H2v6h3l6 5V4ZM15 8a6 6 0 0 1 0 8M18 5a10 10 0 0 1 0 14',
  mute: 'm11 4-6 5H2v6h3l6 5V4ZM16 9l6 6M22 9l-6 6',
  book: 'M12 5v16M2 3h5a5 5 0 0 1 5 3 5 5 0 0 1 5-3h5v16h-5a5 5 0 0 0-5 2 5 5 0 0 0-5-2H2V3Z',
  close: 'm6 6 12 12M18 6 6 18',
  check: 'm5 12 4 4L19 6',
  arrow: 'M5 12h14m-6-6 6 6-6 6',
  door: 'M9 21H4V3h10v5M10 12h12m-4-4 4 4-4 4M14 16v5H9',
  copy: 'M8 8h12v13H8V8ZM16 8V3H3v13h5',
  sort: 'M4 6h16M4 12h11M4 18h6m8-5v9m-3-3 3 3 3-3',
  sparkles: 'm12 3 2.5 6.5L21 12l-6.5 2.5L12 21l-2.5-6.5L3 12l6.5-2.5L12 3ZM20 2v4m-2-2h4',
  clock: 'M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0ZM12 7v5l3 2',
  crown: 'm3 6 5 4 4-7 4 7 5-4-2 12H5L3 6ZM5 21h14',
  shield: 'm12 3 8 3v6c0 5-8 9-8 9S4 17 4 12V6l8-3Zm-4 9 3 3 5-6',
  plus: 'M12 5v14M5 12h14',
  wifi: 'M2 8a16 16 0 0 1 20 0M5 12a11 11 0 0 1 14 0M8.5 15.5a5 5 0 0 1 7 0M12 19h.01',
  alert: 'm12 3 10 18H2L12 3ZM12 9v5M12 17h.01',
  cards: 'm3 5 9-3 6 17-9 3L3 5ZM14 3h7v16h-3',
  history: 'M3 11a9 9 0 1 1 2 7M3 4v7h7M12 7v5l3 2',
  undo: 'M3 10h11a6 6 0 0 1 0 12M3 10l6-6M3 10l6 6',
  info: 'M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0ZM12 11v6M12 7h.01',
}
</script>

<template>
  <svg :width="size" :height="size" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" class="icon">
    <path :d="paths[name] || paths.info" />
  </svg>
</template>
