import { getSiteOrigin } from "./site-url";

type EmailLink = {
  href: string;
  label: string;
};

type EmailTemplateInput = {
  preview: string;
  title: string;
  intro: string;
  body: string[];
  cta?: EmailLink;
  fallbackLink?: string;
  footerNote: string;
  secondaryLinks?: EmailLink[];
};

const palette = {
  background: "#faf8f5",
  cardBackground: "#ffffff",
  textPrimary: "#2c2418",
  textSecondary: "#6b5d4f",
  border: "#e8e0d4",
  accent: "#88a5f2",
  accentWarm: "#c5a55a",
  buttonText: "#ffffff",
};

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function normalizeName(name: string): string {
  const trimmed = name.trim();
  return trimmed || "there";
}

function renderLinks(links: EmailLink[] | undefined): string {
  if (!links || links.length === 0) {
    return "";
  }

  return `
    <p style="margin:20px 0 0;color:${palette.textSecondary};font-size:13px;line-height:1.7;text-align:center;">
      ${links
        .map(
          (link) =>
            `<a href="${escapeHtml(link.href)}" style="color:${palette.accent};text-decoration:none;">${escapeHtml(link.label)}</a>`,
        )
        .join(' <span style="color:#b4a794;">&middot;</span> ')}
    </p>
  `;
}

function renderEmailTemplate(input: EmailTemplateInput): string {
  return `
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <meta name="color-scheme" content="light dark" />
    <meta name="supported-color-schemes" content="light dark" />
    <title>${escapeHtml(input.title)}</title>
  </head>
  <body style="margin:0;padding:0;background:${palette.background};color:${palette.textPrimary};font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;">
    <div style="display:none;max-height:0;overflow:hidden;opacity:0;">
      ${escapeHtml(input.preview)}
    </div>
    <div style="width:100%;padding:32px 16px;background:${palette.background};">
      <div style="margin:0 auto;max-width:560px;">
        <div style="overflow:hidden;border:1px solid ${palette.border};border-radius:16px;background:${palette.cardBackground};box-shadow:0 18px 45px rgba(70,52,29,0.08);">
          <div style="height:4px;background:${palette.accent};background-image:linear-gradient(135deg, ${palette.accent}, ${palette.accentWarm});"></div>
          <div style="padding:32px 32px 28px;">
            <div style="margin:0 0 24px;text-align:center;">
              <div style="display:inline-block;border-radius:999px;border:1px solid ${palette.border};padding:8px 14px;font-size:12px;font-weight:700;letter-spacing:0.14em;color:${palette.textSecondary};">
                CERUL
              </div>
            </div>

            <h1 style="margin:0 0 16px;color:${palette.textPrimary};font-size:28px;line-height:1.2;font-weight:700;text-align:center;">
              ${escapeHtml(input.title)}
            </h1>

            <p style="margin:0 0 18px;color:${palette.textSecondary};font-size:16px;line-height:1.8;text-align:center;">
              ${escapeHtml(input.intro)}
            </p>

            ${input.body
              .map(
                (paragraph) => `
                  <p style="margin:0 0 16px;color:${palette.textSecondary};font-size:15px;line-height:1.8;">
                    ${escapeHtml(paragraph)}
                  </p>
                `,
              )
              .join("")}

            ${
              input.cta
                ? `
                  <div style="padding:16px 0 8px;text-align:center;">
                    <a
                      href="${escapeHtml(input.cta.href)}"
                      style="display:inline-block;border-radius:12px;background:${palette.accent};padding:14px 32px;color:${palette.buttonText};font-size:15px;font-weight:600;line-height:1;text-decoration:none;"
                    >
                      ${escapeHtml(input.cta.label)}
                    </a>
                  </div>
                `
                : ""
            }

            ${
              input.fallbackLink
                ? `
                  <p style="margin:12px 0 0;color:${palette.textSecondary};font-size:13px;line-height:1.7;text-align:center;word-break:break-word;">
                    If the button does not work, open this link:
                    <br />
                    <a href="${escapeHtml(input.fallbackLink)}" style="color:${palette.accent};text-decoration:none;">
                      ${escapeHtml(input.fallbackLink)}
                    </a>
                  </p>
                `
                : ""
            }

            <div style="margin:28px 0 0;border-top:1px solid ${palette.border};padding-top:20px;">
              <p style="margin:0;color:${palette.textSecondary};font-size:13px;line-height:1.8;">
                ${escapeHtml(input.footerNote)}
              </p>

              ${renderLinks(input.secondaryLinks)}
            </div>
          </div>
        </div>

        <p style="margin:18px 0 0;padding:0 10px;color:${palette.textSecondary};font-size:12px;line-height:1.7;text-align:center;">
          Cerul — where video becomes citable
        </p>
      </div>
    </div>
  </body>
</html>
  `.trim();
}

export function emailVerificationTemplate(params: {
  name: string;
  url: string;
}): string {
  const name = normalizeName(params.name);

  return renderEmailTemplate({
    preview: "Verify your Cerul email address.",
    title: "Verify your email address",
    intro: `Hi ${name}, thanks for signing up to Cerul.`,
    body: [
      "Click below to verify your email and start searching your video by meaning.",
    ],
    cta: {
      href: params.url,
      label: "Verify email",
    },
    fallbackLink: params.url,
    footerNote:
      "This verification link expires in 24 hours. If you did not create a Cerul account, you can safely ignore this email.",
    secondaryLinks: [
      {
        href: `${getSiteOrigin()}/docs`,
        label: "Docs",
      },
      {
        href: "mailto:support@cerul.ai",
        label: "Help",
      },
    ],
  });
}

export function passwordResetTemplate(params: {
  name: string;
  url: string;
}): string {
  const name = normalizeName(params.name);

  return renderEmailTemplate({
    preview: "Reset your Cerul password.",
    title: "Reset your password",
    intro: `Hi ${name}, we received a request to reset your Cerul password.`,
    body: [
      "Use the secure link below to choose a new password for your account.",
    ],
    cta: {
      href: params.url,
      label: "Reset password",
    },
    fallbackLink: params.url,
    footerNote:
      "This reset link expires in 1 hour. If you did not request this, you can safely ignore this email.",
    secondaryLinks: [
      {
        href: "mailto:support@cerul.ai",
        label: "Help",
      },
    ],
  });
}

export function welcomeTemplate(params: {
  name: string;
}): string {
  const name = normalizeName(params.name);
  const siteOrigin = getSiteOrigin();

  return renderEmailTemplate({
    preview: `Welcome to Cerul, ${name}.`,
    title: `Welcome to Cerul, ${name}!`,
    intro: "Your account is ready.",
    body: [
      "You now have 100 signup credits and 10 free searches each day to start testing Cerul with your own workflows.",
      "A default API key is ready in your dashboard, and you can add a payment method there whenever you want the 300-credit monthly free tier.",
      "The quickstart docs walk through the search API, dashboard basics, and how to get your first request live.",
    ],
    cta: {
      href: `${siteOrigin}/dashboard`,
      label: "Go to dashboard",
    },
    fallbackLink: `${siteOrigin}/dashboard`,
    footerNote:
      "Cerul keeps the API surface thin while pushing heavy indexing work into shared workers, so you can focus on building grounded agent experiences.",
    secondaryLinks: [
      {
        href: `${siteOrigin}/docs`,
        label: "Read the quickstart",
      },
      {
        href: `${siteOrigin}/pricing`,
        label: "Pricing",
      },
    ],
  });
}

export function passwordChangedTemplate(params: {
  name: string;
}): string {
  const name = normalizeName(params.name);

  return renderEmailTemplate({
    preview: "Your Cerul password was changed.",
    title: "Password changed",
    intro: `Hi ${name}, your Cerul password was updated successfully.`,
    body: [
      "If you made this change, no further action is needed.",
      "If you did not make this change, contact support immediately so we can help secure your account.",
    ],
    footerNote:
      "Cerul security notices are sent automatically when account credentials change.",
    secondaryLinks: [
      {
        href: "mailto:support@cerul.ai",
        label: "Contact support",
      },
    ],
  });
}
