<script setup lang="ts">
import { computed } from 'vue'
import type { Card } from '../types'
const props = withDefaults(defineProps<{ card?: Card; back?: boolean; small?: boolean }>(), { back: false, small: false })
const symbols = { spade: '♠', heart: '♥', club: '♣', diamond: '♦', joker: '★' }
const rank = computed(() => {
  if (!props.card) return ''
  return ({ 11: 'J', 12: 'Q', 13: 'K', 14: 'A', 15: '2', 16: '小', 17: '大' } as Record<number, string>)[props.card.rank] || String(props.card.rank)
})
const red = computed(() => props.card && (['heart', 'diamond'].includes(props.card.suit) || props.card.rank === 17))
const joker = computed(() => props.card?.suit === 'joker')
</script>

<template>
  <span class="playing-card" :class="{ 'card-back': back, 'card-red': red, 'card-joker': joker, 'card-small': small }" aria-hidden="true">
    <template v-if="back">
      <span class="back-pattern"><span class="back-emblem">♠</span></span>
    </template>
    <template v-else-if="card">
      <span class="card-corner"><b>{{ rank }}</b><span>{{ joker ? '王' : symbols[card.suit] }}</span></span>
      <span v-if="joker" class="joker-word">JOKER</span>
      <span class="card-center">{{ joker ? '✦' : symbols[card.suit] }}</span>
      <span class="card-corner card-corner-bottom"><b>{{ rank }}</b><span>{{ joker ? '王' : symbols[card.suit] }}</span></span>
    </template>
  </span>
</template>
