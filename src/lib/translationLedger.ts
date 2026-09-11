export const SUPPORTED_LOCALES = ["ko", "en", "ja", "zh", "vi", "es", "de", "fr"] as const;

export type LocaleTag = (typeof SUPPORTED_LOCALES)[number];
export type ResourceVersion = string;
export type ScreenKey = string;

/**
 * Serializes all translation lookup identity dimensions without delimiter ambiguity.
 * Resource version is intentionally part of the key so copy from an older release cannot survive
 * a resource upgrade merely because locale and screen key are unchanged.
 */
export function translationLookupCacheKey(
  resourceVersion: ResourceVersion,
  locale: LocaleTag,
  screenKey: ScreenKey,
): string {
  return JSON.stringify([resourceVersion, locale, screenKey]);
}

type CachedTranslation = {
  resourceVersion: ResourceVersion;
  text: string;
};

/**
 * Disposable presentation cache. The versioned translation database remains authoritative;
 * cache identity includes the immutable resource version so an upgrade cannot reuse stale copy.
 */
export class TranslationLookupCache {
  private readonly entries = new Map<string, CachedTranslation>();

  /** Rejects unbounded or degenerate capacities rather than silently disabling eviction. */
  constructor(private readonly maxEntries: number) {
    if (!Number.isSafeInteger(maxEntries) || maxEntries < 1) {
      throw new RangeError("translation-cache-capacity-must-be-positive-safe-integer");
    }
  }

  /** Returns a cached projection and promotes it to the most-recently-used position. */
  get(resourceVersion: ResourceVersion, locale: LocaleTag, screenKey: ScreenKey): string | undefined {
    const key = translationLookupCacheKey(resourceVersion, locale, screenKey);
    const cached = this.entries.get(key);
    if (!cached) return undefined;
    this.entries.delete(key);
    this.entries.set(key, cached);
    return cached.text;
  }

  /** Replaces only the exact version/locale/key projection and evicts the least-recently-used entry. */
  set(resourceVersion: ResourceVersion, locale: LocaleTag, screenKey: ScreenKey, text: string): void {
    const key = translationLookupCacheKey(resourceVersion, locale, screenKey);
    this.entries.delete(key);
    this.entries.set(key, { resourceVersion, text });
    while (this.entries.size > this.maxEntries) {
      const oldestKey = this.entries.keys().next().value as string;
      this.entries.delete(oldestKey);
    }
  }

  /** Invalidates one immutable resource release without disturbing cached projections for others. */
  clearVersion(resourceVersion: ResourceVersion): void {
    for (const [key, cached] of this.entries) {
      if (cached.resourceVersion === resourceVersion) this.entries.delete(key);
    }
  }
}
