/**
 * Auth middleware — redirects unauthenticated users to /login
 */
export default defineNuxtRouteMiddleware(async (to) => {
  // Skip auth check for login page
  if (to.path === '/login') return

  const authStore = useAuthStore()

  // Fetch user info if not loaded yet
  if (!authStore.user && authStore.isLoading) {
    await authStore.fetchMe()
  }

  // Redirect to login if not authenticated
  if (!authStore.isAuthenticated) {
    return navigateTo('/login')
  }
})
