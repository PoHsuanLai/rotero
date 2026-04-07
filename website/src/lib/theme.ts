import { writable } from 'svelte/store';
import { browser } from '$app/environment';

function getInitialTheme(): 'dark' | 'light' {
  if (!browser) return 'dark';
  const saved = localStorage.getItem('rotero-theme');
  if (saved === 'light' || saved === 'dark') return saved;
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

export const theme = writable<'dark' | 'light'>(getInitialTheme());

export function toggleTheme() {
  theme.update((t) => {
    const next = t === 'dark' ? 'light' : 'dark';
    if (browser) {
      localStorage.setItem('rotero-theme', next);
      document.documentElement.setAttribute('data-theme', next);
    }
    return next;
  });
}
