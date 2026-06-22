import { cn } from "@/lib/utils";

export function App() {
  return (
    <main className="flex min-h-screen flex-col items-center justify-center gap-4 p-8">
      <h1 className="text-4xl font-semibold tracking-tight text-primary">
        Geos
      </h1>
      <p className="text-muted-foreground">
        OSINT event intelligence globe — scaffold ready.
      </p>
      <span
        className={cn(
          "rounded-md border border-border bg-card px-3 py-1 text-sm",
          "text-card-foreground",
        )}
      >
        command-center online
      </span>
    </main>
  );
}
