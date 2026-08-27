export async function requestUnknownExtensionInsights<T>(
  samples: readonly string[],
  reason: (samples: string[]) => Promise<T[]>,
): Promise<T[] | null> {
  if (samples.length === 0) return null;
  return reason([...samples]);
}
