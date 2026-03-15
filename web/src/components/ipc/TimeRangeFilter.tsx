/** Quick-select time range presets used by Sessions, Spawns, and Audit pages. */

export const TIME_RANGES = [
  { label: 'All time', value: '' },
  { label: '1 hour', value: '1' },
  { label: '24 hours', value: '24' },
  { label: '7 days', value: '168' },
  { label: '30 days', value: '720' },
];

/** Convert a time range value (hours string) to a `from_ts` epoch seconds, or undefined. */
export function timeRangeToTs(value: string): number | undefined {
  if (!value) return undefined;
  const hours = parseInt(value, 10);
  if (isNaN(hours) || hours <= 0) return undefined;
  return Math.floor(Date.now() / 1000) - hours * 3600;
}
