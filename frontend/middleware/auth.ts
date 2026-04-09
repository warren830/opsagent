/**
 * Auth middleware — redirects unauthenticated users to /login
 */
export default defineNuxtRouteMiddleware(async (to) => {
  // Skip auth check for login page and OAuth callback pages
  if (to.path === '/login' || to.path.startsWith('/auth/')) return

  const authStore = useAuthStore()

  // Fetch user info if not loaded yet
  if (!authStore.user && authStore.isLoading) {
    await authStore.fetchMe()
  }

  // If not authenticated, try refreshing the access token before redirecting
  if (!authStore.isAuthenticated) {
    const refreshed = await authStore.refreshAccessToken()
    if (refreshed) {
      await authStore.fetchMe()
    }
  }

  // Redirect to login if still not authenticated
  if (!authStore.isAuthenticated) {
    return navigateTo('/login')
  }
})
