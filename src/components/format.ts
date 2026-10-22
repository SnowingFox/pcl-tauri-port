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
