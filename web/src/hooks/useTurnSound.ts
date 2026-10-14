import { useCallback, useRef } from 'react';

const STORAGE_KEY = 'player_sound_muted';

/** The sound is for the moment a decision passes to the viewer; the following ones of the same run stay quiet. */
export function shouldPlayTurnSound(wasViewerDecision: boolean, isViewerDecision: boolean): boolean {
  return isViewerDecision && !wasViewerDecision;
}

/**
 * Hook to manage turn sound notifications using Web Audio API.
 * Generates a simple beep sound when it's the player's turn.
 * Sound preference is persisted in localStorage.
 */
export function useTurnSound() {
  const audioContextRef = useRef<AudioContext | null>(null);

  // Initialize isMuted state from localStorage
  const getIsMuted = useCallback((): boolean => {
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      return stored === 'true';
    } catch {
      return false;
    }
  }, []);

  const toggleMute = useCallback((): boolean => {
    try {
      const currentMuted = getIsMuted();
      const newMuted = !currentMuted;
      localStorage.setItem(STORAGE_KEY, String(newMuted));
      return newMuted;
    } catch {
      return getIsMuted();
    }
  }, [getIsMuted]);

  const setMuted = useCallback((muted: boolean): void => {
    try {
      localStorage.setItem(STORAGE_KEY, String(muted));
    } catch {
      // Silently fail if localStorage is unavailable
    }
  }, []);

  /**
   * Play a simple beep notification sound using Web Audio API.
   * Generates a 800 Hz sine wave that fades out over 0.5 seconds.
   */
  const playTurnNotification = useCallback((): void => {
    // Don't play if muted
    if (getIsMuted()) return;

    try {
      // Create or reuse AudioContext
      if (!audioContextRef.current) {
        audioContextRef.current = new (window.AudioContext || (window as any).webkitAudioContext)();
      }

      const audioContext = audioContextRef.current;

      // Resume AudioContext if suspended (required for some browsers)
      if (audioContext.state === 'suspended') {
        audioContext.resume();
      }

      // Create oscillator and gain nodes
      const oscillator = audioContext.createOscillator();
      const gainNode = audioContext.createGain();

      // Connect nodes: oscillator -> gain -> speakers
      oscillator.connect(gainNode);
      gainNode.connect(audioContext.destination);

      // Configure oscillator for a 800 Hz beep
      oscillator.frequency.value = 800;
      oscillator.type = 'sine';

      // Configure volume envelope: start at 0.3, fade to 0.01 over 0.5 seconds
      const now = audioContext.currentTime;
      gainNode.gain.setValueAtTime(0.3, now);
      gainNode.gain.exponentialRampToValueAtTime(0.01, now + 0.5);

      // Start and stop the oscillator
      oscillator.start(now);
      oscillator.stop(now + 0.5);
    } catch (error) {
      // Silently fail if Web Audio API is not available
      console.debug('Failed to play turn notification:', error);
    }
  }, [getIsMuted]);

  return {
    playTurnNotification,
    isMuted: getIsMuted(),
    setMuted,
    toggleMute,
  };
}
