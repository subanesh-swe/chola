import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { User } from '../types';

interface AuthState {
  token: string | null;
  tokenExpiresAt: string | null;
  user: User | null;
  isAuthenticated: boolean;
  login: (token: string, expiresAt: string, user: User) => void;
  logout: () => void;
  updateUser: (user: User) => void;
  isTokenExpired: () => boolean;
}

export const useAuthStore = create<AuthState>()(
  persist(
    (set, get) => ({
      token: null,
      tokenExpiresAt: null,
      user: null,
      isAuthenticated: false,
      login: (token, expiresAt, user) => set({ token, tokenExpiresAt: expiresAt, user, isAuthenticated: true }),
      logout: () => {
        set({ token: null, tokenExpiresAt: null, user: null, isAuthenticated: false });
        window.sessionStorage.clear();
      },
      updateUser: (user) => set({ user }),
      isTokenExpired: () => {
        const expiresAt = get().tokenExpiresAt;
        if (!expiresAt) return true;
        return new Date(expiresAt) <= new Date();
      },
    }),
    {
      name: 'chola-auth',
      partialize: (state) => ({
        token: state.token,
        tokenExpiresAt: state.tokenExpiresAt,
        user: state.user,
        isAuthenticated: state.isAuthenticated,
      }),
    },
  ),
);
