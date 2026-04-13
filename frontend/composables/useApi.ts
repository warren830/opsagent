/**
 * API client composable — all backend calls go through this.
 * In development, Nuxt proxy forwards /api/* to the Rust backend.
 * Automatically retries on 401 by refreshing the access token (once).
 */
export function useApi() {
  const config = useRuntimeConfig()
  const clientBase = config.public.apiBase || ''

  // SSR: call the backend directly via K8s service DNS (bypasses CloudFront)
  // Client: use relative URL (goes through CloudFront)
  const baseURL = import.meta.server ? (config.backendUrl || clientBase) : clientBase

  async function $fetch<T>(url: string, options?: RequestInit): Promise<T> {
    // Build headers with SSR cookie forwarding
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    }

    // SSR: forward cookies from the incoming browser request to the backend
    if (import.meta.server) {
      try {
        const requestHeaders = useRequestHeaders(['cookie'])
        if (requestHeaders.cookie) {
          headers['Cookie'] = requestHeaders.cookie
        }
      } catch {
        // useRequestHeaders may fail outside of Nuxt context — ignore
      }
    }

    // Merge with caller-provided headers
    if (options?.headers) {
      Object.assign(headers, options.headers)
    }

    const response = await fetch(`${baseURL}${url}`, {
      credentials: 'include',
      ...options,
      headers,
    })

    // Auto-refresh on 401 (skip for auth endpoints to avoid loops)
    if (response.status === 401 && !url.includes('/api/auth/refresh') && !url.includes('/api/auth/login')) {
      const authStore = useAuthStore()
      const refreshed = await authStore.refreshAccessToken()
      if (refreshed) {
        // Retry the original request once (reuse SSR cookie headers)
        const retryResponse = await fetch(`${baseURL}${url}`, {
          credentials: 'include',
          ...options,
          headers,
        })

        if (!retryResponse.ok) {
          const error = await retryResponse.json().catch(() => ({ error: retryResponse.statusText }))
          throw new ApiError(retryResponse.status, error.error || 'Unknown error')
        }

        return retryResponse.json()
      }
    }

    if (!response.ok) {
      const error = await response.json().catch(() => ({ error: response.statusText }))
      throw new ApiError(response.status, error.error || 'Unknown error')
    }

    return response.json()
  }

  async function fetchBlob(url: string): Promise<Blob> {
    // Build headers with SSR cookie forwarding
    const blobHeaders: Record<string, string> = {}
    if (import.meta.server) {
      try {
        const requestHeaders = useRequestHeaders(['cookie'])
        if (requestHeaders.cookie) {
          blobHeaders['Cookie'] = requestHeaders.cookie
        }
      } catch {
        // useRequestHeaders may fail outside of Nuxt context — ignore
      }
    }

    const response = await fetch(`${baseURL}${url}`, {
      credentials: 'include',
      headers: blobHeaders,
    })
    if (!response.ok) {
      throw new ApiError(response.status, response.statusText)
    }
    return response.blob()
  }

  return {
    get: <T>(url: string) => $fetch<T>(url),
    getBlob: (url: string) => fetchBlob(url),
    post: <T>(url: string, body?: unknown) =>
      $fetch<T>(url, { method: 'POST', body: body ? JSON.stringify(body) : undefined }),
    put: <T>(url: string, body?: unknown) =>
      $fetch<T>(url, { method: 'PUT', body: body ? JSON.stringify(body) : undefined }),
    del: <T>(url: string) => $fetch<T>(url, { method: 'DELETE' }),
  }
}

export class ApiError extends Error {
  status: number

  constructor(status: number, message: string) {
    super(message)
    this.status = status
    this.name = 'ApiError'
  }
}
