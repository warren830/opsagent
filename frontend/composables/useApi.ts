/**
 * API client composable — all backend calls go through this.
 * In development, Nuxt proxy forwards /api/* to the Rust backend.
 * Automatically retries on 401 by refreshing the access token (once).
 */
export function useApi() {
  const config = useRuntimeConfig()
  const baseURL = config.public.apiBase || ''

  async function $fetch<T>(url: string, options?: RequestInit): Promise<T> {
    const response = await fetch(`${baseURL}${url}`, {
      credentials: 'include', // send HttpOnly cookies
      headers: {
        'Content-Type': 'application/json',
        ...options?.headers,
      },
      ...options,
    })

    // Auto-refresh on 401 (skip for auth endpoints to avoid loops)
    if (response.status === 401 && !url.includes('/api/auth/refresh') && !url.includes('/api/auth/login')) {
      const authStore = useAuthStore()
      const refreshed = await authStore.refreshAccessToken()
      if (refreshed) {
        // Retry the original request once
        const retryResponse = await fetch(`${baseURL}${url}`, {
          credentials: 'include',
          headers: {
            'Content-Type': 'application/json',
            ...options?.headers,
          },
          ...options,
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
    const response = await fetch(`${baseURL}${url}`, {
      credentials: 'include',
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
