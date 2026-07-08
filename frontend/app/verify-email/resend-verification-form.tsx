"use client";

import type { Route } from "next";
import Link from "next/link";
import { useCallback, useEffect, useState } from "react";
import { authClient } from "@/lib/auth";
import { getAuthErrorMessage } from "@/lib/auth-shared";

const COOLDOWN_SECONDS = 60;

type ResendVerificationFormProps = {
  email: string | null;
  initialError?: string | null;
};

function buildVerifyEmailCallback(email: string): string {
  const query = new URLSearchParams({ email });
  return `/verify-email?${query.toString()}`;
}

export function ResendVerificationForm({
  email,
  initialError = null,
}: ResendVerificationFormProps) {
  const [error, setError] = useState<string | null>(initialError);
  const [success, setSuccess] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [cooldown, setCooldown] = useState(COOLDOWN_SECONDS);

  useEffect(() => {
    if (cooldown <= 0) return;
    const timer = setInterval(() => {
      setCooldown((c) => (c <= 1 ? 0 : c - 1));
    }, 1000);
    return () => clearInterval(timer);
  }, [cooldown]);

  const handleResend = useCallback(async () => {
    if (!email) {
      setError("We could not determine which email address to verify. Please sign in again.");
      return;
    }

    setError(null);
    setSuccess(null);
    setIsSubmitting(true);

    try {
      const result = await authClient.sendVerificationEmail({
        email,
        callbackURL: buildVerifyEmailCallback(email),
      });

      if (result.error) {
        setError(
          getAuthErrorMessage(
            result.error,
            "Unable to resend the verification email right now.",
          ),
        );
        return;
      }

      setSuccess(`We sent a fresh verification link to ${email}.`);
      setCooldown(COOLDOWN_SECONDS);
    } catch (requestError) {
      setError(
        getAuthErrorMessage(
          requestError,
          "Unable to resend the verification email right now.",
        ),
      );
    } finally {
      setIsSubmitting(false);
    }
  }, [email]);

  const isDisabled = isSubmitting || !email || cooldown > 0;

  return (
    <div className="space-y-6">
      <div className="space-y-2">
        <p className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--brand-bright)]">
          Check your inbox
        </p>
        <h2 className="text-2xl font-semibold tracking-[-0.04em] text-[var(--foreground)]">
          Verify your email address
        </h2>
        <p className="text-sm leading-7 text-[var(--foreground-secondary)]">
          {email
            ? `We sent a verification link to ${email}.`
            : "Open the verification link from your email to finish activating your Cerul account."}
        </p>
      </div>

      {success && (
        <div className="rounded-[18px] border border-[rgba(92,132,191,0.18)] bg-[rgba(111, 151, 201,0.12)] px-4 py-3 text-sm text-[var(--foreground)]">
          {success}
        </div>
      )}

      {error && (
        <div className="rounded-[18px] border border-[rgba(191,91,70,0.18)] bg-[rgba(191,91,70,0.12)] px-4 py-3 text-sm text-[var(--error)]">
          {error}
        </div>
      )}

      <button
        type="button"
        className="button-primary w-full"
        disabled={isDisabled}
        onClick={() => void handleResend()}
      >
        {isSubmitting
          ? "Resending..."
          : cooldown > 0
            ? `Resend in ${cooldown}s`
            : "Resend verification email"}
      </button>

      <p className="text-center text-sm text-[var(--foreground-tertiary)]">
        <Link
          href={"/login" as Route}
          className="font-medium text-[var(--foreground)] transition hover:text-[var(--brand-bright)]"
        >
          Back to log in
        </Link>
      </p>
    </div>
  );
}
