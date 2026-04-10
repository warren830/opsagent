import { defineStore } from 'pinia'

interface User {
  id: string
  username: string
  role: 'super_admin' | 'member'
  tenant_id: string | null
  email: string | null
  auth_method: string
}

interface AuthProviders {
  local: boolean
  microsoft: boolean
  cognito: boolean
  is_cloud: boolean
}

export const useAuthStore = defineStore('auth', {
  state: () => ({
    user: null as User | null,
    isAuthenticated: false,
    isLoading: true,
    providers: null as AuthProviders | null,
    isRefreshing: false,
  }),

  getters: {
    isSuperAdmin: (state) => state.user?.role === 'super_admin',
    tenantId: (state) => state.user?.tenant_id,
  },

  actions: {
    async fetchProviders() {
      try {
        const api = useApi()
        this.providers = await api.get<AuthProviders>('/api/auth/providers')
      } catch {
        // Default to local-only if providers endpoint fails
        this.providers = { local: true, microsoft: false, cognito: false }
      }
    },

    async fetchMe() {
      this.isLoading = true
      try {
        const api = useApi()
        this.user = await api.get<User>('/api/auth/me')
        this.isAuthenticated = true
      } catch {
        this.user = null
        this.isAuthenticated = false
      } finally {
        this.isLoading = false
      }
    },

    async login(username: string, password: string) {
      const api = useApi()
      const response = await api.post<{ user: User; token: string }>('/api/auth/login', {
        username,
        password,
      })
      this.user = response.user
      this.isAuthenticated = true
    },

    /** Called after OAuth callback completes — tokens are already in cookies */
    setOAuthUser(user: User) {
      this.user = user
      this.isAuthenticated = true
    },

    /** Refresh access token using refresh token cookie */
    async refreshAccessToken(): Promise<boolean> {
      if (this.isRefreshing) return false
      this.isRefreshing = true
      try {
        const api = useApi()
        const response = await api.post<{ user: User; token: string }>('/api/auth/refresh')
        this.user = response.user
        this.isAuthenticated = true
        return true
      } catch {
        this.user = null
        this.isAuthenticated = false
        return false
      } finally {
        this.isRefreshing = false
      }
    },

    async logout() {
      try {
        const api = useApi()
        await api.post('/api/auth/logout')
      } finally {
        this.user = null
        this.isAuthenticated = false
      }
    },

    async logoutAll() {
      try {
        const api = useApi()
        await api.post('/api/auth/revoke-all')
        await api.post('/api/auth/logout')
      } finally {
        this.user = null
        this.isAuthenticated = false
      }
    },
  },
})
