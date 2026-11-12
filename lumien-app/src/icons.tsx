// Hand-drawn 24px stroke icons (no icon dependency).
const paths = {
  plus: 'M12 5v14M5 12h14',
  minus: 'M5 12h14',
  undo: 'M9 14 4 9l5-5M4 9h10.5a5.5 5.5 0 0 1 0 11H11',
  redo: 'm15 14 5-5-5-5M20 9H9.5a5.5 5.5 0 0 0 0 11H13',
  save: 'M5 3h10.2a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM17 21v-7H7v7M7 3v4h7',
  file: 'M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9zM14 3v6h6M12 12v6M9 15h6',
  eye: 'M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12zM12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z',
  eyeOff: 'M3 3l18 18M10.6 5.1A10 10 0 0 1 12 5c6.5 0 10 7 10 7a17 17 0 0 1-3.2 4.2M6.6 6.6C3.7 8.5 2 12 2 12s3.5 7 10 7a9.7 9.7 0 0 0 5.4-1.6M9.9 9.9a3 3 0 0 0 4.2 4.2',
  lock: 'M7 11V7a5 5 0 0 1 10 0v4M6 11h12a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-8a1 1 0 0 1 1-1z',
  unlock: 'M7 11V7a5 5 0 0 1 9.9-1M6 11h12a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-8a1 1 0 0 1 1-1z',
  send: 'M12 15V3M7 8l5-5 5 5M4 15v4a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-4',
  screenOff: 'M3 5h18v12H3zM8 21h8M12 17v4M9.5 8.5l5 5M14.5 8.5l-5 5',
  gauge: 'M12 14l3.5-3.5M4.9 19a9 9 0 1 1 14.2 0',
  text: 'M4 7V5h16v2M9 19h6M12 5v14',
  layers: 'M12 3 2 8l10 5 10-5zM2 13l10 5 10-5',
  grid: 'M4 4h16v16H4zM4 10h16M4 15h16M10 4v16M15 4v16',
  magnet: 'M5 4h4v7a3 3 0 0 0 6 0V4h4v7a7 7 0 0 1-14 0zM5 8h4M15 8h4',
  split: 'M3 5h18v14H3zM12 5v14',
  bend: 'M3 18h6a6 6 0 0 0 6-6V6h6',
  image: 'M3 5h18v14H3zM3 16l5-5 4 4 3-3 6 6M15.5 8.5h.01',
  fit: 'M8 3H5a2 2 0 0 0-2 2v3M21 8V5a2 2 0 0 0-2-2h-3M3 16v3a2 2 0 0 0 2 2h3M16 21h3a2 2 0 0 0 2-2v-3',
  play: 'M7 4v16l13-8z',
  trash: 'M3 6h18M8 6V4h8v2m3 0-1 14H6L5 6m4 4v6m6-6v6',
  copy: 'M9 9h11v11H9zM5 15H4V4h11v1',
  up: 'M12 19V5M5 12l7-7 7 7',
  down: 'M12 5v14M19 12l-7 7-7-7',
  film: 'M3 4h18v16H3zM7 4v16M17 4v16M3 9h4M3 15h4M17 9h4M17 15h4',
  palette: 'M12 3a9 9 0 1 0 0 18c1 0 1.5-.7 1.5-1.5 0-.9-.8-1.3-.8-2.2 0-.8.7-1.5 1.5-1.5H16a5 5 0 0 0 5-5c0-4.3-4-7.8-9-7.8zM7.5 12h.01M9.5 7.5h.01M14.5 7.5h.01',
  video: 'M2 7a3 3 0 0 1 3-3h14a3 3 0 0 1 3 3v10a3 3 0 0 1-3 3H5a3 3 0 0 1-3-3zM10 9l5 3-5 3z',
  download: 'M12 4v12M7 11l5 5 5-5M4 20h16',
  check: 'M5 12l5 5 9-10',
  pointer: 'M5 3l6 17 2.5-7.5L21 10z',
  cpu: 'M7 7h10v10H7zM10 3v4M14 3v4M10 17v4M14 17v4M3 10h4M3 14h4M17 10h4M17 14h4',
  thermometer: 'M14 14.8V5a2 2 0 0 0-4 0v9.8a4 4 0 1 0 4 0z',
  memory: 'M3 7h18v10H3zM7 11v2M11 11v2M15 11v2M7 17v3M17 17v3',
  arrowDown: 'M12 4v16M6 14l6 6 6-6',
  arrowUp: 'M12 20V4M6 10l6-6 6 6',
  logo: 'M12 2 3 7v10l9 5 9-5V7zM12 22V12M3 7l9 5 9-5',
} as const;

export type IconName = keyof typeof paths;

export function Icon({ name, size = 16 }: { name: IconName; size?: number }) {
  return (
    <svg className="icon" aria-hidden="true" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
      <path d={paths[name]} />
    </svg>
  );
}

export const sensorIcon = (binding: string): IconName =>
  binding.includes('temperature') ? 'thermometer'
    : binding === 'Memory usage' ? 'memory'
    : binding === 'Network download' ? 'arrowDown'
    : binding === 'Network upload' ? 'arrowUp'
    : 'cpu';
