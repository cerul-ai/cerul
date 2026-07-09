"use client";

import type { Route } from "next";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { startTransition, useEffect, useState } from "react";
import { AuthSocialSection } from "@/components/auth/auth-social-section";
import { authClient } from "@/lib/auth";
import type { AuthSocialProviderId } from "@/lib/auth-providers";
import {
  getAuthErrorMessage,
  isEmailNotVerifiedError,
} from "@/lib/auth-shared";
import {
  normalizeReferralCode,
  PENDING_REFERRAL_CODE_STORAGE_KEY,
} from "@/lib/referral";

type LoginFormProps = {
  nextPath: string;
  initialMode?: "login" | "signup";
  enabledProviders: AuthSocialProviderId[];
  googleOneTapClientId: string | null;
  initialError?: string | null;
  referralCode?: string | null;
};

function buildVerifyEmailHref(email: string): string {
  return `/verify-email?email=${encodeURIComponent(email)}`;
}

export function LoginForm({
  nextPath,
  initialMode = "login",
  enabledProviders,
  googleOneTapClientId,
  initialError = null,
  referralCode = null,
}: LoginFormProps) {
  const router = useRouter();
  const [mode, setMode] = useState<"login" | "signup">(initialMode);
  const [fullName, setFullName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [showConfirmPassword, setShowConfirmPassword] = useState(false);
  const [error, setError] = useState<string | null>(initialError);
  const [isSubmitting, setIsSubmitting] = useState(false);

  useEffect(() => {
    const normalizedReferralCode = normalizeReferralCode(referralCode);
    if (!normalizedReferralCode) {
      return;
    }

    try {
      window.localStorage.setItem(
        PENDING_REFERRAL_CODE_STORAGE_KEY,
        normalizedReferralCode,
      );
    } catch {
      // Ignore localStorage access issues and continue with auth.
    }
  }, [referralCode]);

  async function handleLoginSubmit(normalizedEmail: string) {
    const result = await authClient.signIn.email({
      email: normalizedEmail,
      password,
      rememberMe: true,
    });

    if (result.error) {
      if (isEmailNotVerifiedError(result.error)) {
        startTransition(() => {
          router.replace(buildVerifyEmailHref(normalizedEmail) as Route);
          router.refresh();
        });
        return;
      }

      setError(
        getAuthErrorMessage(result.error, "Invalid email or password."),
      );
      return;
    }

    startTransition(() => {
      router.replace(nextPath as Route);
      router.refresh();
    });
  }

  async function handleSignupSubmit(normalizedEmail: string) {
    const normalizedName = fullName.trim().replace(/\s+/g, " ");

    if (!normalizedName) {
      setError("Your name is required.");
      return;
    }

    if (password.length < 8) {
      setError("Password must be at least 8 characters.");
      return;
    }

    if (password !== confirmPassword) {
      setError("Passwords do not match.");
      return;
    }

    const result = await authClient.signUp.email({
      name: normalizedName,
      email: normalizedEmail,
      password,
      callbackURL: buildVerifyEmailHref(normalizedEmail),
    });

    if (result.error) {
      setError(
        getAuthErrorMessage(result.error, "Unable to create that account."),
      );
      return;
    }

    startTransition(() => {
      router.replace(buildVerifyEmailHref(normalizedEmail) as Route);
      router.refresh();
    });
  }

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    setIsSubmitting(true);

    const normalizedEmail = email.trim().toLowerCase();

    try {
      if (mode === "signup") {
        await handleSignupSubmit(normalizedEmail);
      } else {
        await handleLoginSubmit(normalizedEmail);
      }
    } catch (nextError) {
      setError(
        getAuthErrorMessage(
          nextError,
          mode === "signup"
            ? "Unable to create an account right now."
            : "Unable to sign in right now.",
        ),
      );
    } finally {
      setIsSubmitting(false);
    }
  }

  const isLogin = mode === "login";

  return (
    <form className="space-y-6" onSubmit={(event) => void handleSubmit(event)}>
      <div className="flex rounded-[20px] border border-[var(--border)] bg-white/60 p-1 shadow-[0_1px_3px_rgba(70,52,29,0.06)]">
        <button
          type="button"
          className={`flex-1 rounded-[16px] px-4 py-2.5 text-sm font-semibold transition ${
            isLogin
              ? "bg-[var(--foreground)] text-white shadow-md"
              : "text-[var(--foreground-secondary)] hover:text-[var(--foreground)]"
          }`}
          onClick={() => { setMode("login"); setError(null); }}
        >
          Log in
        </button>
        <button
          type="button"
          className={`flex-1 rounded-[16px] px-4 py-2.5 text-sm font-semibold transition ${
            !isLogin
              ? "bg-[var(--foreground)] text-white shadow-md"
              : "text-[var(--foreground-secondary)] hover:text-[var(--foreground)]"
          }`}
          onClick={() => { setMode("signup"); setError(null); }}
        >
          Sign up
        </button>
      </div>

      <div className="space-y-4">
        <AuthSocialSection
          mode={mode}
          nextPath={nextPath}
          enabledProviders={enabledProviders}
          googleOneTapClientId={googleOneTapClientId}
          onErrorChange={setError}
        />

        {!isLogin && (
          <div className="space-y-2">
            <label className="text-sm font-medium text-[var(--foreground-secondary)]" htmlFor="auth-name">
              Full name
            </label>
            <div className="auth-input-shell" data-leading-icon="true">
              <span className="auth-input-leading-icon" aria-hidden="true">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
                  <path d="M20 21a8 8 0 0 0-16 0" />
                  <circle cx="12" cy="8" r="4" />
                </svg>
              </span>
              <input
                id="auth-name"
                className="auth-input"
                type="text"
                placeholder="Your full name"
                autoComplete="name"
                value={fullName}
                onChange={(event) => setFullName(event.target.value)}
                required
              />
            </div>
          </div>
        )}

        <div className="space-y-2">
          <label className="text-sm font-medium text-[var(--foreground-secondary)]" htmlFor="auth-email">
            Email
          </label>
          <div className="auth-input-shell" data-leading-icon="true">
            <span className="auth-input-leading-icon" aria-hidden="true">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
                <rect x="3" y="5" width="18" height="14" rx="2" />
                <path d="m4 7 8 6 8-6" />
              </svg>
            </span>
            <input
              id="auth-email"
              className="auth-input"
              type="email"
              placeholder="you@company.com"
              autoComplete="email"
              inputMode="email"
              autoCapitalize="none"
              spellCheck={false}
              value={email}
              onChange={(event) => setEmail(event.target.value)}
              required
            />
          </div>
        </div>

        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <label className="text-sm font-medium text-[var(--foreground-secondary)]" htmlFor="auth-password">
              Password
            </label>
            {isLogin && (
              <Link
                href={"/forgot-password" as Route}
                className="text-xs text-[var(--foreground-tertiary)] transition hover:text-[var(--foreground-secondary)]"
              >
                Forgot your password?
              </Link>
            )}
          </div>
          <div className="auth-input-shell" data-leading-icon="true">
            <span className="auth-input-leading-icon" aria-hidden="true">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
                <rect x="4" y="11" width="16" height="9" rx="2" />
                <path d="M8 11V8a4 4 0 1 1 8 0v3" />
              </svg>
            </span>
            <input
              id="auth-password"
              className="auth-input !pr-12"
              type={showPassword ? "text" : "password"}
              placeholder={isLogin ? "Enter your password" : "At least 8 characters"}
              autoComplete={isLogin ? "current-password" : "new-password"}
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              required
              minLength={isLogin ? undefined : 8}
            />
            <button
              type="button"
              aria-label={showPassword ? "Hide password" : "Show password"}
              className="absolute right-3.5 top-1/2 -translate-y-1/2 rounded-lg p-1.5 text-[var(--foreground-tertiary)] transition hover:bg-[rgba(255,255,255,0.05)] hover:text-[var(--foreground-secondary)]"
              onClick={() => setShowPassword((current) => !current)}
            >
              {showPassword ? (
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M3 3 21 21" />
                  <path d="M10.58 10.58A2 2 0 0 0 12 14a2 2 0 0 0 1.42-.58" />
                  <path d="M16.68 16.67A10.94 10.94 0 0 1 12 18C7 18 3.73 14.89 2 12c.92-1.55 2.14-3.01 3.65-4.16" />
                  <path d="M9.88 5.09A11 11 0 0 1 12 5c5 0 8.27 3.11 10 6-1.01 1.7-2.41 3.3-4.17 4.5" />
                </svg>
              ) : (
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z" />
                  <circle cx="12" cy="12" r="3" />
                </svg>
              )}
            </button>
          </div>
        </div>

        {!isLogin && (
          <div className="space-y-2">
            <label className="text-sm font-medium text-[var(--foreground-secondary)]" htmlFor="auth-confirm-password">
              Confirm password
            </label>
            <div className="auth-input-shell" data-leading-icon="true">
              <span className="auth-input-leading-icon" aria-hidden="true">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
                  <rect x="4" y="11" width="16" height="9" rx="2" />
                  <path d="M8 11V8a4 4 0 1 1 8 0v3" />
                </svg>
              </span>
              <input
                id="auth-confirm-password"
                className="auth-input !pr-12"
                type={showConfirmPassword ? "text" : "password"}
                placeholder="Repeat password"
                autoComplete="new-password"
                value={confirmPassword}
                onChange={(event) => setConfirmPassword(event.target.value)}
                required
                minLength={8}
              />
              <button
                type="button"
                aria-label={showConfirmPassword ? "Hide password" : "Show password"}
                className="absolute right-3.5 top-1/2 -translate-y-1/2 rounded-lg p-1.5 text-[var(--foreground-tertiary)] transition hover:bg-[rgba(255,255,255,0.05)] hover:text-[var(--foreground-secondary)]"
                onClick={() => setShowConfirmPassword((current) => !current)}
              >
                {showConfirmPassword ? (
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M3 3 21 21" />
                    <path d="M10.58 10.58A2 2 0 0 0 12 14a2 2 0 0 0 1.42-.58" />
                    <path d="M16.68 16.67A10.94 10.94 0 0 1 12 18C7 18 3.73 14.89 2 12c.92-1.55 2.14-3.01 3.65-4.16" />
                    <path d="M9.88 5.09A11 11 0 0 1 12 5c5 0 8.27 3.11 10 6-1.01 1.7-2.41 3.3-4.17 4.5" />
                  </svg>
                ) : (
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z" />
                    <circle cx="12" cy="12" r="3" />
                  </svg>
                )}
              </button>
            </div>
          </div>
        )}

        {error && (
          <div className="rounded-[18px] border border-[rgba(191,91,70,0.18)] bg-[rgba(191,91,70,0.12)] px-4 py-3 text-sm text-[var(--error)]">
            {error}
          </div>
        )}

        <button
          type="submit"
          className="button-primary w-full"
          disabled={isSubmitting}
        >
          {isSubmitting
            ? isLogin ? "Signing in..." : "Creating account..."
            : isLogin ? "Log in" : "Create account"}
        </button>

      </div>

      <p className="text-center text-xs leading-6 text-[var(--foreground-tertiary)]">
        By continuing, you agree to the Cerul{" "}
        <a href="/terms" className="underline hover:text-[var(--foreground-secondary)]">Terms of Service</a>
        {" "}and{" "}
        <a href="/privacy" className="underline hover:text-[var(--foreground-secondary)]">Privacy Policy</a>.
      </p>
    </form>
  );
}
