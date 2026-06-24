import type { ReactNode } from "react";
import { Activity, Globe2, Radio, ShieldCheck } from "lucide-react";
import { AuthBackdrop } from "./AuthBackdrop";

type HighlightItem = {
  icon: ReactNode;
  title: string;
  body: string;
};

const HIGHLIGHTS: HighlightItem[] = [
  {
    icon: <Globe2 size={16} />,
    title: "Geospatial intelligence",
    body: "Every signal placed on an interactive 3D globe in real time.",
  },
  {
    icon: <Radio size={16} />,
    title: "Live ingestion",
    body: "Public-source events normalized, correlated, and streamed as they happen.",
  },
  {
    icon: <ShieldCheck size={16} />,
    title: "Tenant-isolated",
    body: "Multi-tenant by design with role-based access and full audit trails.",
  },
];

/** Small brand wordmark used across the auth surfaces. */
export function AuthWordmark({ className = "" }: { className?: string }) {
  return (
    <div className={`flex items-center gap-2.5 ${className}`}>
      <span className="relative inline-flex size-8 items-center justify-center rounded-xl bg-primary/15 ring-1 ring-primary/30">
        <Globe2 className="text-primary" size={18} />
        <span className="absolute inset-0 rounded-xl bg-primary/20 blur-md" aria-hidden />
      </span>
      <span className="text-lg font-semibold tracking-tight text-foreground">Geos</span>
    </div>
  );
}

export function AuthShell({
  badge,
  title,
  subtitle,
  children,
  footer,
}: {
  badge: string;
  title: string;
  subtitle: string;
  children: ReactNode;
  footer?: ReactNode;
}) {
  return (
    <div className="relative flex min-h-screen w-full items-center justify-center p-4 sm:p-6">
      <AuthBackdrop />

      <div className="glass-panel relative grid w-full max-w-5xl overflow-hidden rounded-[1.75rem] lg:grid-cols-[1.05fr_1fr]">
        {/* Brand / value panel — hidden on small screens to keep focus on the form. */}
        <aside className="relative hidden flex-col justify-between gap-10 p-10 lg:flex lg:border-r lg:border-white/10">
          <AuthWordmark />

          <div className="geos-rise relative flex flex-col gap-4">
            <span className="inline-flex w-fit items-center gap-2 rounded-full bg-white/5 px-3 py-1 text-[11px] font-semibold uppercase tracking-[0.18em] text-primary/90">
              <Activity size={12} /> OSINT Command Center
            </span>
            <h2 className="text-balance text-3xl font-semibold leading-tight tracking-tight text-foreground">
              Turn the world&apos;s public signals into a single live picture.
            </h2>
            <p className="max-w-sm text-sm leading-relaxed text-foreground/60">
              Geos ingests, normalizes, and correlates open-source events, then
              renders them on a real-time globe so your team can see what matters,
              the instant it happens.
            </p>
          </div>

          <ul className="relative flex flex-col gap-4">
            {HIGHLIGHTS.map((item) => (
              <li key={item.title} className="flex items-start gap-3">
                <span className="mt-0.5 inline-flex size-8 shrink-0 items-center justify-center rounded-lg bg-white/5 text-primary ring-1 ring-white/10">
                  {item.icon}
                </span>
                <div className="flex flex-col">
                  <span className="text-sm font-medium text-foreground/90">
                    {item.title}
                  </span>
                  <span className="text-xs leading-relaxed text-foreground/50">
                    {item.body}
                  </span>
                </div>
              </li>
            ))}
          </ul>
        </aside>

        {/* Form panel. */}
        <section className="relative flex flex-col justify-center p-7 sm:p-10">
          <div className="geos-rise mx-auto flex w-full max-w-sm flex-col">
            <AuthWordmark className="mb-8 lg:hidden" />

            <header className="mb-7 flex flex-col gap-1.5">
              <span className="text-[11px] font-semibold uppercase tracking-[0.18em] text-primary/80">
                {badge}
              </span>
              <h1 className="text-2xl font-semibold tracking-tight text-foreground">
                {title}
              </h1>
              <p className="text-sm text-foreground/55">{subtitle}</p>
            </header>

            {children}

            {footer ? (
              <p className="mt-7 text-center text-sm text-foreground/55">{footer}</p>
            ) : null}
          </div>
        </section>
      </div>
    </div>
  );
}
