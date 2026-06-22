/** Successful auth payload from `/api/v1/auth/*`. */
export interface AuthResponse {
  access_token: string;
  refresh_token: string;
  expires_in: number;
  token_type: string;
  user_id: string;
  tenant_id: string;
  tenant_slug: string;
  role: string;
}

export interface AuthSession {
  accessToken: string;
  refreshToken: string;
  expiresAt: number;
  userId: string;
  tenantId: string;
  tenantSlug: string;
  role: string;
}

const STORAGE_KEY = "geos.auth.session";

export function sessionFromResponse(response: AuthResponse): AuthSession {
  return {
    accessToken: response.access_token,
    refreshToken: response.refresh_token,
    expiresAt: Date.now() + response.expires_in * 1000,
    userId: response.user_id,
    tenantId: response.tenant_id,
    tenantSlug: response.tenant_slug,
    role: response.role,
  };
}

export function loadSession(): AuthSession | null {
  const raw = localStorage.getItem(STORAGE_KEY);
  if (!raw) {
    return null;
  }
  try {
    const parsed = JSON.parse(raw) as AuthSession;
    if (
      typeof parsed.accessToken === "string" &&
      typeof parsed.refreshToken === "string" &&
      typeof parsed.expiresAt === "number"
    ) {
      return parsed;
    }
  } catch {
    localStorage.removeItem(STORAGE_KEY);
  }
  return null;
}

export function saveSession(session: AuthSession): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(session));
}

export function clearSession(): void {
  localStorage.removeItem(STORAGE_KEY);
}

export function sessionNeedsRefresh(session: AuthSession, skewMs = 60_000): boolean {
  return Date.now() >= session.expiresAt - skewMs;
}
