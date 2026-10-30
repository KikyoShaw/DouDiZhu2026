<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { Card } from '../types'
import PlayingCard from './PlayingCard.vue'
const props = defineProps<{ cards: Card[]; selected: number[]; enabled: boolean; descending: boolean }>()
const emit = defineEmits<{ select: [ids: number[]] }>()
const fan = ref<HTMLElement | null>(null)
const step = ref(42)
const ordered = computed(() => [...props.cards].sort((a, b) => props.descending ? b.rank - a.rank || b.id - a.id : a.rank - b.rank || a.id - b.id))
const selectedSet = computed(() => new Set(props.selected))
let observer: ResizeObserver | null = null
let pointerId: number | null = null
let startIndex = -1
let endIndex = -1
let original = new Set<number>()
let adding = true
let downX = 0
let downY = 0
let moved = false
let lastTap: { id: number; at: number; x: number; y: number } | null = null
const suitNames = { spade: '黑桃', heart: '红桃', club: '梅花', diamond: '方块', joker: '' }
function label(card: Card) {
  const rank = ({ 11: 'J', 12: 'Q', 13: 'K', 14: 'A', 15: '2', 16: '小王', 17: '大王' } as Record<number, string>)[card.rank] || card.rank
  return `${suitNames[card.suit]}${rank}`
}
function measure() {
  const root = fan.value
  const first = root?.querySelector<HTMLElement>('.hand-card')
  if (!root || !first) return
  const width = first.getBoundingClientRect().width
  step.value = Math.max(12, Math.min(width * 0.62, (root.clientWidth - width - 8) / Math.max(1, ordered.value.length - 1)))
}
function rangeSelection(index: number) {
  if (index === endIndex) return
  endIndex = index
  const next = new Set(original)
  for (let position = Math.min(index, startIndex); position <= Math.max(index, startIndex); position++) {
    const card = ordered.value[position]
    if (card) { if (adding) next.add(card.id); else next.delete(card.id) }
  }
  emit('select', [...next])
}
function pointerDown(event: PointerEvent) {
  if (!props.enabled || pointerId !== null || (event.pointerType === 'mouse' && event.button !== 0)) return
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>('[data-card-index]')
  if (!button) return
  const index = Number(button.dataset.cardIndex)
  const card = ordered.value[index]
  if (!card) return
  event.preventDefault()
  if (lastTap && lastTap.id === card.id && performance.now() - lastTap.at < 300
    && Math.hypot(event.clientX - lastTap.x, event.clientY - lastTap.y) < 9) return
  pointerId = event.pointerId
  startIndex = index
  endIndex = -1
  downX = event.clientX
  downY = event.clientY
  moved = false
  original = new Set(props.selected)
  adding = !original.has(card.id)
  button.focus({ preventScroll: true })
  fan.value?.setPointerCapture(event.pointerId)
  rangeSelection(index)
}
function pointerMove(event: PointerEvent) {
  if (event.pointerId !== pointerId || !props.enabled) return
  if (!moved && Math.hypot(event.clientX - downX, event.clientY - downY) < 5) return
  moved = true
  const buttons = [...(fan.value?.querySelectorAll<HTMLElement>('[data-card-index]') ?? [])]
  let index = 0
  for (let position = 0; position < buttons.length; position++) {
    if (event.clientX >= buttons[position]!.getBoundingClientRect().left) index = position
  }
  rangeSelection(index)
}
function finish(restore = false) {
  if (pointerId === null) return
  const previous = pointerId
  const tappedCard = ordered.value[startIndex]
  if (!restore && !moved && tappedCard) lastTap = { id: tappedCard.id, at: performance.now(), x: downX, y: downY }
  else lastTap = null
  pointerId = null
  if (restore && props.enabled) emit('select', [...original])
  if (fan.value?.hasPointerCapture(previous)) fan.value.releasePointerCapture(previous)
  startIndex = -1
  endIndex = -1
}
function keyboardToggle(card: Card, event: MouseEvent) {
  // Pointer selection is handled by the captured gesture, including a simple click.
  if (event.detail !== 0 || !props.enabled) return
  const next = new Set(props.selected)
  if (next.has(card.id)) next.delete(card.id); else next.add(card.id)
  emit('select', [...next])
}
function navigate(event: KeyboardEvent, index: number) {
  if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return
  event.preventDefault()
  const buttons = fan.value?.querySelectorAll<HTMLButtonElement>('[data-card-index]')
  if (!buttons?.length) return
  const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1
    : Math.max(0, Math.min(buttons.length - 1, index + (event.key === 'ArrowLeft' ? -1 : 1)))
  buttons[next]?.focus()
}
watch(() => [props.cards.length, props.descending], async () => { finish(); await nextTick(); measure() })
watch(() => props.enabled, enabled => { if (!enabled) finish() })
onMounted(() => {
  observer = new ResizeObserver(measure)
  if (fan.value) observer.observe(fan.value)
  measure()
})
onBeforeUnmount(() => { finish(); observer?.disconnect() })
</script>

<template>
  <div ref="fan" class="hand-fan" :class="{ 'hand-locked': !enabled }" :style="{ '--hand-step': `${step}px` }" role="group" aria-label="我的手牌，点击选牌，拖动可连续选择，方向键移动焦点" @pointerdown="pointerDown" @pointermove="pointerMove" @pointerup="finish()" @pointercancel="finish(true)" @lostpointercapture="finish()">
    <button v-for="(card, index) in ordered" :key="card.id" type="button" class="hand-card" :class="{ 'is-selected': selectedSet.has(card.id) }" :data-card-index="index" :aria-label="label(card)" :aria-pressed="selectedSet.has(card.id)" :aria-disabled="!enabled" :tabindex="enabled ? 0 : -1" @click="keyboardToggle(card, $event)" @keydown="navigate($event, index)">
      <PlayingCard :card="card" />
      <span v-if="selectedSet.has(card.id)" class="selection-tick">✓</span>
    </button>
  </div>
</template>
