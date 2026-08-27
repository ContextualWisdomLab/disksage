export function isCurrentInventoryRequest(
  requestedRoot: string,
  requestedGeneration: number,
  currentRoot: string | null,
  currentGeneration: number,
): boolean {
  return requestedRoot === currentRoot && requestedGeneration === currentGeneration;
}

export async function requestUnknownExtensionInsights<T>(
  samples: readonly string[],
  reason: (samples: string[]) => Promise<T[]>,
): Promise<T[] | null> {
  if (samples.length === 0) return null;
  return reason([...samples]);
}
