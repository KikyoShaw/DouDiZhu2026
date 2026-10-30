<script setup lang="ts">
import type { PlayerView } from '../types'
import AppIcon from './AppIcon.vue'
import PlayingCard from './PlayingCard.vue'
withDefaults(defineProps<{ player?: PlayerView; active?: boolean; seconds?: number | null; position: 'left' | 'right'; bidding?: boolean; lan?: boolean }>(), { active: false, seconds: null, bidding: false, lan: false })
</script>

<template>
  <section class="opponent" :class="[`opponent-${position}`, { 'opponent-active': active }]" :aria-label="player ? `${player.name}，剩余 ${player.count} 张牌` : '等待玩家入座'">
    <div class="opponent-profile">
      <div class="avatar" :class="{ 'avatar-landlord': player?.role === 'landlord', 'avatar-empty': !player?.connected }">
        <AppIcon v-if="!player?.connected" name="users" :size="28" />
        <AppIcon v-else-if="player.role === 'landlord'" name="crown" :size="30" />
        <span v-else>{{ player.bot ? (position === 'left' ? '山' : '竹') : player.name.slice(0, 1) }}</span>
        <span v-if="player?.connected" class="online-dot" :class="{ 'dot-bot': player.bot }"></span>
      </div>
      <span v-if="active && seconds !== null" class="seat-countdown" :class="{ 'countdown-urgent': seconds <= 5 }" aria-label="剩余秒数">{{ seconds }}</span>
      <h3>{{ player?.name || '虚位以待' }}</h3>
      <div class="opponent-labels">
        <span v-if="player?.role === 'landlord'" class="role-tag landlord-tag">地主</span>
        <span v-else-if="player?.role === 'farmer'" class="role-tag">农民</span>
        <span v-else class="role-tag">{{ player?.bot ? '电脑牌友' : (lan ? '房间牌友' : '等待入座') }}</span>
        <span v-if="player?.autoplay" class="muted-tag">托管中</span>
        <span v-else-if="lan && player?.ready && player?.role === 'unknown'" class="ready-tag">已准备</span>
      </div>
      <p v-if="player" class="player-score">{{ player.score > 0 ? '+' : '' }}{{ player.score }} <span>分</span></p>
    </div>
    <div v-if="player && player.count > 0" class="opponent-cards">
      <div class="mini-back-fan"><PlayingCard v-for="index in Math.min(3, player.count)" :key="index" back small /></div>
      <span class="remaining-count"><b>{{ player.count }}</b> 张</span>
    </div>
    <p v-else-if="!player?.connected" class="waiting-seat">等待连接</p>
    <p v-if="active" class="opponent-status">{{ bidding ? '正在叫牌' : '正在出牌' }}<span class="thinking-dots">···</span></p>
  </section>
</template>