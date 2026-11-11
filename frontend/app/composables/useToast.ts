import { ref, watch, onScopeDispose, inject } from 'vue'
import type { InjectionKey, Ref } from 'vue'
export type ToastTone = 'success' | 'error' | 'info'
export interface ToastMessage {
  id: number
  message: string
  tone: ToastTone
  confirmation?: boolean
}

export function createToastState() {
  return {
    messages: ref<ToastMessage[]>([]),
    sequence: ref(0),
    resolvers: new Map<number, (accepted: boolean) => void>(),
  }
}
export const toastKey: InjectionKey<ReturnType<typeof createToastState>> = Symbol('toasts')

export function useToast() {
  const state = inject(toastKey)
  if (!state) throw new Error('Toast state must be provided by the application.')
  const { messages, sequence, resolvers } = state
  const owned = new Set<number>()
  function show(message: string, tone: ToastTone = 'success') {
    if (!message) return
    const id = ++sequence.value
    const transient = messages.value.filter((item) => !item.confirmation).slice(-4)
    messages.value = [
      ...messages.value.filter((item) => item.confirmation),
      ...transient,
      { id, message, tone },
    ]
  }
  function respond(id: number, accepted: boolean) {
    const resolve = resolvers.get(id)
    resolvers.delete(id)
    messages.value = messages.value.filter((message) => message.id !== id)
    resolve?.(accepted)
  }
  function dismiss(id: number) {
    respond(id, false)
  }
  async function confirm(message: string) {
    const id = ++sequence.value
    owned.add(id)
    try {
      return await new Promise<boolean>((resolve) => {
        resolvers.set(id, resolve)
        messages.value = [...messages.value, { id, message, tone: 'info', confirmation: true }]
      })
    } finally {
      owned.delete(id)
    }
  }
  function cancelConfirmations() {
    for (const id of owned) dismiss(id)
  }
  onScopeDispose(cancelConfirmations)
  return { messages, show, dismiss, confirm, respond, cancelConfirmations }
}

// Clear transient feedback after delivery so identical repeated actions are announced.
export function useActionFeedback(source: Ref<string>, tone: ToastTone = 'error') {
  const toast = useToast()
  watch(
    source,
    (message) => {
      if (!message) return
      toast.show(message, tone)
      source.value = ''
    },
    { flush: 'sync' },
  )
}
