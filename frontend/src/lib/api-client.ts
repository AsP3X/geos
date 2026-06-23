import { apiBaseUrl } from "@/lib/env";
import { ApiError, parseApiError } from "@/lib/api-error";
import type { AuthResponse } from "@/lib/auth-storage";

type JsonBody = Record<string, unknown>;

async function request<T>(
  path: string,
  init: RequestInit,
  accessToken?: string,
): Promise<T> {
  const headers = new Headers(init.headers);
  if (!headers.has("Content-Type") && init.body) {
    headers.set("Content-Type", "application/json");
  }
  if (accessToken) {
    headers.set("Authorization", `Bearer ${accessToken}`);
  }

  const response = await fetch(`${apiBaseUrl()}${path}`, {
    ...init,
    headers,
  });

  if (!response.ok) {
    throw await parseApiError(response);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return (await response.json()) as T;
}

export async function register(input: {
  email: string;
  password: string;
  tenantName: string;
  tenantSlug: string;
  displayName?: string;
}): Promise<AuthResponse> {
  const body: JsonBody = {
    email: input.email,
    password: input.password,
    tenant_name: input.tenantName,
    tenant_slug: input.tenantSlug,
  };
  if (input.displayName) {
    body.display_name = input.displayName;
  }
  return request<AuthResponse>("/api/v1/auth/register", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function login(input: {
  email: string;
  password: string;
  tenantSlug?: string;
}): Promise<AuthResponse> {
  const body: JsonBody = {
    email: input.email,
    password: input.password,
  };
  if (input.tenantSlug) {
    body.tenant_slug = input.tenantSlug;
  }
  return request<AuthResponse>("/api/v1/auth/login", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function refresh(refreshToken: string, tenantSlug?: string): Promise<AuthResponse> {
  const body: JsonBody = { refresh_token: refreshToken };
  if (tenantSlug) {
    body.tenant_slug = tenantSlug;
  }
  return request<AuthResponse>("/api/v1/auth/refresh", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function apiGet<T>(path: string, accessToken: string): Promise<T> {
  return request<T>(path, { method: "GET" }, accessToken);
}

export async function apiPost<T>(
  path: string,
  body: unknown,
  accessToken: string,
): Promise<T> {
  return request<T>(path, { method: "POST", body: JSON.stringify(body) }, accessToken);
}

export async function apiPut<T>(
  path: string,
  body: unknown,
  accessToken: string,
): Promise<T> {
  return request<T>(path, { method: "PUT", body: JSON.stringify(body) }, accessToken);
}

export async function apiDelete<T>(path: string, accessToken: string): Promise<T> {
  return request<T>(path, { method: "DELETE" }, accessToken);
}

export { ApiError };
