export function formatSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}
export function formatCount(n: number): string {
  return n >= 10000 ? `${(n / 10000).toFixed(1)} 万` : String(n);
}
export function formatDate(ts?: number): string {
  return ts ? new Date(ts).toLocaleString("zh-CN", { hour12: false }) : "";
}
export function formatDuration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s} 秒`;
  const m = Math.floor(s / 60);
  return m < 60 ? `${m} 分 ${s % 60} 秒` : `${Math.floor(m / 60)} 时 ${m % 60} 分`;
}
export function formatSpeed(bps: number): string {
  return `${formatSize(bps)}/s`;
}
