import { ref, onMounted, onScopeDispose } from 'vue'
export function useSidebarResize() {
  const width = ref(320)
  const minimum = ref(220)
  const maximum = ref(560)
  const resizing = ref(false)
  let startX = 0
  let startWidth = 0
  let handle: HTMLElement | null = null
  let pointerId: number | null = null

  function setWidth(next: number) {
    width.value = Math.round(Math.min(maximum.value, Math.max(minimum.value, next)))
  }
  function updateBounds() {
    minimum.value = window.innerWidth <= 620 ? 160 : 220
    maximum.value = Math.max(minimum.value, Math.min(560, window.innerWidth - 320))
    setWidth(width.value)
  }
  function stop() {
    if (handle && pointerId !== null && handle.hasPointerCapture(pointerId)) {
      handle.releasePointerCapture(pointerId)
    }
    resizing.value = false
    handle = null
    pointerId = null
  }
  function start(event: PointerEvent) {
    if (!event.isPrimary || event.button !== 0) return
    event.preventDefault()
    handle = event.currentTarget as HTMLElement
    pointerId = event.pointerId
    handle.setPointerCapture(event.pointerId)
    startX = event.clientX
    startWidth = width.value
    resizing.value = true
  }
  function move(event: PointerEvent) {
    if (resizing.value && event.pointerId === pointerId)
      setWidth(startWidth + event.clientX - startX)
  }
  function keydown(event: KeyboardEvent) {
    const step = event.shiftKey ? 40 : 10
    if (event.key === 'ArrowLeft') setWidth(width.value - step)
    else if (event.key === 'ArrowRight') setWidth(width.value + step)
    else if (event.key === 'Home') setWidth(minimum.value)
    else if (event.key === 'End') setWidth(maximum.value)
    else return
    event.preventDefault()
  }
  onMounted(() => {
    updateBounds()
    window.addEventListener('resize', updateBounds)
  })
  onScopeDispose(() => {
    stop()
    window.removeEventListener('resize', updateBounds)
  })
  return {
    width,
    minimum,
    maximum,
    resizing,
    start,
    move,
    stop,
    keydown,
    reset: () => setWidth(320),
  }
}
