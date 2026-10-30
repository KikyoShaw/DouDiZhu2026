<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import AppIcon from './AppIcon.vue'
const props = withDefaults(defineProps<{ title: string; subtitle?: string; wide?: boolean; closeLabel?: string }>(), { subtitle: '', wide: false, closeLabel: '关闭弹窗' })
const emit = defineEmits<{ close: [] }>()
const panel = ref<HTMLElement | null>(null)
let previousFocus: HTMLElement | null = null
let previousOverflow = ''
let backdropDown = false
function backdropPointerDown(event: PointerEvent) { backdropDown = event.target === event.currentTarget }
function backdropPointerUp() { if (backdropDown) emit('close'); backdropDown = false }
function backdropPointerCancel() { backdropDown = false }
const selector = 'button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href], [tabindex="0"]'
function focusable() {
  return [...(panel.value?.querySelectorAll<HTMLElement>(selector) ?? [])].filter(element => element.getClientRects().length > 0 && element.getAttribute('aria-hidden') !== 'true')
}
function keydown(event: KeyboardEvent) {
  if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); emit('close'); return }
  if (event.key !== 'Tab') return
  const elements = focusable()
  if (!elements.length) { event.preventDefault(); panel.value?.focus(); return }
  const first = elements[0]!
  const last = elements[elements.length - 1]!
  if (event.shiftKey && (document.activeElement === first || document.activeElement === panel.value)) { event.preventDefault(); last.focus() }
  else if (!event.shiftKey && (document.activeElement === last || !panel.value?.contains(document.activeElement))) { event.preventDefault(); first.focus() }
}
onMounted(async () => {
  previousFocus = document.activeElement as HTMLElement | null
  previousOverflow = document.body.style.overflow
  document.body.style.overflow = 'hidden'
  window.addEventListener('keydown', keydown, true)
  await nextTick()
  const autofocus = panel.value?.querySelector<HTMLElement>('[data-autofocus]')
  ;(autofocus || focusable()[0] || panel.value)?.focus()
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', keydown, true)
  document.body.style.overflow = previousOverflow
  if (previousFocus?.isConnected) previousFocus.focus()
})
</script>

<template>
  <Teleport to="body">
    <div class="dialog-scrim" @pointerdown="backdropPointerDown" @pointerup.self="backdropPointerUp" @pointercancel="backdropPointerCancel">
      <section ref="panel" class="app-dialog" :class="{ 'dialog-wide': props.wide }" role="dialog" aria-modal="true" aria-labelledby="dialog-title" :aria-describedby="subtitle ? 'dialog-subtitle' : undefined" tabindex="-1">
        <header class="dialog-heading">
          <div><p v-if="subtitle" id="dialog-subtitle" class="eyebrow">{{ subtitle }}</p><h2 id="dialog-title">{{ title }}</h2></div>
          <button class="icon-button" :aria-label="closeLabel" @click="emit('close')"><AppIcon name="close" /></button>
        </header>
        <div class="dialog-content"><slot /></div>
        <footer v-if="$slots.footer" class="dialog-footer"><slot name="footer" /></footer>
      </section>
    </div>
  </Teleport>
</template>
