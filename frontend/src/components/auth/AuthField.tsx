import { type InputHTMLAttributes, type ReactNode, useId, useState } from "react";
import { Eye, EyeOff } from "lucide-react";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

type AuthFieldProps = Omit<InputHTMLAttributes<HTMLInputElement>, "id"> & {
  label: string;
  icon?: ReactNode;
  hint?: ReactNode;
  optional?: boolean;
};

/**
 * Labeled input tuned for the glass auth panels: a leading icon, accessible
 * label association, and a built-in show/hide toggle for password fields.
 */
export function AuthField({
  label,
  icon,
  hint,
  optional,
  type = "text",
  className,
  ...props
}: AuthFieldProps) {
  const id = useId();
  const [revealed, setRevealed] = useState(false);
  const isPassword = type === "password";
  const resolvedType = isPassword && revealed ? "text" : type;

  return (
    <div className="flex flex-col gap-1.5">
      <label
        htmlFor={id}
        className="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-[0.14em] text-foreground/55"
      >
        {label}
        {optional ? (
          <span className="font-normal normal-case tracking-normal text-foreground/35">
            optional
          </span>
        ) : null}
      </label>
      <div className="relative">
        {icon ? (
          <span className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-foreground/40">
            {icon}
          </span>
        ) : null}
        <Input
          id={id}
          type={resolvedType}
          className={cn(
            "h-11 rounded-xl border-white/10 bg-white/[0.04] text-[0.95rem] transition-colors",
            "placeholder:text-foreground/30 hover:border-white/20 focus-visible:border-primary/50 focus-visible:ring-primary/30",
            icon && "pl-10",
            isPassword && "pr-10",
            className,
          )}
          {...props}
        />
        {isPassword ? (
          <button
            type="button"
            onClick={() => setRevealed((v) => !v)}
            aria-label={revealed ? "Hide password" : "Show password"}
            className="absolute right-1.5 top-1/2 flex size-8 -translate-y-1/2 items-center justify-center rounded-lg text-foreground/40 transition-colors hover:bg-white/10 hover:text-foreground/70"
          >
            {revealed ? <EyeOff size={16} /> : <Eye size={16} />}
          </button>
        ) : null}
      </div>
      {hint ? <p className="text-xs text-foreground/40">{hint}</p> : null}
    </div>
  );
}
