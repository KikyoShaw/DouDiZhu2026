<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import type { Settings } from './types'
import AppDialog from './components/AppDialog.vue'
import AppIcon from './components/AppIcon.vue'
import HandCards from './components/HandCards.vue'
import PlayerSeat from './components/PlayerSeat.vue'
import PlayingCard from './components/PlayingCard.vue'
import TablePile from './components/TablePile.vue'
import { defaultSettings, useGame } from './composables/useGame'
import { useSound } from './composables/useSound'

const {
  snapshot: game, loading, startupError, commandError, toast, localSeat, me, networkReady, isOwnTurn,
  canAct, canSelect, turnBusy, controlBusy, autoplayBusy, readyBusy, hintBusy, connectionBusy,
  preferencesBusy, remainingMs, connect, action, getHint, newMatch, hostLan, joinLan, leaveLan,
  preferences, notify,
} = useGame()
useSound(game)

type Dialog = 'settings' | 'rules' | 'room' | 'result' | 'history' | 'new' | null
const dialog = ref<Dialog>(null)
const selected = ref<number[]>([])
const descending = ref(true)
const settingsDraft = reactive<Settings>({ ...defaultSettings })
const confirmSettings = ref(false)
const roomTab = ref<'host' | 'join'>('host')
const roomDraft = reactive({ playerName: '我', bidMode: 'score' as Settings['bidMode'], baseScore: 1, turnSeconds: 20, port: 9528, address: '' })
const roomAddressInput = ref<HTMLInputElement | null>(null)
const pendingSettings = ref(false)
const seenResults = new Set<string>()

const rightSeat = computed(() => (localSeat.value + 1) % 3)
const leftSeat = computed(() => (localSeat.value + 2) % 3)
const rightPlayer = computed(() => game.value?.players.find(player => player.seat === rightSeat.value))
const leftPlayer = computed(() => game.value?.players.find(player => player.seat === leftSeat.value))
const seconds = computed(() => remainingMs.value === null ? null : Math.max(0, Math.ceil(remainingMs.value / 1000)))
const isLan = computed(() => game.value?.mode === 'lan')
const isActiveRound = computed(() => !!game.value && ['bidding', 'playing'].includes(game.value.stage))
const isReadyStage = computed(() => !!game.value && ['lobby', 'finished', 'allPassed'].includes(game.value.stage))
const readyCount = computed(() => game.value?.players.filter(player => player.connected && player.ready).length ?? 0)
const readyAllowed = computed(() => isLan.value && isReadyStage.value && networkReady.value && !!me.value?.connected && !readyBusy.value && !connectionBusy.value)
const localAction = computed(() => game.value?.table.find(item => item?.seat === localSeat.value) ?? null)
const leftAction = computed(() => game.value?.table.find(item => item?.seat === leftSeat.value) ?? null)
const rightAction = computed(() => game.value?.table.find(item => item?.seat === rightSeat.value) ?? null)
const recentEvents = computed(() => game.value?.events.slice(-3).reverse() ?? [])
const allEvents = computed(() => [...(game.value?.events ?? [])].reverse())
const settingsChanged = computed(() => {
  const current = game.value?.settings
  return !!current && (settingsDraft.playerName.trim() !== current.playerName || settingsDraft.bidMode !== current.bidMode
    || Number(settingsDraft.baseScore) !== current.baseScore || Number(settingsDraft.turnSeconds) !== current.turnSeconds)
})
const phaseButton = computed(() => {
  const value = game.value
  if (!value) return { text: '开始对局', icon: 'play', action: 'start' as const, disabled: true, title: '正在连接游戏服务' }
  const next = ['finished', 'allPassed'].includes(value.stage)
  const paused = value.lifecycle === 'paused'
  const starting = value.stage === 'lobby' || next
  const text = starting ? next ? '再来一局' : '开始对局' : paused ? '继续对局' : '暂停'
  const reason = starting
    ? !value.canStart && isLan.value ? value.network.role === 'guest' ? '开始对局由房主控制，请先准备' : '三位玩家全部入座并准备后，由房主开始' : ''
    : !value.canControl ? '暂停和继续由房主控制' : ''
  return { text, icon: paused || starting ? 'play' : 'pause', action: next ? 'nextRound' as const : value.stage === 'lobby' ? 'start' as const : paused ? 'resume' as const : 'pause' as const,
    disabled: !networkReady.value || (starting ? !value.canStart : !value.canControl) || controlBusy.value || connectionBusy.value,
    title: reason || text }
})
const statusTitle = computed(() => {
  const value = game.value
  if (!value) return '正在连接牌桌'
  if (!networkReady.value) return value.network.status === 'connecting' ? '正在连接房主' : '房间连接已断开'
  if (value.stage === 'lobby') return isLan.value ? '三位牌友，一桌好牌' : '有空，来一局。'
  if (value.stage === 'allPassed') return '无人叫地主，重新发牌'
  if (value.stage === 'finished') return '好牌有来回，下局再见分晓'
  if (value.lifecycle === 'paused') return '歇一歇，牌局等你'
  if (value.stage === 'bidding') return isOwnTurn.value ? '轮到你叫牌' : `${turnName.value}正在叫牌`
  if (me.value?.autoplay && isOwnTurn.value) return '电脑正在替你出牌'
  return isOwnTurn.value ? game.value?.canPass ? '轮到你，接得住就出' : '由你领出第一手' : `${turnName.value}正在出牌`
})
const turnName = computed(() => game.value?.players.find(player => player.seat === game.value?.turn)?.name || '牌友')
const statusDetail = computed(() => {
  const value = game.value
  if (!value) return ''
  if (!networkReady.value) return value.network.message || '请在网络房间中检查连接，或离开房间回到单人牌局。'
  if (value.stage === 'lobby') return isLan.value ? `${readyCount.value} / 3 人已准备 · 同一局域网内相约开局` : '一位玩家，两位电脑牌友。54 张牌，各凭本事。'
  if (value.stage === 'allPassed') return isLan.value ? '全部玩家准备后，由房主发起下一轮。' : '三位玩家都选择不叫，本轮不计分。'
  if (value.stage === 'finished') return '本局积分已记入累计成绩。你可以查看结算，或准备下一局。'
  if (value.lifecycle === 'paused') return value.canControl ? '倒计时和电脑操作已暂停，点击「继续对局」回到牌桌。' : '房主已暂停对局，等待房主继续。'
  if (value.stage === 'bidding') return value.bidMode === 'score' ? '叫分越高，底分越高。最高 3 分，0 分表示不叫。' : '叫地主或抢地主，确认后等待下一位玩家。'
  if (turnBusy.value) return '操作已提交，等待房主确认。'
  if (me.value?.autoplay) return '你随时可以取消托管，亲自接管手牌。'
  return value.previous ? `上一手 · ${value.previous.label}` : '新一轮出牌，可以自由选择牌型。'
})
const won = computed(() => !!game.value?.result && game.value.result.landlordWon === (game.value.landlord === localSeat.value))
const resultDelta = computed(() => game.value?.result?.deltas[localSeat.value] ?? 0)
const networkLabel = computed(() => {
  const status = game.value?.network.status
  return ({ offline: '本机对局', hosting: '房间已创建', connecting: '连接中', connected: '已连接房间', disconnected: '连接已断开' } as Record<string, string>)[status ?? 'offline']
})

watch(() => game.value, (value, previous) => {
  if (!value) return
  const owned = new Set(value.myHand.map(card => card.id))
  const changedTurn = previous && (value.sessionId !== previous.sessionId || value.roundId !== previous.roundId || value.turnToken !== previous.turnToken)
  selected.value = changedTurn ? [] : selected.value.filter(id => owned.has(id))
  if (value.result && value.stage === 'finished') {
    const key = `${value.sessionId}:${value.roundId}`
    if (!seenResults.has(key)) {
      seenResults.add(key)
      if (seenResults.size > 64) seenResults.delete(seenResults.values().next().value!)
      if (!dialog.value || dialog.value === 'result') openDialog('result')
    }
  }
  if (dialog.value === 'result' && (value.stage !== 'finished' || !value.result)) dialog.value = null
})
watch(canSelect, enabled => { if (!enabled) selected.value = [] })

function openDialog(value: Exclude<Dialog, null>) {
  commandError.value = ''
  confirmSettings.value = false
  if (value === 'settings') Object.assign(settingsDraft, game.value?.settings || defaultSettings)
  if (value === 'room' && game.value) {
    Object.assign(roomDraft, {
      playerName: game.value.settings.playerName, bidMode: game.value.settings.bidMode,
      baseScore: game.value.settings.baseScore, turnSeconds: game.value.settings.turnSeconds,
    })
  }
  dialog.value = value
}
function closeDialog() { dialog.value = null; commandError.value = ''; confirmSettings.value = false }
function validateSettings(value: Settings) {
  if (!value.playerName.trim()) return '请填写你的昵称。'
  if (!Number.isInteger(Number(value.baseScore)) || value.baseScore < 1 || value.baseScore > 100) return '基础分请填写 1 至 100 的整数。'
  if (!Number.isInteger(Number(value.turnSeconds)) || value.turnSeconds < 0 || value.turnSeconds > 120) return '出牌时间请填写 0 至 120 的整数，0 表示不限时。'
  return ''
}
function inlineError(text: string) { commandError.value = text }
function cleanSettings(value: Settings): Settings {
  return { ...value, playerName: value.playerName.trim(), baseScore: Number(value.baseScore), turnSeconds: Number(value.turnSeconds) }
}
async function saveSettings() {
  if (!game.value || pendingSettings.value) return
  if (isLan.value) {
    if (await preferences(settingsDraft.sound, settingsDraft.showHints)) { closeDialog(); notify('偏好设置已保存') }
    return
  }
  const error = validateSettings(settingsDraft)
  if (error) { inlineError(error); return }
  if (settingsChanged.value && isActiveRound.value && !confirmSettings.value) { confirmSettings.value = true; return }
  const replacesMatch = settingsChanged.value
  pendingSettings.value = true
  try {
    const updated = replacesMatch ? await newMatch(cleanSettings(settingsDraft)) : await preferences(settingsDraft.sound, settingsDraft.showHints)
    if (updated) { closeDialog(); notify(replacesMatch ? '已应用设置，新的牌局已就绪' : '设置已保存') }
  } finally { pendingSettings.value = false }
}
async function startFresh() {
  if (!game.value || connectionBusy.value) return
  if (await newMatch({ ...game.value.settings })) { closeDialog(); selected.value = []; notify('新的单人牌局已就绪') }
}
async function runPhase() { await action({ type: phaseButton.value.action }) }
async function toggleReady() { if (readyAllowed.value) await action({ type: 'ready', value: !me.value?.ready }) }
async function toggleAutoplay() {
  if (!game.value || !networkReady.value || autoplayBusy.value || connectionBusy.value) return
  const enabling = !me.value?.autoplay
  if (await action({ type: 'autoplay', value: enabling })) {
    selected.value = []
    notify(enabling ? '已开启托管，可随时取消' : '已取消托管')
  }
}
async function toggleSound() {
  if (!game.value) return
  const enabled = !game.value.settings.sound
  if (await preferences(enabled, game.value.settings.showHints)) notify(enabled ? '音效已开启' : '音效已关闭')
}
function selectCards(ids: number[]) {
  if (!canSelect.value || !game.value) return
  const owned = new Set(game.value.myHand.map(card => card.id))
  selected.value = [...new Set(ids)].filter(id => owned.has(id))
}
async function playSelected() {
  if (!game.value || !canSelect.value || !selected.value.length) return
  const owned = new Set(game.value.myHand.map(card => card.id))
  if (!selected.value.every(id => owned.has(id))) { selected.value = []; notify('手牌已更新，请重新选择。'); return }
  await action({ type: 'play', cards: [...selected.value] })
}
async function pass() {
  if (!canSelect.value || !game.value?.canPass) return
  await action({ type: 'pass' })
}
async function hint() {
  const result = await getHint()
  if (!result) return
  selected.value = result.pass ? [] : [...new Set(result.cards)]
  notify(result.pass ? `可以不出${result.message ? ` · ${result.message}` : ''}` : result.message || '已选中推荐手牌，可自行调整后出牌')
}
function bidLabel(value: number) {
  if (game.value?.bidMode === 'score') return value === 0 ? '不叫' : `${value} 分`
  if (game.value?.bidPrompt === '叫地主') return value === 0 ? '不叫' : '叫地主'
  return value === 0 ? '不抢' : '抢地主'
}
function portError() {
  const port = Number(roomDraft.port)
  return !Number.isInteger(port) || port < 1 || port > 65535 ? '端口请填写 1 至 65535 的整数。' : ''
}
async function createRoom() {
  const settings: Settings = { ...(game.value?.settings || defaultSettings), playerName: roomDraft.playerName.trim(), bidMode: roomDraft.bidMode, baseScore: Number(roomDraft.baseScore), turnSeconds: Number(roomDraft.turnSeconds) }
  const error = validateSettings(settings) || portError()
  if (error) { inlineError(error); return }
  if (await hostLan(Number(roomDraft.port), settings)) notify('房间已创建，邀请同一局域网的牌友加入')
}
async function enterRoom() {
  const error = portError() || (!roomDraft.address.trim() ? '请填写房主的局域网地址。' : '') || (!roomDraft.playerName.trim() ? '请填写你的昵称。' : '')
  if (error) { inlineError(error); return }
  if (await joinLan(roomDraft.address.trim(), Number(roomDraft.port), roomDraft.playerName.trim())) notify('已发送加入请求，请等待房主连接')
}
async function exitRoom() {
  if (await leaveLan()) { notify('已离开网络房间，返回单人牌局'); commandError.value = '' }
}
async function copyAddress() {
  const address = game.value?.network.address
  if (!address) return
  try { await navigator.clipboard.writeText(address); notify('房间地址已复制') }
  catch { roomAddressInput.value?.select(); notify('请按 Ctrl+C 复制已选中的房间地址') }
}
function keydown(event: KeyboardEvent) {
  if (dialog.value || event.defaultPrevented || event.repeat || event.ctrlKey || event.metaKey || event.altKey || event.isComposing) return
  const target = event.target as HTMLElement | null
  if (target?.closest('input, textarea, select, [contenteditable="true"]')) return
  if (event.key === 'Escape') { selected.value = []; return }
  if (target?.closest('button, a, [role="button"]') && !target.closest('.hand-card')) return
  if (event.key === 'Enter' && canSelect.value && selected.value.length) { event.preventDefault(); void playSelected() }
  if (event.code === 'Space' && canSelect.value && game.value?.canPass) { event.preventDefault(); void pass() }
}
onMounted(() => window.addEventListener('keydown', keydown))
onBeforeUnmount(() => window.removeEventListener('keydown', keydown))
</script>

<template>
  <div class="game-app">
    <header class="app-header">
      <a class="brand" href="#" aria-label="斗地主 · 三人局，返回牌桌" @click.prevent="closeDialog()">
        <span class="brand-mark">♠<i>♦</i></span>
        <span><strong>斗地主 <span class="brand-separator">·</span> 三人局</strong><small>一桌三人，一手好牌</small></span>
      </a>
      <span class="mode-badge"><span class="status-dot" :class="{ 'status-warning': isLan && !networkReady }"></span>{{ isLan ? '局域网 · 三人' : '单人 · 双 AI' }}</span>
      <nav class="toolbar" aria-label="牌桌工具栏">
        <button class="toolbar-button" :disabled="!game || connectionBusy" @click="openDialog('new')"><AppIcon name="plus" :size="18" /><span>新对局</span></button>
        <button class="toolbar-button toolbar-phase" :disabled="phaseButton.disabled" :title="phaseButton.title" @click="runPhase"><AppIcon :name="phaseButton.icon" :size="17" /><span>{{ controlBusy ? '请稍候' : phaseButton.text }}</span></button>
        <span class="toolbar-divider"></span>
        <button class="toolbar-button" :disabled="!game" @click="openDialog('room')"><AppIcon name="users" :size="18" /><span>网络房间</span></button>
        <button class="icon-button" :disabled="!game || preferencesBusy" :title="game?.settings.sound ? '关闭音效' : '开启音效'" :aria-label="game?.settings.sound ? '关闭音效' : '开启音效'" :aria-pressed="game?.settings.sound" @click="toggleSound"><AppIcon :name="game?.settings.sound ? 'volume' : 'mute'" :size="19" /></button>
        <button class="icon-button" :disabled="!game" title="设置" aria-label="设置" @click="openDialog('settings')"><AppIcon name="settings" :size="19" /></button>
        <button class="icon-button" title="游戏规则" aria-label="游戏规则" @click="openDialog('rules')"><AppIcon name="book" :size="19" /></button>
      </nav>
    </header>

    <main v-if="!game" class="startup-screen">
      <div class="startup-art" aria-hidden="true"><span class="startup-card">♣</span><span class="startup-card">♥</span><span class="startup-card">♠</span></div>
      <p class="eyebrow">斗地主 · 三人局</p>
      <h1>{{ loading ? '正在为你准备牌桌' : '连接原生游戏服务' }}</h1>
      <p class="startup-description">{{ loading ? '洗好一副牌，等你入座。' : startupError || '请通过桌面应用进入完整牌局。' }}</p>
      <template v-if="!loading">
        <button class="button button-gold" @click="connect"><AppIcon name="refresh" :size="17" />重新连接</button>
        <div class="native-instructions"><span>开发环境请在项目目录启动原生应用</span><code>npm run tauri dev</code></div>
      </template>
      <span v-else class="loading-spinner" aria-label="正在连接"></span>
    </main>

    <main v-else class="game-main">
      <div v-if="!networkReady" class="connection-banner" role="status"><AppIcon name="alert" :size="17" /><span>{{ game.network.message || '房间连接暂不可用，等待重新连接。' }}</span><button @click="openDialog('room')">查看房间 <AppIcon name="arrow" :size="15" /></button></div>
      <section class="tabletop" aria-label="三人斗地主牌桌">
        <div class="table-inlay" aria-hidden="true"></div>
        <div class="table-topline">
          <div class="table-round"><span class="eyebrow">{{ isLan ? '好友同桌' : '闲时一局' }}</span><span>{{ game.stage === 'lobby' ? '等待开局' : `第 ${game.roundId} 局` }}</span></div>
          <div class="bottom-area">
            <span class="bottom-label">地主底牌</span>
            <div class="bottom-cards" :aria-label="game.bottomHidden ? '三张底牌尚未公开' : '已公开的地主底牌'">
              <template v-if="game.bottomHidden"><PlayingCard v-for="index in 3" :key="index" back small /></template>
              <PlayingCard v-for="card in game.bottomHidden ? [] : game.bottom" :key="card.id" :card="card" small />
              <template v-if="!game.bottomHidden && game.bottom.length === 0"><PlayingCard v-for="index in 3" :key="index" back small /></template>
            </div>
          </div>
          <div class="table-stakes"><span>基础分 <b>{{ game.baseScore }}</b></span><i></i><span>当前倍数 <b class="multiplier">×{{ game.multiplier }}</b></span></div>
        </div>

        <PlayerSeat :player="leftPlayer" position="left" :active="game.turn === leftSeat && game.lifecycle === 'running'" :seconds="seconds" :bidding="game.stage === 'bidding'" :lan="isLan" />
        <PlayerSeat :player="rightPlayer" position="right" :active="game.turn === rightSeat && game.lifecycle === 'running'" :seconds="seconds" :bidding="game.stage === 'bidding'" :lan="isLan" />
        <TablePile :action="leftAction" position="left" :name="leftPlayer?.name" />
        <TablePile :action="rightAction" position="right" :name="rightPlayer?.name" />
        <TablePile v-if="isActiveRound" :action="localAction" position="local" :name="me?.name" />

        <div class="table-center" :class="{ 'center-welcome': isReadyStage, 'center-active': isActiveRound && game.lifecycle !== 'paused', 'center-paused': game.lifecycle === 'paused' && isActiveRound }">
          <div v-if="game.stage === 'lobby'" class="welcome-emblem" aria-hidden="true">♠<span>♥</span>♣<span>♦</span></div>
          <div v-else-if="game.stage === 'finished'" class="center-emblem"><AppIcon name="crown" :size="34" /></div>
          <div v-else-if="game.stage === 'allPassed'" class="center-emblem"><AppIcon name="refresh" :size="30" /></div>
          <div v-else-if="game.lifecycle === 'paused'" class="center-emblem"><AppIcon name="pause" :size="30" /></div>
          <p v-if="game.stage === 'lobby'" class="eyebrow">五十四张牌 · 三位牌友</p>
          <p v-else-if="game.stage === 'bidding'" class="center-kicker">{{ game.bidMode === 'score' ? '叫分定地主' : '叫抢定地主' }}<span v-if="game.bidMode === 'score' && game.highestBid > 0"> · 当前 {{ game.highestBid }} 分</span></p>
          <h1 v-if="isReadyStage">{{ statusTitle }}</h1><h2 v-else>{{ statusTitle }}</h2>
          <p class="table-center-description">{{ statusDetail }}</p>
          <div v-if="isReadyStage" class="welcome-actions">
            <button v-if="isLan" class="button button-outline" :class="{ 'button-ready': me?.ready }" :disabled="!readyAllowed" @click="toggleReady"><AppIcon :name="me?.ready ? 'check' : 'plus'" :size="17" />{{ me?.ready ? '已准备 · 取消' : '准备就绪' }}</button>
            <button class="button button-gold" :disabled="phaseButton.disabled" :title="phaseButton.title" @click="runPhase"><AppIcon name="play" :size="17" />{{ phaseButton.text }}</button>
            <button v-if="game.stage === 'finished'" class="button button-quiet" @click="openDialog('result')">查看本局结算<AppIcon name="arrow" :size="16" /></button>
          </div>
          <button v-else-if="game.lifecycle === 'paused' && game.canControl" class="button button-gold" :disabled="controlBusy || connectionBusy || !networkReady" @click="action({ type: 'resume' })"><AppIcon name="play" :size="17" />继续对局</button>
          <div v-if="game.stage === 'lobby'" class="lobby-notes"><span><AppIcon :name="isLan ? 'wifi' : 'shield'" :size="14" />{{ isLan ? '同一局域网 · 无需账号' : '本地运行 · 随开随玩' }}</span><button @click="openDialog('rules')">第一次玩？看看规则 <AppIcon name="arrow" :size="13" /></button></div>
        </div>

        <div class="table-footnote"><span class="table-watermark">三人局 <i>·</i> 好牌慢慢打</span><button class="table-history" @click="openDialog('history')"><AppIcon name="history" :size="15" /><span>{{ recentEvents[0]?.text || '牌桌动态' }}</span></button></div>
      </section>

      <section class="hand-dock" aria-label="我的手牌及出牌操作">
        <div class="hand-heading">
          <div class="my-profile">
            <span class="my-avatar" :class="{ 'avatar-landlord': me?.role === 'landlord' }"><AppIcon v-if="me?.role === 'landlord'" name="crown" :size="24" /><span v-else>{{ me?.name.slice(0, 1) || '我' }}</span></span>
            <div><div class="my-name"><strong>{{ me?.name || game.settings.playerName }}</strong><span class="you-tag">你</span><span v-if="me?.role !== 'unknown'" class="role-tag" :class="{ 'landlord-tag': me?.role === 'landlord' }">{{ me?.role === 'landlord' ? '地主' : '农民' }}</span></div><p>累计 <b>{{ (me?.score ?? 0) > 0 ? '+' : '' }}{{ me?.score ?? 0 }}</b> 分<span v-if="game.myHand.length" class="my-card-count">手牌 {{ game.myHand.length }} 张</span></p></div>
          </div>
          <div v-if="game.turn === localSeat && isActiveRound" class="my-turn-badge" :class="{ 'turn-paused': game.lifecycle !== 'running' }"><AppIcon :name="game.lifecycle === 'paused' ? 'pause' : 'clock'" :size="17" /><span>{{ game.lifecycle === 'paused' ? '已暂停' : me?.autoplay ? '托管出牌中' : '轮到你了' }}</span><b v-if="seconds !== null" :class="{ 'countdown-urgent': seconds <= 5 }">{{ seconds }}<small>秒</small></b><span v-else class="unlimited-tag">不限时</span></div>
          <div class="hand-heading-actions">
            <label v-if="isLan && isReadyStage" class="ready-checkbox"><input type="checkbox" :checked="me?.ready" :disabled="!readyAllowed" @change="toggleReady" /><span>我已准备</span></label>
            <button class="autoplay-button" :class="{ 'autoplay-active': me?.autoplay }" :disabled="!networkReady || autoplayBusy || connectionBusy" :aria-pressed="me?.autoplay" @click="toggleAutoplay"><AppIcon name="shield" :size="17" />{{ me?.autoplay ? '取消托管' : '托管' }}<span class="toggle-track"><i></i></span></button>
          </div>
        </div>

        <div v-if="game.myHand.length" class="hand-area"><HandCards :cards="game.myHand" :selected="selected" :enabled="canSelect" :descending="descending" @select="selectCards" /></div>
        <div v-else class="empty-hand"><span class="empty-hand-suits" aria-hidden="true">♠ <i>♥</i> ♣ <i>♦</i></span><p>{{ game.stage === 'lobby' ? '入座已就绪，开始后发牌' : game.stage === 'allPassed' ? '牌已收好，等待重新发牌' : game.stage === 'finished' ? '本局已结束，下一手好牌在等你' : '你的手牌已出完' }}</p></div>

        <div v-if="game.stage === 'bidding'" class="action-bar bidding-bar">
          <div class="selection-summary"><span class="action-kicker">{{ game.bidMode === 'score' ? '叫分阶段' : game.bidPrompt || '叫抢阶段' }}</span><span>{{ isOwnTurn ? me?.autoplay ? '正在自动叫牌' : '请选择你的决定' : `等待 ${turnName}` }}</span></div>
          <div class="bid-actions"><button v-for="value in game.bidOptions" :key="value" class="button" :class="value === 0 ? 'button-outline' : 'button-gold'" :disabled="!canAct || turnBusy" @click="action({ type: 'bid', value })">{{ bidLabel(value) }}</button><span v-if="!game.bidOptions.length" class="action-waiting">等待下一位叫牌</span></div>
        </div>
        <div v-else class="action-bar">
          <div class="selection-summary"><template v-if="selected.length"><span>已选 <b>{{ selected.length }}</b> 张</span><button class="text-button" @click="selected = []">撤销选中</button></template><span v-else class="selection-placeholder">{{ isReadyStage ? '好牌不急，准备开局' : me?.autoplay ? '托管中 · 可随时接管' : isOwnTurn ? '点击或拖动选择手牌' : '看看牌，等你的回合' }}</span></div>
          <div class="play-actions"><button class="button button-quiet sort-button" :disabled="!game.myHand.length || turnBusy" :title="descending ? '当前从大到小，切换从小到大' : '当前从小到大，切换从大到小'" @click="descending = !descending"><AppIcon name="sort" :size="17" />整理<span class="sort-direction">{{ descending ? '大→小' : '小→大' }}</span></button><button class="button button-outline" :disabled="!canSelect || hintBusy" @click="hint"><AppIcon name="sparkles" :size="17" />{{ hintBusy ? '寻找中' : '提示' }}</button><button class="button button-outline pass-button" :disabled="!canSelect || !game.canPass" :title="!game.canPass ? '本轮由你领出，不能不出' : '不出（空格键）'" @click="pass">不出</button><button class="button button-gold play-button" :disabled="!canSelect || !selected.length" @click="playSelected">{{ turnBusy ? '提交中' : '出牌' }}<AppIcon name="arrow" :size="18" /></button></div>
        </div>
        <div v-if="game.settings.showHints" class="keyboard-hints"><span>点击选牌 · 拖动连选</span><span><kbd>Enter</kbd> 出牌 <kbd>Space</kbd> 不出 <kbd>Esc</kbd> 撤销</span></div>
      </section>
      <footer class="app-status"><span><span class="status-dot" :class="{ 'status-warning': !networkReady }"></span>{{ networkLabel }}<i>·</i>{{ game.bidMode === 'score' ? '叫分制' : '叫抢制' }}<i>·</i>{{ game.settings.turnSeconds === 0 ? '不限时' : `${game.settings.turnSeconds} 秒出牌` }}</span><span>纯粹牌局 · 只计积分</span></footer>
    </main>

    <Transition name="toast"><div v-if="toast" class="toast-message" :class="{ 'toast-error': toast.error }" role="status" aria-live="polite"><AppIcon :name="toast.error ? 'alert' : 'check'" :size="18" /><span>{{ toast.text }}</span><button class="toast-dismiss" aria-label="关闭提示" @click="toast = null"><AppIcon name="close" :size="15" /></button></div></Transition>

    <AppDialog v-if="dialog === 'settings'" title="把牌局调成你的节奏" subtitle="偏好与对局设置" @close="closeDialog">
      <form id="settings-form" class="settings-form" @submit.prevent="saveSettings">
        <p v-if="isLan" class="form-notice"><AppIcon name="info" :size="18" />联机对局规则已锁定。音效与操作提示仅影响你自己的界面。</p>
        <fieldset class="form-fields" :disabled="isLan || pendingSettings || connectionBusy">
          <label class="field">我的昵称<input v-model="settingsDraft.playerName" maxlength="16" autocomplete="nickname" placeholder="怎么称呼你" /></label>
          <div class="field"><span>地主决定方式</span><div class="segmented-control"><label :class="{ selected: settingsDraft.bidMode === 'score' }"><input v-model="settingsDraft.bidMode" type="radio" value="score" />叫分制</label><label :class="{ selected: settingsDraft.bidMode === 'rob' }"><input v-model="settingsDraft.bidMode" type="radio" value="rob" />叫抢制</label></div></div>
          <div class="form-row"><label class="field">基础分<div class="input-with-unit"><input v-model.number="settingsDraft.baseScore" type="number" min="1" max="100" step="1" required /><span>分</span></div><small>1–100，按倍数结算</small></label><label class="field">每次操作时间<div class="input-with-unit"><input v-model.number="settingsDraft.turnSeconds" type="number" min="0" max="120" step="1" required /><span>秒</span></div><small>默认 20 秒，0 为不限时</small></label></div>
        </fieldset>
        <div class="preference-list"><label class="preference-row"><span><strong>牌桌音效</strong><small>轻量合成音，回应每一次出牌</small></span><input v-model="settingsDraft.sound" class="switch-input" type="checkbox" /></label><label class="preference-row"><span><strong>操作提示</strong><small>显示选牌说明与键盘快捷键</small></span><input v-model="settingsDraft.showHints" class="switch-input" type="checkbox" /></label></div>
        <p v-if="!isLan && settingsChanged" class="form-note">对局设置将应用到一场新的单人牌局，累计积分会重置。</p>
        <div v-if="confirmSettings" class="confirmation-note"><AppIcon name="alert" :size="19" /><p>当前牌局还未结束。再次确认将结束本局，按新设置回到待开始状态。</p></div>
        <p v-if="commandError" class="inline-error" role="alert">{{ commandError }}</p>
      </form>
      <template #footer><button class="button button-outline" @click="closeDialog">取消</button><button class="button button-gold" form="settings-form" type="submit" :disabled="pendingSettings || connectionBusy || preferencesBusy">{{ pendingSettings || preferencesBusy ? '正在保存' : confirmSettings ? '确认结束并应用' : !isLan && settingsChanged ? '保存并新建对局' : '保存设置' }}</button></template>
    </AppDialog>

    <AppDialog v-if="dialog === 'new'" title="一场新的牌局" subtitle="重新入座" @close="closeDialog">
      <div class="new-match-art" aria-hidden="true">♠ <span>♥</span> ♣ <span>♦</span></div><p class="dialog-copy">{{ isLan ? '将离开当前网络房间，回到一人对两位电脑的单人牌局。' : isActiveRound ? '当前对局将结束。重新发牌之前，可以先在设置中选择喜欢的规则。' : '重新创建一场单人牌局，与两位电脑牌友从零开始。' }}累计积分将重置，当前设置会保留。</p>
      <div class="match-summary"><span>{{ game?.settings.playerName }}</span><span>{{ game?.settings.bidMode === 'score' ? '叫分制' : '叫抢制' }}</span><span>{{ game?.settings.baseScore }} 基础分</span><span>{{ game?.settings.turnSeconds === 0 ? '不限时' : `${game?.settings.turnSeconds} 秒` }}</span></div>
      <p v-if="commandError" class="inline-error" role="alert">{{ commandError }}</p>
      <template #footer><button class="button button-outline" @click="closeDialog">留在当前牌桌</button><button class="button button-gold" :disabled="connectionBusy || controlBusy" @click="startFresh">{{ connectionBusy ? '正在准备' : '创建新对局' }}<AppIcon name="arrow" :size="17" /></button></template>
    </AppDialog>

    <AppDialog v-if="dialog === 'room'" :title="isLan ? '牌友已入座' : '好牌，和朋友一起打'" subtitle="局域网 · 三人房间" wide @close="closeDialog">
      <template v-if="isLan && game">
        <div class="room-live-status"><span class="connection-pill" :class="{ 'connection-bad': !networkReady }"><span class="status-dot"></span>{{ networkLabel }}</span><span>{{ game.network.role === 'host' ? '你是房主' : '你是访客' }}</span></div>
        <label v-if="game.network.address" class="field room-address">房间地址<div class="copy-field"><input ref="roomAddressInput" :value="game.network.address" readonly aria-label="房间地址" @focus="($event.target as HTMLInputElement).select()" /><button class="icon-button" type="button" title="复制房间地址" aria-label="复制房间地址" @click="copyAddress"><AppIcon name="copy" :size="18" /></button></div></label>
        <p class="form-note">三位玩家需要在同一局域网内。请将房主的局域网 IP 与端口告诉牌友。</p>
        <div class="room-roster"><div v-for="player in [...game.players].sort((a, b) => a.seat - b.seat)" :key="player.seat" class="room-player"><span class="room-player-avatar"><AppIcon v-if="!player.connected" name="users" :size="21" /><span v-else>{{ player.name.slice(0, 1) }}</span></span><div><strong>{{ player.connected ? player.name : '等待玩家加入' }}<small v-if="player.seat === localSeat">你</small><small v-if="player.seat === 0">房主</small></strong><span>{{ !player.connected ? '空座位' : player.autoplay ? '托管中' : player.ready ? '准备就绪' : isActiveRound ? '正在对局' : '尚未准备' }}</span></div><AppIcon v-if="player.connected && player.ready" name="check" :size="20" /><span v-else class="status-dot" :class="{ 'status-warning': !player.connected }"></span></div></div>
        <div class="room-rule-summary"><span>{{ game.bidMode === 'score' ? '叫分制' : '叫抢制' }}</span><span>基础分 {{ game.baseScore }}</span><span>{{ game.settings.turnSeconds === 0 ? '不限时' : `${game.settings.turnSeconds} 秒出牌` }}</span></div>
        <p v-if="game.network.message" class="form-notice" :class="{ 'notice-error': !networkReady }"><AppIcon :name="networkReady ? 'info' : 'alert'" :size="18" />{{ game.network.message }}</p>
        <p v-if="commandError" class="inline-error" role="alert">{{ commandError }}</p>
      </template>
      <template v-else>
        <div class="room-tabs" role="tablist" aria-label="连接方式"><button id="host-tab" role="tab" :aria-selected="roomTab === 'host'" aria-controls="room-form" :class="{ active: roomTab === 'host' }" @click="roomTab = 'host'; commandError = ''"><AppIcon name="plus" :size="17" />创建房间</button><button id="join-tab" role="tab" :aria-selected="roomTab === 'join'" aria-controls="room-form" :class="{ active: roomTab === 'join' }" @click="roomTab = 'join'; commandError = ''"><AppIcon name="door" :size="17" />加入房间</button></div>
        <form id="room-form" class="room-form" role="tabpanel" :aria-labelledby="roomTab === 'host' ? 'host-tab' : 'join-tab'" @submit.prevent="roomTab === 'host' ? createRoom() : enterRoom()">
          <p class="room-intro">{{ roomTab === 'host' ? '开一张牌桌，把局域网地址告诉另外两位朋友。' : '输入房主的局域网地址，和朋友在同一张桌上碰面。' }}</p>
          <fieldset class="form-fields" :disabled="connectionBusy">
            <label class="field">你的昵称<input v-model="roomDraft.playerName" maxlength="16" autocomplete="nickname" placeholder="牌友怎么称呼你" required /></label>
            <div v-if="roomTab === 'join'" class="form-row address-row"><label class="field">房主地址<input v-model="roomDraft.address" autocomplete="off" spellcheck="false" placeholder="例如 192.168.1.8" required /></label><label class="field port-field">端口<input v-model.number="roomDraft.port" type="number" min="1" max="65535" required /></label></div>
            <template v-else><div class="form-row"><label class="field">房间端口<input v-model.number="roomDraft.port" type="number" min="1" max="65535" required /><small>默认 9528</small></label><label class="field">地主决定方式<select v-model="roomDraft.bidMode"><option value="score">叫分制</option><option value="rob">叫抢制</option></select></label></div><div class="form-row"><label class="field">基础分<div class="input-with-unit"><input v-model.number="roomDraft.baseScore" type="number" min="1" max="100" required /><span>分</span></div></label><label class="field">每次操作时间<div class="input-with-unit"><input v-model.number="roomDraft.turnSeconds" type="number" min="0" max="120" required /><span>秒</span></div><small>0 表示不限时</small></label></div></template>
          </fieldset>
          <p class="form-notice"><AppIcon name="wifi" :size="19" /><span>{{ roomTab === 'host' ? '创建房间会结束当前单人牌局。三人入座并准备后，由房主开始。' : '加入房间会结束当前单人牌局。连接过程中可以关闭此窗口。' }}</span></p>
          <p v-if="commandError" class="inline-error" role="alert">{{ commandError }}</p>
        </form>
      </template>
      <template #footer><template v-if="isLan"><button class="button button-danger-quiet" :disabled="connectionBusy" @click="exitRoom"><AppIcon name="door" :size="17" />{{ connectionBusy ? '正在离开' : '离开房间' }}</button><div class="room-footer-actions"><button v-if="isReadyStage" class="button button-outline" :class="{ 'button-ready': me?.ready }" :disabled="!readyAllowed" @click="toggleReady"><AppIcon :name="me?.ready ? 'check' : 'plus'" :size="16" />{{ me?.ready ? '取消准备' : '准备就绪' }}</button><button class="button button-gold" @click="closeDialog">回到牌桌</button></div></template><template v-else><button class="button button-outline" @click="closeDialog">稍后再说</button><button class="button button-gold" type="submit" form="room-form" :disabled="connectionBusy || controlBusy"><AppIcon :name="roomTab === 'host' ? 'plus' : 'arrow'" :size="17" />{{ connectionBusy ? '正在连接' : roomTab === 'host' ? '创建房间' : '加入房间' }}</button></template></template>
    </AppDialog>

    <AppDialog v-if="dialog === 'result' && game?.result" :title="won ? '这一局，打得漂亮。' : '好牌留给下一局。'" subtitle="本局结算" wide @close="closeDialog">
      <div class="result-hero" :class="{ 'result-won': won }"><div class="result-symbol"><AppIcon :name="won ? 'crown' : 'cards'" :size="40" /></div><div><span>{{ game.result.landlordWon ? '地主获胜' : '农民获胜' }}<em v-if="game.result.spring">{{ game.result.spring }}</em></span><strong>{{ resultDelta > 0 ? '+' : '' }}{{ resultDelta }}<small>分</small></strong></div><span class="result-team">{{ me?.role === 'landlord' ? '地主阵营' : '农民阵营' }}</span></div>
      <div class="result-formula"><span>基础分 <b>{{ game.result.baseScore }}</b></span><i>×</i><span>最终倍数 <b>{{ game.result.multiplier }}</b></span><i>=</i><span>单份积分 <b>{{ game.result.unitScore }}</b></span></div>
      <div class="result-table" role="table" aria-label="本局积分结算"><div class="result-table-head" role="row"><span role="columnheader">玩家</span><span role="columnheader">本局</span><span role="columnheader">累计积分</span></div><div v-for="player in [...game.players].sort((a, b) => a.seat - b.seat)" :key="player.seat" class="result-table-row" :class="{ 'result-self': player.seat === localSeat }" role="row"><span role="cell"><AppIcon :name="player.role === 'landlord' ? 'crown' : 'shield'" :size="17" /><strong>{{ player.name }}</strong><small>{{ player.seat === localSeat ? '你 · ' : '' }}{{ player.role === 'landlord' ? '地主' : '农民' }}</small></span><b role="cell" :class="(game.result.deltas[player.seat] ?? 0) > 0 ? 'points-positive' : 'points-negative'">{{ (game.result.deltas[player.seat] ?? 0) > 0 ? '+' : '' }}{{ game.result.deltas[player.seat] ?? 0 }}</b><span role="cell">{{ player.score }}</span></div></div>
      <p class="result-note">地主按两份、每位农民按一份结算，三人积分增减合计为零。</p>
      <p v-if="isLan" class="form-note">下一局需要三位玩家重新准备，随后由房主开始。</p>
      <p v-if="commandError" class="inline-error" role="alert">{{ commandError }}</p>
      <template #footer><button class="button button-outline" @click="closeDialog">回到牌桌</button><div class="room-footer-actions"><button v-if="isLan" class="button button-outline" :disabled="!readyAllowed" @click="toggleReady">{{ me?.ready ? '取消准备' : '准备下一局' }}</button><button class="button button-gold" :disabled="phaseButton.disabled" :title="phaseButton.title" @click="runPhase"><AppIcon name="play" :size="17" />再来一局</button></div></template>
    </AppDialog>

    <AppDialog v-if="dialog === 'rules'" title="坐下来，就会玩" subtitle="经典斗地主 · 本桌规则" wide @close="closeDialog">
      <div class="rules-lead"><span aria-hidden="true">♠</span><p>一副 54 张牌，三人对局。地主独自一队，两位农民合作；任意一方先出完手牌，该方获胜。</p></div>
      <div class="rules-section"><h3><span>01</span>发牌与叫牌</h3><p>每人先发 17 张，另留 3 张底牌。叫分制可选不叫、1 分、2 分或 3 分，只能叫当前合法的更高分；最高叫分者成为地主。叫抢制按提示选择叫地主或抢地主。地主拿到 3 张公开底牌，先出牌。所有人都不叫，本轮重新发牌且不计分。</p></div>
      <div class="rules-section"><h3><span>02</span>牌大，才能接</h3><p>点数从小到大为 3、4、5、6、7、8、9、10、J、Q、K、A、2、小王、大王，花色不分大小。跟牌须同牌型、同张数，并且主体点数更大；也可以选择不出。其余两人连续不出后，由最后出牌者自由领出。</p></div>
      <div class="rules-section"><h3><span>03</span>允许的牌型</h3><div class="rule-types"><p><b>基础牌型</b>单张、对子、三张、三带一、三带一对。</p><p><b>连续牌型</b>顺子至少 5 张，连对至少 3 对，连续三张至少 2 组；2 和大小王均不能进入这些连续主体。</p><p><b>飞机带翅膀</b>连续三张带等组数的单张或对子。翅膀不得包含主体点数；单张翅膀可用对子拆作两张，但不能用同点数的四张。对子翅膀为不同点数的对子。</p><p><b>四带二</b>四张同点数带两张单牌，两张单牌可以是一对；四带两对时，两对必须点数不同。</p><p><b>炸弹与火箭</b>四张同点数为炸弹，可压普通牌型；炸弹之间比较点数。大小王组成火箭，是最大的牌型。</p></div></div>
      <div class="rules-section"><h3><span>04</span>积分与托管</h3><p>叫牌确定初始倍数；每出一次炸弹或火箭，倍数 ×2。地主获胜且农民未出过牌，或农民获胜且地主仅出过一手，构成春天或反春，倍数再 ×2。单份积分为基础分 × 最终倍数，地主输赢两份，每位农民输赢一份，总和为零。</p><p>默认每次操作 20 秒，房间可设为不限时。超时由游戏服务处理；托管可随时取消。暂停会冻结倒计时与电脑操作，联机时由房主控制暂停和继续。</p></div>
      <p class="rules-bottom-note">这里没有筹码与兑换，只有一场好好打牌的时间。</p>
      <template #footer><span class="dialog-footer-note">单人双 AI · 局域网三人</span><button class="button button-gold" @click="closeDialog">知道了，回牌桌<AppIcon name="arrow" :size="17" /></button></template>
    </AppDialog>

    <AppDialog v-if="dialog === 'history'" title="刚才那手好牌" subtitle="本次牌桌 · 最近动态" @close="closeDialog">
      <ol v-if="allEvents.length" class="event-list"><li v-for="event in allEvents" :key="event.id"><span class="event-dot"></span><span>{{ event.text }}</span><small>#{{ event.id }}</small></li></ol><div v-else class="empty-history"><AppIcon name="history" :size="32" /><p>牌局开始后，这里会记录最近的公共操作。</p></div>
      <p class="form-note">仅展示游戏服务保留的最近记录。</p>
      <template #footer><button class="button button-gold" @click="closeDialog">回到牌桌</button></template>
    </AppDialog>
  </div>
</template>