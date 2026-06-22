import {
  createContext,
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import * as api from "@/lib/api-client";
import {
  clearSession,
  loadSession,
  saveSession,
  sessionFromResponse,
  sessionNeedsRefresh,
  type AuthSession,
} from "@/lib/auth-storage";

export interface AuthContextValue {
  session: AuthSession | null;
  isLoading: boolean;
  login: (email: string, password: string, tenantSlug?: string) => Promise<void>;
  register: (input: {
    email: string;
    password: string;
    tenantName: string;
    tenantSlug: string;
    displayName?: string;
  }) => Promise<void>;
  logout: () => void;
  getAccessToken: () => Promise<string | null>;
}

export const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [session, setSession] = useState<AuthSession | null>(() => loadSession());
  const [isLoading, setIsLoading] = useState(true);

  const applyAuthResponse = useCallback((response: Awaited<ReturnType<typeof api.login>>) => {
    const next = sessionFromResponse(response);
    saveSession(next);
    setSession(next);
  }, []);

  const refreshIfNeeded = useCallback(async (current: AuthSession): Promise<AuthSession | null> => {
    if (!sessionNeedsRefresh(current)) {
      return current;
    }
    try {
      const response = await api.refresh(current.refreshToken, current.tenantSlug);
      const next = sessionFromResponse(response);
      saveSession(next);
      setSession(next);
      return next;
    } catch {
      clearSession();
      setSession(null);
      return null;
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const stored = loadSession();
      if (!stored) {
        if (!cancelled) {
          setIsLoading(false);
        }
        return;
      }
      const refreshed = await refreshIfNeeded(stored);
      if (!cancelled) {
        setSession(refreshed);
        setIsLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [refreshIfNeeded]);

  const login = useCallback(
    async (email: string, password: string, tenantSlug?: string) => {
      const response = await api.login({ email, password, tenantSlug });
      applyAuthResponse(response);
    },
    [applyAuthResponse],
  );

  const register = useCallback(
    async (input: {
      email: string;
      password: string;
      tenantName: string;
      tenantSlug: string;
      displayName?: string;
    }) => {
      const response = await api.register(input);
      applyAuthResponse(response);
    },
    [applyAuthResponse],
  );

  const logout = useCallback(() => {
    clearSession();
    setSession(null);
  }, []);

  const getAccessToken = useCallback(async () => {
    if (!session) {
      return null;
    }
    const current = await refreshIfNeeded(session);
    return current?.accessToken ?? null;
  }, [refreshIfNeeded, session]);

  const value = useMemo(
    () => ({
      session,
      isLoading,
      login,
      register,
      logout,
      getAccessToken,
    }),
    [session, isLoading, login, register, logout, getAccessToken],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}
