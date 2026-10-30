<script setup lang="ts">
import type { SeatAction } from '../types'
import PlayingCard from './PlayingCard.vue'
withDefaults(defineProps<{ action: SeatAction | null; position: 'left' | 'right' | 'local'; name?: string }>(), { name: '' })
const suitNames = { spade: '黑桃', heart: '红桃', club: '梅花', diamond: '方块', joker: '' }
</script>

<template>
  <div class="table-pile" :class="`pile-${position}`" :aria-label="action ? `${name}：${action.text}` : `${name}出牌区域`">
    <template v-if="action">
      <div v-if="action.kind === 'play' && action.cards.length" class="played-cards" :style="{ '--pile-count': action.cards.length }">
        <span v-for="card in action.cards" :key="card.id" class="played-card" :aria-label="`${suitNames[card.suit]}${card.rank > 15 ? (card.rank === 16 ? '小王' : '大王') : ({11: 'J', 12: 'Q', 13: 'K', 14: 'A', 15: '2'} as Record<number, string>)[card.rank] || card.rank}`"><PlayingCard :card="card" small /></span>
      </div>
      <p v-else class="table-speech" :class="{ 'speech-pass': action.kind === 'pass' }">{{ action.text }}</p>
      <span v-if="action.kind === 'play' && action.combo" class="combo-label">{{ action.combo.label }}</span>
    </template>
  </div>
</template>