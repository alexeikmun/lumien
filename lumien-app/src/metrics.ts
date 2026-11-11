export const sensors = [
  { name: 'CPU Load', binding: 'CPU load', text: 'CPU LOAD', key: 'cpu_load' },
  { name: 'CPU Temp', binding: 'CPU temperature', text: 'CPU TEMP', key: 'cpu_temp' },
  { name: 'GPU Load', binding: 'GPU load', text: 'GPU LOAD', key: 'gpu_load' },
  { name: 'GPU Temp', binding: 'GPU temperature', text: 'GPU TEMP', key: 'gpu_temp' },
  { name: 'Memory Usage', binding: 'Memory usage', text: 'MEMORY', key: 'ram' },
  { name: 'Download Speed', binding: 'Network download', text: 'DOWNLOAD', key: 'network_download' },
  { name: 'Upload Speed', binding: 'Network upload', text: 'UPLOAD', key: 'network_upload' },
] as const;

export const isNetwork = (binding: string) => binding === 'Network download' || binding === 'Network upload';
export const metricMax = (binding: string) => isNetwork(binding) ? Number.MAX_SAFE_INTEGER : 100;
export function formatMetric(binding: string, value: number | null | undefined) {
  const unit = isNetwork(binding) ? 'MB/s' : binding.toLowerCase().includes('temp') ? '°C' : '%';
  if (value == null || !Number.isFinite(value) || value < 0) return { text: '--', unit };
  if (isNetwork(binding)) {
    return { text: String(Number((value / 1e6).toFixed(2))), unit };
  }
  return { text: String(value), unit };
}
