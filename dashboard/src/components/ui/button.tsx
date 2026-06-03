import { cn } from "@/lib/utils";
import { type ButtonHTMLAttributes, forwardRef } from "react";

/** Minimal shadcn/ui-style button. Visual design is out of scope (SPEC-009 §Out of
 *  scope); this is the contract-rendering primitive the login form uses. */
export const Button = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement>>(
  function Button({ className, type = "button", ...props }, ref) {
    return (
      <button
        ref={ref}
        type={type}
        className={cn(
          "inline-flex items-center justify-center rounded-md bg-slate-900 px-4 py-2 text-sm font-medium text-white disabled:opacity-50",
          className,
        )}
        {...props}
      />
    );
  },
);
