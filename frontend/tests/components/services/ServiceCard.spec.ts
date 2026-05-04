/**
 * ServiceCard dispatcher — asserts kind→card resolution via the pure
 * helper (resolveCardKind). The SFC itself only wraps the helper in a
 * Vue switch so these string assertions document the mapping without
 * needing `@vitejs/plugin-vue`.
 */
import { describe, it, expect } from 'vitest'
import { resolveCardKind } from '~/components/services/cardRegistry'

describe('ServiceCard (dispatcher)', () => {
  it('resolves each of the 6 runtime kinds to a distinct id', () => {
    const resolved = ['eks', 'ec2', 'rds', 'lambda', 'external', 'generic'].map(resolveCardKind)
    expect(new Set(resolved).size).toBe(6)
  })

  it.each(['eks', 'ec2', 'rds', 'lambda', 'external', 'generic'])(
    'kind %s resolves to itself',
    (kind) => {
      expect(resolveCardKind(kind)).toBe(kind)
    },
  )

  it('falls back to generic for unknown / missing kinds', () => {
    expect(resolveCardKind(null)).toBe('generic')
    expect(resolveCardKind(undefined)).toBe('generic')
    expect(resolveCardKind('ec2-xyz')).toBe('generic')
    expect(resolveCardKind(42)).toBe('generic')
  })
})
