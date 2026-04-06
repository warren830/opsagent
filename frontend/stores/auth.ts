import { defineStore } from 'pinia'

interface User {
  id: string
  username: string
  role: 'super_admin' | 'tenant_admin'
  tenant_id: string | null
  email: string | null
}

export const useAuthStore = defineStore('auth', {
  state: () => ({
    user: null as User | null,
    isAuthenticated: false,
    isLoading: true,
  }),

  getters: {
    isSuperAdmin: (state) => state.user?.role === 'super_admin',
    tenantId: (state) => state.user?.tenant_id,
  },

  actions: {
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

    async logout() {
      try {
        const api = useApi()
        await api.post('/api/auth/logout')
      } finally {
        this.user = null
        this.isAuthenticated = false
      }
    },
  },
})
