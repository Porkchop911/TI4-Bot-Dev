import { useCallback, useState } from 'react';
import { AutoResolveNotification } from '../components/AutoResolveToast';

/**
 * Hook to manage auto-resolve toast notifications.
 * Handles creation, display, and dismissal of toasts.
 */
export function useAutoResolveToasts() {
  const [toasts, setToasts] = useState<AutoResolveNotification[]>([]);

  /**
   * Show a new auto-resolve toast notification.
   * @param decisionType The type of decision (e.g., "Strategy Card", "System")
   * @param selectedValue The label/name of the selected option
   * @param reason Why there was no choice; the toast says "the only legal option" when absent
   */
  const showToast = useCallback(
    (decisionType: string, selectedValue: string, reason?: string) => {
      const id = crypto.randomUUID();
      const notification: AutoResolveNotification = {
        id,
        decisionType,
        selectedValue,
        ...(reason ? { reason } : {}),
      };
      setToasts((prev) => [...prev, notification]);
    },
    []
  );

  /**
   * Dismiss a toast by its ID.
   * @param id The notification ID to remove
   */
  const dismissToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  /**
   * Clear all toasts immediately.
   */
  const clearAllToasts = useCallback(() => {
    setToasts([]);
  }, []);

  return { toasts, showToast, dismissToast, clearAllToasts };
}
