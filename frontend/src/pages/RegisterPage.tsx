import { type FormEvent, useState } from "react";
import { Link } from "react-router-dom";
import { useAuth } from "@/hooks/useAuth";
import { ApiError } from "@/lib/api-client";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";

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
    <div className="flex min-h-screen items-center justify-center p-6">
      <Card className="w-full max-w-md">
        <CardHeader>
          <CardTitle className="text-primary">Create tenant</CardTitle>
          <CardDescription>Register your organization and owner account</CardDescription>
        </CardHeader>
        <CardContent>
          <form className="flex flex-col gap-4" onSubmit={onSubmit}>
            <label className="flex flex-col gap-1.5 text-sm">
              Organization name
              <Input
                required
                value={tenantName}
                onChange={(e) => {
                  const next = e.target.value;
                  setTenantName(next);
                  if (!slugTouched) {
                    setTenantSlug(slugify(next));
                  }
                }}
              />
            </label>
            <label className="flex flex-col gap-1.5 text-sm">
              Tenant slug
              <Input
                required
                value={tenantSlug}
                onChange={(e) => {
                  setSlugTouched(true);
                  setTenantSlug(e.target.value);
                }}
              />
            </label>
            <label className="flex flex-col gap-1.5 text-sm">
              Email
              <Input
                autoComplete="email"
                required
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
              />
            </label>
            <label className="flex flex-col gap-1.5 text-sm">
              Password
              <Input
                autoComplete="new-password"
                minLength={12}
                required
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
              />
            </label>
            {error ? <p className="text-sm text-destructive">{error}</p> : null}
            <Button disabled={pending} type="submit">
              {pending ? "Creating…" : "Create account"}
            </Button>
          </form>
          <p className="mt-4 text-center text-sm text-muted-foreground">
            Already registered?{" "}
            <Link className="text-primary hover:underline" to="/login">
              Sign in
            </Link>
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
