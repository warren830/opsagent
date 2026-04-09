/**
 * Proxy GET /api/issues/:id/rca/status to backend.
 */
export default defineEventHandler(async (event) => {
  const id = getRouterParam(event, 'id')
  const backendUrl = `http://localhost:3080/api/issues/${id}/rca/status`

  const cookie = getHeader(event, 'cookie') || ''

  const response = await fetch(backendUrl, {
    headers: { 'Cookie': cookie },
  })

  if (!response.ok) {
    const err = await response.text().catch(() => 'Backend error')
    throw createError({
      statusCode: response.status,
      statusMessage: err,
    })
  }

  return await response.json()
})
