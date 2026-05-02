/**
 * Composable that subscribes to `/api/incidents/{id}/timeline/stream` and
 * keeps a reactive, chronological timeline array in sync with the backend.
 *
 * Behaviour:
 * - On mount (or when `incidentId` changes), opens an EventSource.
 * - The backend sends a backlog of the last 50 events in chronological
 *   order on connect, then pushes live events as they occur. We dedupe by
 *   event id because `stream.lagged` can trigger a backlog reload.
 * - `timeline.event` frames are parsed into `IncidentTimelineEvent`.
 * - `stream.lagged` frames trigger a one-shot reload via `GET .../timeline`
 *   so no events are missed when the broadcast channel saturates.
 * - On unmount or id change the stream is closed cleanly.
 */

export interface IncidentTimelineEvent {
  id: string
  incident_id: string
  kind: string
  actor: Record<string, unknown>
  occurred_at: string
  service_id: string | null
  summary: string
  payload: Record<string, unknown>
  correlation_id: string | null
}

/**
 * Subscribes a component to the incident timeline SSE stream.
 *
 * @param incidentId reactive ref to the incident UUID. When the ref changes
 *   the old stream is torn down and a new one opens against the new id.
 */
export function useIncidentStream(incidentId: Ref<string | null | undefined>) {
  const events = ref<IncidentTimelineEvent[]>([])
  const connected = ref(false)
  const error = ref<string | null>(null)
  const lastEventAt = ref<number | null>(null)

  let source: EventSource | null = null
  let knownIds: Set<string> = new Set()

  function closeSource() {
    if (source) {
      source.close()
      source = null
    }
    connected.value = false
  }

  function reset() {
    events.value = []
    knownIds = new Set()
    error.value = null
    lastEventAt.value = null
  }

  function pushEvent(ev: IncidentTimelineEvent) {
    if (knownIds.has(ev.id)) return
    knownIds.add(ev.id)
    // Chronological append. Backend sends backlog oldest-first and live
    // events in fire order, so plain push keeps the array sorted.
    events.value.push(ev)
    lastEventAt.value = Date.now()
  }

  async function reloadBacklog(id: string) {
    try {
      const fetched = await $fetch<IncidentTimelineEvent[]>(
        `/api/incidents/${id}/timeline?limit=200`,
        { credentials: 'include' },
      )
      // Backend list endpoint returns newest-first — reverse so the
      // display order matches the stream.
      const ordered = [...fetched].reverse()
      reset()
      for (const ev of ordered) pushEvent(ev)
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'reload failed'
    }
  }

  function openSource(id: string) {
    closeSource()
    reset()
    try {
      // EventSource sends cookies by default for same-origin requests.
      // All browsers we target support `withCredentials`.
      const es = new EventSource(`/api/incidents/${id}/timeline/stream`, {
        withCredentials: true,
      })
      source = es

      es.addEventListener('open', () => {
        connected.value = true
        error.value = null
      })

      es.addEventListener('timeline.event', (ev: MessageEvent) => {
        try {
          const parsed = JSON.parse(ev.data) as IncidentTimelineEvent
          pushEvent(parsed)
        } catch {
          // Ignore unparseable frames; backend validates before sending.
        }
      })

      es.addEventListener('stream.lagged', () => {
        // Broadcast channel saturated — reload the full backlog so we
        // don't silently miss events.
        if (incidentId.value) {
          void reloadBacklog(incidentId.value)
        }
      })

      es.addEventListener('error', () => {
        connected.value = false
        // EventSource auto-retries with exponential backoff; we surface
        // the state so the UI can show a badge.
        error.value = 'disconnected'
      })
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'failed to open stream'
    }
  }

  // Reopen on id change; close on unmount.
  watch(
    () => incidentId.value,
    (id) => {
      if (!id) {
        closeSource()
        reset()
        return
      }
      openSource(id)
    },
    { immediate: true },
  )

  if (typeof window !== 'undefined') {
    onBeforeUnmount(() => {
      closeSource()
    })
  }

  return {
    events,
    connected,
    error,
    lastEventAt,
    /**
     * Force-reload the backlog from the REST endpoint. Useful when the
     * tab regains focus after a long sleep.
     */
    refresh: async () => {
      if (incidentId.value) {
        await reloadBacklog(incidentId.value)
      }
    },
  }
}
