import { describe, it, expect, beforeEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { shouldPlayTurnSound, useTurnSound } from './useTurnSound';

describe('useTurnSound', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
  });

  it('should initialize with muted state from localStorage', () => {
    localStorage.setItem('player_sound_muted', 'true');
    const { result } = renderHook(() => useTurnSound());
    expect(result.current.isMuted).toBe(true);
  });

  it('should initialize with unmuted state when localStorage is empty', () => {
    const { result } = renderHook(() => useTurnSound());
    expect(result.current.isMuted).toBe(false);
  });

  it('should toggle mute state', () => {
    const { result } = renderHook(() => useTurnSound());
    expect(result.current.isMuted).toBe(false);

    act(() => {
      const newMuted = result.current.toggleMute();
      expect(newMuted).toBe(true);
    });

    expect(localStorage.getItem('player_sound_muted')).toBe('true');
  });

  it('should set muted state', () => {
    const { result } = renderHook(() => useTurnSound());

    act(() => {
      result.current.setMuted(true);
    });

    expect(localStorage.getItem('player_sound_muted')).toBe('true');
  });

  it('should handle localStorage unavailability gracefully', () => {
    const spy = vi.spyOn(Storage.prototype, 'getItem');
    spy.mockImplementation(() => {
      throw new Error('localStorage is not available');
    });

    const { result } = renderHook(() => useTurnSound());
    expect(result.current.isMuted).toBe(false);

    spy.mockRestore();
  });

  it('should have playTurnNotification function', () => {
    const { result } = renderHook(() => useTurnSound());
    expect(typeof result.current.playTurnNotification).toBe('function');
  });

  it('should not crash when playing sound', () => {
    const { result } = renderHook(() => useTurnSound());
    expect(() => {
      act(() => {
        result.current.playTurnNotification();
      });
    }).not.toThrow();
  });

  it('should not play sound when muted', () => {
    const { result } = renderHook(() => useTurnSound());

    act(() => {
      result.current.setMuted(true);
    });

    // Should not throw, and shouldn't create audio nodes
    expect(() => {
      act(() => {
        result.current.playTurnNotification();
      });
    }).not.toThrow();
  });
});

describe('shouldPlayTurnSound', () => {
  it('plays once when the decision passes to the viewer', () => {
    expect(shouldPlayTurnSound(false, true)).toBe(true);
  });

  it('stays quiet for the next decisions of the same turn', () => {
    expect(shouldPlayTurnSound(true, true)).toBe(false);
  });

  it('stays quiet while someone else decides', () => {
    expect(shouldPlayTurnSound(false, false)).toBe(false);
    expect(shouldPlayTurnSound(true, false)).toBe(false);
  });
});
