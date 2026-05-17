/**
 * Global notification composable — real-time via SSE, fallback to polling.
 * Shared singleton across all components (module-level refs).
 */
import { ref, readonly } from 'vue'

export interface AppNotification {
  id: string
  user_id: string
  tenant_id: string | null
  event_type: string
  title: string
  description: string
  payload: Record<string, unknown>
  related_id: string | null
  is_read: boolean
  created_at: string
}

const unreadCount = ref(0)
const notifications = ref<AppNotification[]>([])
const loaded = ref(false)
/** The most recent notification received via SSE — pages can watch this to react in real-time */
const lastNotification = ref<AppNotification | null>(null)
let eventSource: EventSource | null = null
let reconnectTimer: ReturnType<typeof setTimeout> | null = null
let fallbackPolling: ReturnType<typeof setInterval> | null = null

export function useNotifications() {
  const api = useApi()

  async function fetchUnreadCount() {
    try {
      const data = await api.get<{ count: number }>('/api/notifications/unread-count')
      unreadCount.value = data.count
    } catch {
      // ignore — user may not be authenticated yet
    }
  }

  async function fetchNotifications(limit = 50, offset = 0) {
    try {
      const data = await api.get<AppNotification[]>(
        `/api/notifications?limit=${limit}&offset=${offset}`,
      )
      if (offset === 0) {
        notifications.value = data
      } else {
        notifications.value.push(...data)
      }
      loaded.value = true
      return data
    } catch {
      return []
    }
  }

  async function markRead(id: string) {
    // Optimistic update
    const n = notifications.value.find(item => item.id === id)
    if (n && !n.is_read) {
      n.is_read = true
      unreadCount.value = Math.max(0, unreadCount.value - 1)
    }
    try {
      await api.post(`/api/notifications/${id}/read`)
    } catch {
      // Revert on failure
      if (n) {
        n.is_read = false
        unreadCount.value++
      }
    }
  }

  async function markAllRead() {
    const prevCount = unreadCount.value
    const prevUnreadIds = notifications.value.filter(n => !n.is_read).map(n => n.id)
    // Optimistic update
    unreadCount.value = 0
    notifications.value.forEach(n => { n.is_read = true })
    try {
      await api.post('/api/notifications/read-all')
    } catch {
      // Revert on failure
      unreadCount.value = prevCount
      notifications.value.forEach(n => {
        if (prevUnreadIds.includes(n.id)) n.is_read = false
      })
    }
  }

  /** Alias for fetchNotifications — used by NotificationBell */
  async function refresh() {
    return fetchNotifications(50, 0)
  }

  /** Connect to SSE stream for real-time notifications */
  function connect() {
    // Only run on client
    if (import.meta.server) return
    disconnect()

    // Fetch initial unread count
    fetchUnreadCount()

    try {
      eventSource = new EventSource('/api/notifications/stream', { withCredentials: true })

      eventSource.onmessage = (event) => {
        try {
          const notification: AppNotification = JSON.parse(event.data)
          unreadCount.value++
          lastNotification.value = notification
          // Prepend to list if already loaded
          if (loaded.value) {
            notifications.value = [notification, ...notifications.value]
          }
        } catch {
          // ignore malformed events
        }
      }

      eventSource.onerror = () => {
        // Close broken connection and schedule reconnect
        eventSource?.close()
        eventSource = null
        scheduleReconnect()
      }

      // SSE connected — stop fallback polling if running
      stopFallbackPolling()
    } catch {
      // EventSource failed to construct — use polling fallback
      startFallbackPolling()
    }
  }

  function scheduleReconnect() {
    if (reconnectTimer) return
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null
      // Sync unread count on reconnect (may have missed events)
      fetchUnreadCount()
      connect()
    }, 3000)
  }

  function startFallbackPolling() {
    if (fallbackPolling) return
    fallbackPolling = setInterval(fetchUnreadCount, 30_000)
  }

  function stopFallbackPolling() {
    if (fallbackPolling) {
      clearInterval(fallbackPolling)
      fallbackPolling = null
    }
  }

  function disconnect() {
    if (eventSource) {
      eventSource.close()
      eventSource = null
    }
    if (reconnectTimer) {
      clearTimeout(reconnectTimer)
      reconnectTimer = null
    }
    stopFallbackPolling()
  }

  return {
    unreadCount: readonly(unreadCount),
    notifications: readonly(notifications),
    loaded: readonly(loaded),
    lastNotification: readonly(lastNotification),
    fetchUnreadCount,
    fetchNotifications,
    markRead,
    markAllRead,
    refresh,
    connect,
    disconnect,
  }
}
