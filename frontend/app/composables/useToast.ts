export type ToastTone = 'success' | 'error' | 'info'
export interface ToastMessage {
  id: number
  message: string
  tone: ToastTone
  confirmation?: boolean
}

const confirmations = new WeakMap<object, Map<number, (accepted: boolean) => void>>()

export function useToast() {
  const app = useNuxtApp()
  const resolvers = confirmations.get(app) ?? new Map<number, (accepted: boolean) => void>()
  confirmations.set(app, resolvers)
  const owned = new Set<number>()
  const messages = useState<ToastMessage[]>('action-toasts', () => [])
  const sequence = useState('action-toast-sequence', () => 0)
  function show(message: string, tone: ToastTone = 'success') {
    if (!import.meta.client || !message) return
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
    if (!import.meta.client) return false
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
