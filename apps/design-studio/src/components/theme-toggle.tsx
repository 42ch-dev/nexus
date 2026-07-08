import { Sun, Moon } from 'lucide-react';
import { useTheme } from '@/components/theme-provider';

/**
 * Inline theme toggle — icon-only control for the app chrome.
 *
 * Click cycles light ↔ dark (explicit; system mode is set on first paint only
 * and is replaced by explicit choice on toggle). Uses lucide-react Sun/Moon
 * icons matching the apps/web UI pattern.
 */
export function ThemeToggle() {
  const { resolvedTheme, toggleTheme } = useTheme();

  return (
    <button
      type="button"
      onClick={toggleTheme}
      className="p-2 rounded-md hover:bg-gray-alpha-100 dark:hover:bg-gray-alpha-200 transition-colors"
      aria-label={`Switch to ${resolvedTheme === 'dark' ? 'light' : 'dark'} theme`}
    >
      {resolvedTheme === 'dark' ? (
        <Sun className="w-5 h-5" />
      ) : (
        <Moon className="w-5 h-5" />
      )}
    </button>
  );
}
