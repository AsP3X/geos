import { type FormEvent, useState } from "react";
import { Link } from "react-router-dom";
import { Building2, Lock, Mail } from "lucide-react";
import { useAuth } from "@/hooks/useAuth";
import { ApiError } from "@/lib/api-client";
import { Button } from "@/components/ui/button";
import { AuthField } from "@/components/auth/AuthField";
import { AuthShell } from "@/components/auth/AuthShell";

export function LoginPage() {
  const { login } = useAuth();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [tenantSlug, setTenantSlug] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setPending(true);
    try {
      await login(email, password, tenantSlug.trim() || undefined);
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Login failed");
    } finally {
      setPending(false);
    }
  }

  return (
    <AuthShell
      badge="Welcome back"
      title="Sign in to Geos"
      subtitle="Access your command center and resume monitoring live events."
      footer={
        <>
          No account?{" "}
          <Link className="font-medium text-primary hover:underline" to="/register">
            Register a tenant
          </Link>
        </>
      }
    >
      <form className="flex flex-col gap-4" onSubmit={onSubmit}>
        <AuthField
          label="Email"
          icon={<Mail size={16} />}
          autoComplete="email"
          required
          type="email"
          placeholder="you@organization.com"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
        />
        <AuthField
          label="Password"
          icon={<Lock size={16} />}
          autoComplete="current-password"
          required
          type="password"
          placeholder="Enter your password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        <AuthField
          label="Tenant slug"
          optional
          icon={<Building2 size={16} />}
          autoComplete="organization"
          placeholder="acme-intel"
          value={tenantSlug}
          onChange={(e) => setTenantSlug(e.target.value)}
        />

        {error ? (
          <p
            role="alert"
            className="rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive"
          >
            {error}
          </p>
        ) : null}

        <Button disabled={pending} type="submit" className="mt-2 h-11 rounded-xl">
          {pending ? "Signing in…" : "Sign in"}
        </Button>
      </form>
    </AuthShell>
  );
}
