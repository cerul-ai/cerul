"use client";

import { useEffect, useRef, useState } from "react";

type AnimationDirection = "up" | "down" | "left" | "right" | "none";

interface FadeInProps {
  children: React.ReactNode;
  delay?: number;
  duration?: number;
  direction?: AnimationDirection;
  distance?: number;
  className?: string;
  once?: boolean;
}

function useMountedReveal() {
  const [isVisible, setIsVisible] = useState(false);

  useEffect(() => {
    const frameId = window.requestAnimationFrame(() => {
      setIsVisible(true);
    });

    return () => {
      window.cancelAnimationFrame(frameId);
    };
  }, []);

  return isVisible;
}

export function FadeIn({
  children,
  delay = 0,
  duration = 600,
  direction = "up",
  distance = 24,
  className = "",
}: FadeInProps) {
  const isVisible = useMountedReveal();

  const getTransform = () => {
    if (direction === "none") return "translate3d(0, 0, 0)";
    const offset = isVisible ? 0 : distance;
    switch (direction) {
      case "up":
        return `translate3d(0, ${offset}px, 0)`;
      case "down":
        return `translate3d(0, -${offset}px, 0)`;
      case "left":
        return `translate3d(${offset}px, 0, 0)`;
      case "right":
        return `translate3d(-${offset}px, 0, 0)`;
      default:
        return "translate3d(0, 0, 0)";
    }
  };

  return (
    <div
      className={`motion-reveal-fade ${className}`.trim()}
      style={{
        opacity: isVisible ? 1 : 0,
        transform: getTransform(),
        transition: `opacity ${duration}ms cubic-bezier(0.16, 1, 0.3, 1) ${delay}ms, transform ${duration}ms cubic-bezier(0.16, 1, 0.3, 1) ${delay}ms`,
        willChange: "opacity, transform",
      }}
    >
      {children}
    </div>
  );
}

interface StaggerContainerProps {
  children: React.ReactNode;
  className?: string;
  staggerDelay?: number;
  baseDelay?: number;
}

export function StaggerContainer({
  children,
  className = "",
  staggerDelay = 100,
  baseDelay = 0,
}: StaggerContainerProps) {
  const isVisible = useMountedReveal();

  return (
    <div className={`motion-reveal-stagger ${className}`.trim()}>
      {Array.isArray(children)
        ? children.map((child, index) => (
            <div
              key={index}
              style={{
                opacity: isVisible ? 1 : 0,
                transform: isVisible ? "translateY(0)" : "translateY(20px)",
                transition: `opacity 500ms cubic-bezier(0.16, 1, 0.3, 1) ${
                  baseDelay + index * staggerDelay
                }ms, transform 500ms cubic-bezier(0.16, 1, 0.3, 1) ${
                  baseDelay + index * staggerDelay
                }ms`,
              }}
            >
              {child}
            </div>
          ))
        : children}
    </div>
  );
}

interface GlowCardProps {
  children: React.ReactNode;
  className?: string;
  glowColor?: string;
  hoverGlowColor?: string;
}

export function GlowCard({
  children,
  className = "",
  glowColor = "rgba(111, 151, 201, 0.15)",
  hoverGlowColor = "rgba(111, 151, 201, 0.25)",
}: GlowCardProps) {
  return (
    <div
      className={`group relative overflow-hidden rounded-3xl border border-[var(--border)] bg-[var(--surface)] backdrop-blur-xl transition-all duration-500 ${className}`}
      style={{
        boxShadow: `0 0 0 1px rgba(255,255,255,0.02) inset, 0 20px 60px -20px rgba(0,0,0,0.15), 0 0 40px -10px ${glowColor}`,
      }}
    >
      <div
        className="absolute inset-0 opacity-0 transition-opacity duration-500 group-hover:opacity-100"
        style={{
          background: `radial-gradient(600px circle at var(--mouse-x, 50%) var(--mouse-y, 50%), ${hoverGlowColor}, transparent 40%)`,
        }}
      />
      <div className="relative z-10">{children}</div>
    </div>
  );
}

interface TextGradientProps {
  children: React.ReactNode;
  className?: string;
  animate?: boolean;
}

export function TextGradient({
  children,
  className = "",
  animate = false,
}: TextGradientProps) {
  return (
    <span
      className={`bg-gradient-to-r from-[var(--foreground)] via-[var(--brand-bright)] to-[var(--foreground)] bg-clip-text text-transparent ${
        animate ? "animate-gradient bg-[length:200%_auto]" : ""
      } ${className}`}
    >
      {children}
    </span>
  );
}

interface MagneticButtonProps {
  children: React.ReactNode;
  className?: string;
  onClick?: () => void;
  href?: string;
}

export function MagneticButton({
  children,
  className = "",
  onClick,
  href,
}: MagneticButtonProps) {
  const ref = useRef<HTMLButtonElement | HTMLAnchorElement>(null);

  const handleMouseMove = (e: React.MouseEvent) => {
    if (!ref.current) return;
    const rect = ref.current.getBoundingClientRect();
    const x = e.clientX - rect.left - rect.width / 2;
    const y = e.clientY - rect.top - rect.height / 2;
    ref.current.style.transform = `translate(${x * 0.15}px, ${y * 0.15}px)`;
  };

  const handleMouseLeave = () => {
    if (!ref.current) return;
    ref.current.style.transform = "translate(0, 0)";
  };

  const baseClasses = `inline-flex items-center justify-center transition-transform duration-200 ease-out ${className}`;

  if (href) {
    return (
      <a
        ref={ref as React.RefObject<HTMLAnchorElement>}
        href={href}
        className={baseClasses}
        onMouseMove={handleMouseMove}
        onMouseLeave={handleMouseLeave}
      >
        {children}
      </a>
    );
  }

  return (
    <button
      ref={ref as React.RefObject<HTMLButtonElement>}
      type="button"
      className={baseClasses}
      onClick={onClick}
      onMouseMove={handleMouseMove}
      onMouseLeave={handleMouseLeave}
    >
      {children}
    </button>
  );
}

interface AnimatedCounterProps {
  value: number;
  suffix?: string;
  prefix?: string;
  duration?: number;
  className?: string;
}

export function AnimatedCounter({
  value,
  suffix = "",
  prefix = "",
  duration = 2000,
  className = "",
}: AnimatedCounterProps) {
  const [count, setCount] = useState(0);

  useEffect(() => {
    let frameId = 0;
    let startTime = 0;

    const animate = (timestamp: number) => {
      if (!startTime) {
        startTime = timestamp;
      }

      const elapsed = timestamp - startTime;
      const progress = Math.min(elapsed / duration, 1);
      const easeOutQuart = 1 - Math.pow(1 - progress, 4);
      setCount(Math.floor(easeOutQuart * value));

      if (progress < 1) {
        frameId = requestAnimationFrame(animate);
      } else {
        setCount(value);
      }
    };

    frameId = requestAnimationFrame(animate);

    return () => {
      cancelAnimationFrame(frameId);
    };
  }, [value, duration]);

  return (
    <span className={className}>
      {prefix}
      {count.toLocaleString()}
      {suffix}
    </span>
  );
}

interface BlurFadeProps {
  children: React.ReactNode;
  delay?: number;
  className?: string;
}

export function BlurFade({
  children,
  delay = 0,
  className = "",
}: BlurFadeProps) {
  const isVisible = useMountedReveal();

  return (
    <div
      className={`motion-reveal-blur ${className}`.trim()}
      style={{
        opacity: isVisible ? 1 : 0,
        filter: isVisible ? "blur(0px)" : "blur(10px)",
        transform: isVisible ? "translateY(0)" : "translateY(20px)",
        transition: `opacity 700ms cubic-bezier(0.16, 1, 0.3, 1) ${delay}ms, filter 700ms cubic-bezier(0.16, 1, 0.3, 1) ${delay}ms, transform 700ms cubic-bezier(0.16, 1, 0.3, 1) ${delay}ms`,
        willChange: "opacity, filter, transform",
      }}
    >
      {children}
    </div>
  );
}
