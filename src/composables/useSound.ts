import { onBeforeUnmount, onMounted, watch, type Ref } from 'vue'
import type { Snapshot } from '../types'

export function useSound(snapshot: Readonly<Ref<Snapshot | null>>) {
  let context: AudioContext | null = null
  let disposed = false
  let seenSession: number | null = null
  let seenAction = -1

  function unlock() {
    if (disposed || !snapshot.value?.settings.sound) return
    try {
      context ??= new AudioContext()
      if (context.state === 'suspended') void context.resume().catch(() => {})
    } catch { /* Sound is optional; native game actions remain available. */ }
  }

  function note(frequency: number, delay: number, duration: number, volume = 0.045, type: OscillatorType = 'sine') {
    if (!context || context.state !== 'running' || disposed) return
    const oscillator = context.createOscillator()
    const gain = context.createGain()
    const start = context.currentTime + delay
    oscillator.type = type
    oscillator.frequency.setValueAtTime(frequency, start)
    gain.gain.setValueAtTime(0, start)
    gain.gain.linearRampToValueAtTime(volume, start + 0.012)
    gain.gain.exponentialRampToValueAtTime(0.0001, start + duration)
    oscillator.connect(gain)
    gain.connect(context.destination)
    oscillator.start(start)
    oscillator.stop(start + duration + 0.025)
    oscillator.onended = () => { oscillator.disconnect(); gain.disconnect() }
  }

  watch(() => snapshot.value, value => {
    if (!value) return
    if (seenSession !== value.sessionId) {
      seenSession = value.sessionId
      seenAction = value.action.id
      return
    }
    if (value.action.id <= seenAction) return
    seenAction = value.action.id
    if (!value.settings.sound || !context || context.state !== 'running') return
    switch (value.action.kind) {
      case 'deal': [330, 392, 494].forEach((frequency, index) => note(frequency, index * 0.07, 0.12)); break
      case 'play': note(440, 0, 0.09); note(660, 0.035, 0.1, 0.025); break
      case 'pass': note(240, 0, 0.1, 0.025); break
      case 'bid': note(520, 0, 0.13); break
      case 'bomb': note(110, 0, 0.3, 0.06, 'triangle'); note(220, 0.05, 0.25, 0.035, 'triangle'); break
      case 'rocket': [392, 523, 784].forEach((frequency, index) => note(frequency, index * 0.08, 0.22)); break
      case 'finish': {
        const won = value.result && (value.result.landlordWon === (value.landlord === value.localSeat))
        ;(won ? [523, 659, 784, 1047] : [392, 330, 262]).forEach((frequency, index) => note(frequency, index * 0.13, 0.28))
        break
      }
    }
  })

  onMounted(() => {
    window.addEventListener('pointerdown', unlock, { passive: true })
    window.addEventListener('keydown', unlock)
  })
  onBeforeUnmount(() => {
    disposed = true
    window.removeEventListener('pointerdown', unlock)
    window.removeEventListener('keydown', unlock)
    if (context) void context.close().catch(() => {})
    context = null
  })
}
