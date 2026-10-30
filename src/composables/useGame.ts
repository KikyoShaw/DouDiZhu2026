import { computed, onBeforeUnmount, onMounted, ref, shallowRef } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { GameAction, Hint, Settings, Snapshot } from '../types'

export const defaultSettings: Settings = {
  playerName: '我', bidMode: 'score', baseScore: 1, turnSeconds: 20, sound: true, showHints: true,
}

export function useGame() {
  const snapshot = shallowRef<Snapshot | null>(null)
  const loading = ref(true)
  const startupError = ref('')
  const commandError = ref('')
  const toast = ref<{ text: string; error: boolean } | null>(null)
  const turnBusy = ref(false)
  const controlBusy = ref(false)
  const autoplayBusy = ref(false)
  const readyBusy = ref(false)
  const hintBusy = ref(false)
  const connectionBusy = ref(false)
  const preferencesBusy = ref(false)
  const remainingMs = ref<number | null>(null)
  const nativeAvailable = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
  let disposed = false
  let unlisten: UnlistenFn | null = null
  let startPromise: Promise<void> | null = null
  let toastTimer: ReturnType<typeof setTimeout> | undefined
  let lockTimer: ReturnType<typeof setTimeout> | undefined
  let clock: ReturnType<typeof setInterval> | undefined
  let anchor = 0
  let anchorRemaining: number | null = null
  let pendingTurn: { sessionId: number; revision: number } | null = null

  const localSeat = computed(() => ((snapshot.value?.localSeat ?? 0) % 3 + 3) % 3)
  const me = computed(() => snapshot.value?.players.find(player => player.seat === localSeat.value))
  const networkReady = computed(() => {
    const value = snapshot.value
    return !!value && (value.mode === 'single' ? value.network.status === 'offline' : ['hosting', 'connected'].includes(value.network.status))
  })
  const isOwnTurn = computed(() => !!snapshot.value && snapshot.value.turn === localSeat.value)
  const canAct = computed(() => {
    const value = snapshot.value
    return !!value && value.lifecycle === 'running' && ['bidding', 'playing'].includes(value.stage)
      && isOwnTurn.value && !me.value?.autoplay && networkReady.value && !turnBusy.value && !controlBusy.value && !connectionBusy.value
  })
  const canSelect = computed(() => canAct.value && snapshot.value?.stage === 'playing')

  function notify(text: string, error = false) {
    if (disposed) return
    if (toastTimer) clearTimeout(toastTimer)
    toast.value = { text, error }
    toastTimer = setTimeout(() => { toast.value = null }, error ? 6500 : 3800)
  }

  function readableError(error: unknown): string {
    if (typeof error === 'string') return error
    if (error instanceof Error) return error.message
    if (error && typeof error === 'object' && 'message' in error) return String(error.message)
    return '操作未完成，请稍后重试。'
  }

  function report(error: unknown) {
    const text = readableError(error)
    commandError.value = text
    notify(text, true)
  }

  function releaseTurn() {
    pendingTurn = null
    turnBusy.value = false
    if (lockTimer) clearTimeout(lockTimer)
    lockTimer = undefined
  }

  function tick() {
    const value = snapshot.value
    const counting = value && value.lifecycle === 'running' && ['bidding', 'playing'].includes(value.stage)
      && value.turn !== null && networkReady.value && anchorRemaining !== null
    remainingMs.value = anchorRemaining === null ? null : Math.max(0, anchorRemaining - (counting ? performance.now() - anchor : 0))
  }

  function accept(incoming: Snapshot) {
    if (disposed) return false
    const current = snapshot.value
    if (current && (incoming.sessionId < current.sessionId
      || (incoming.sessionId === current.sessionId && incoming.revision <= current.revision))) return false
    const awaitingGuest = !!pendingTurn && current?.network.role === 'guest'
    snapshot.value = incoming
    anchor = performance.now()
    anchorRemaining = incoming.remainingMs
    tick()
    if (incoming.network.message && incoming.network.message !== current?.network.message) {
      const isError = incoming.network.status === 'disconnected' || awaitingGuest
      if (isError) commandError.value = incoming.network.message
      notify(incoming.network.message, isError)
    }
    if (pendingTurn && (incoming.sessionId > pendingTurn.sessionId
      || (incoming.sessionId === pendingTurn.sessionId && incoming.revision > pendingTurn.revision)
      || !networkReady.value)) releaseTurn()
    startupError.value = ''
    return true
  }

  function tokens(value: Snapshot) {
    return { revision: value.revision, sessionId: value.sessionId, roundId: value.roundId, turnToken: value.turnToken }
  }

  function sameTurn(value: Pick<Snapshot, 'revision' | 'sessionId' | 'roundId' | 'turnToken'>) {
    const current = snapshot.value
    return !!current && current.sessionId === value.sessionId && current.revision === value.revision
      && current.roundId === value.roundId && current.turnToken === value.turnToken
  }

  async function connect() {
    if (startPromise) return startPromise
    startPromise = (async () => {
      loading.value = true
      startupError.value = ''
      try {
        if (!nativeAvailable) throw new Error('当前页面未连接原生游戏服务。请从桌面应用打开。')
        if (!unlisten) {
          const stop = await listen<Snapshot>('game:snapshot', event => accept(event.payload))
          if (disposed) { stop(); return }
          unlisten = stop
        }
        const incoming = await invoke<Snapshot>('get_snapshot')
        accept(incoming)
      } catch (error) {
        if (!disposed) startupError.value = readableError(error)
      } finally {
        loading.value = false
        startPromise = null
      }
    })()
    return startPromise
  }

  async function action(actionValue: GameAction): Promise<boolean> {
    const value = snapshot.value
    if (!value || !networkReady.value) { notify('连接尚未就绪，请先检查房间状态。', true); return false }
    const isTurnAction = ['play', 'bid', 'pass'].includes(actionValue.type)
    const busy = actionValue.type === 'autoplay' ? autoplayBusy : actionValue.type === 'ready' ? readyBusy : isTurnAction ? turnBusy : controlBusy
    if (busy.value || (isTurnAction && !canAct.value)) return false
    commandError.value = ''
    busy.value = true
    if (isTurnAction) pendingTurn = { sessionId: value.sessionId, revision: value.revision }
    try {
      const incoming = await invoke<Snapshot>('game_action', { action: actionValue, ...tokens(value) })
      if (disposed) return false
      accept(incoming)
      if (isTurnAction && pendingTurn) {
        // A LAN guest receives its old snapshot until the host acknowledges the action.
        if (value.network.role === 'guest' && sameTurn(value)) {
          lockTimer = setTimeout(() => {
            if (!pendingTurn || disposed) return
            report('房主尚未确认操作，请检查连接。收到房主更新后才能再次出牌。')
          }, 12000)
        } else releaseTurn()
      }
      return true
    } catch (error) {
      if (isTurnAction) releaseTurn()
      report(error)
      return false
    } finally {
      if (!isTurnAction) busy.value = false
    }
  }

  async function getHint(): Promise<Hint | null> {
    const value = snapshot.value
    if (!value || !canSelect.value || hintBusy.value) return null
    hintBusy.value = true
    commandError.value = ''
    try {
      const hint = await invoke<Hint>('get_hint', tokens(value))
      if (!sameTurn(value) || !sameTurn(hint) || !canSelect.value || disposed) return null
      const owned = new Set(snapshot.value!.myHand.map(card => card.id))
      if (!hint.cards.every(id => owned.has(id))) return null
      return hint
    } catch (error) { report(error); return null }
    finally { hintBusy.value = false }
  }

  async function connectionCommand(command: string, args?: Record<string, unknown>) {
    if (connectionBusy.value || controlBusy.value) return false
    connectionBusy.value = true
    commandError.value = ''
    try {
      const incoming = await invoke<Snapshot>(command, args)
      if (disposed) return false
      accept(incoming)
      return true
    } catch (error) { report(error); return false }
    finally { connectionBusy.value = false }
  }

  async function newMatch(settings: Settings) {
    return connectionCommand('new_match', { settings })
  }
  async function hostLan(port: number, settings: Settings) {
    return connectionCommand('host_lan', { port, settings })
  }
  async function joinLan(address: string, port: number, playerName: string) {
    return connectionCommand('join_lan', { address, port, playerName })
  }
  async function leaveLan() { return connectionCommand('leave_lan') }
  async function preferences(sound: boolean, showHints: boolean) {
    if (preferencesBusy.value || !snapshot.value) return false
    preferencesBusy.value = true
    commandError.value = ''
    try {
      accept(await invoke<Snapshot>('update_preferences', { sound, showHints }))
      return true
    } catch (error) { report(error); return false }
    finally { preferencesBusy.value = false }
  }

  onMounted(() => {
    void connect()
    clock = setInterval(tick, 100)
  })
  onBeforeUnmount(() => {
    disposed = true
    unlisten?.()
    if (clock) clearInterval(clock)
    if (toastTimer) clearTimeout(toastTimer)
    if (lockTimer) clearTimeout(lockTimer)
  })

  return {
    snapshot, loading, startupError, commandError, toast, localSeat, me, networkReady, isOwnTurn,
    canAct, canSelect, turnBusy, controlBusy, autoplayBusy, readyBusy, hintBusy, connectionBusy,
    preferencesBusy, remainingMs, nativeAvailable, connect, action, getHint, newMatch, hostLan,
    joinLan, leaveLan, preferences, notify,
  }
}