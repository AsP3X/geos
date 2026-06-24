import { type FormEvent, useState } from "react";
import { Link } from "react-router-dom";
import { Building2, Hash, Lock, Mail } from "lucide-react";
import { useAuth } from "@/hooks/useAuth";
import { ApiError } from "@/lib/api-client";
import { Button } from "@/components/ui/button";
import { AuthField } from "@/components/auth/AuthField";
import { AuthShell } from "@/components/auth/AuthShell";

function slugify(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48);
}

function isValidTenantSlug(value: string): boolean {
  return /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(value);
}

export function RegisterPage() {
  const { register } = useAuth();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [tenantName, setTenantName] = useState("");
  const [tenantSlug, setTenantSlug] = useState("");
  const [slugTouched, setSlugTouched] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    setError(null);

    const slug = tenantSlug.trim() || slugify(tenantName);
    if (!isValidTenantSlug(slug)) {
      setError("Tenant slug must use lowercase letters, numbers, and hyphens only.");
      return;
    }

    setPending(true);
    try {
      await register({
        email,
        password,
        tenantName,
        tenantSlug: slug,
      });
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Registration failed");
    } finally {
      setPending(false);
    }
  }

  return (
    <AuthShell
      badge="Get started"
      title="Create your tenant"
      subtitle="Register your organization and owner account to launch your command center."
      footer={
        <>
          Already registered?{" "}
          <Link className="font-medium text-primary hover:underline" to="/login">
            Sign in
          </Link>
        </>
      }
    >
      <form className="flex flex-col gap-4" onSubmit={onSubmit}>
        <AuthField
          label="Organization name"
          icon={<Building2 size={16} />}
          required
          placeholder="Acme Intelligence"
          value={tenantName}
          onChange={(e) => {
            const next = e.target.value;
            setTenantName(next);
            if (!slugTouched) {
              setTenantSlug(slugify(next));
            }
          }}
        />
        <AuthField
          label="Tenant slug"
          icon={<Hash size={16} />}
          required
          placeholder="acme-intel"
          hint="Lowercase letters, numbers, and hyphens. Used in your workspace URL."
          value={tenantSlug}
          onChange={(e) => {
            setSlugTouched(true);
            setTenantSlug(e.target.value);
          }}
        />
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
          autoComplete="new-password"
          minLength={12}
          required
          type="password"
          placeholder="At least 12 characters"
          hint="Use at least 12 characters."
          value={password}
          onChange={(e) => setPassword(e.target.value)}
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
          {pending ? "Creating…" : "Create account"}
        </Button>
      </form>
    </AuthShell>
  );
}
