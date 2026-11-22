"use client";

import { Button } from "@/components/ui/button";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { type FormEvent, useState } from "react";

/** An item of `GET /v1/users` (SPEC-026 §Data contracts §3). */
export interface UserItem {
  user_id: string;
  email: string;
  role: string;
  status: string;
  totp_enrolled: boolean;
  invitation: { purpose: string; expires_at: string; expired: boolean } | null;
}

const ROLES = ["admin", "analyst", "viewer"] as const;

/** An invitation just issued: its link is held here alone, in this view's memory, until the
 *  admin dismisses it or leaves the page (dash_ac_012). */
type Issued = { email: string; purpose: string; link: string; expiresAt: string };

type Action = "create" | "role" | "status" | "invitation" | "withdraw" | "reset";

/** What the view says for a refusal of the dashboard's handler, by its status and the api's
 *  `error` code. */
function refusal(action: Action, status: number, code: string | undefined): string {
  if (status === 400) {
    return action === "create" ? "Check the email and the role" : "The request was refused";
  }
  if (status === 403) return "The request was refused";
  if (status === 404) return "The user no longer exists";
  if (status === 409) {
    switch (code) {
      case "user_has_credentials":
        return "This user has credentials: a reset replaces them";
      case "user_disabled":
        return "The user is disabled";
      case "cannot_reset_self":
        return "An admin cannot reset their own credentials";
      case "no_open_invitation":
        return "The user has no open invitation";
      default:
        return action === "create"
          ? "A user with this email exists"
          : "The org needs another active admin first";
    }
  }
  if (status === 429) return "Too many attempts; try again later";
  return "The service is unavailable";
}

/** POST a JSON body to one of the dashboard's handlers: its status and its body, or status 0
 *  when it did not answer. */
async function post(path: string, body: unknown): Promise<{ status: number; body: unknown }> {
  try {
    const res = await fetch(path, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
    let parsed: unknown;
    try {
      parsed = await res.json();
    } catch {
      parsed = undefined;
    }
    return { status: res.status, body: parsed };
  } catch {
    return { status: 0, body: undefined };
  }
}

/** A user's open invitation, as the list shows it. */
function invitationText(invitation: UserItem["invitation"]): string {
  if (invitation === null) return "none";
  const expired = invitation.expired ? " (expired)" : "";
  return `${invitation.purpose}, until ${invitation.expires_at}${expired}`;
}

const cell = "border-b border-slate-200 px-2 py-2 align-top";

/**
 * `/users`'s view (SPEC-026 §Operational §9, phase 1): a new user, and for each user the role,
 * the status, a new invitation, its withdrawal and a reset, each through the dashboard's
 * handlers (dash_ac_009). The link of an invitation it issues, `<origin>/invitation#<token>`,
 * is shown here once: it is held in this view's memory alone, never in the address, the
 * browser's storage or a request's URL, and a reload does not show it (dash_ac_012).
 */
export function UsersAdmin({
  page,
  selfId,
  cursor,
}: {
  page: { items: UserItem[]; next_cursor: string | null };
  selfId: string;
  cursor: string | undefined;
}) {
  const router = useRouter();
  const [issued, setIssued] = useState<Issued | null>(null);
  const [message, setMessage] = useState<string | undefined>();
  const [pending, setPending] = useState(false);
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<string>("analyst");
  const [roles, setRoles] = useState<Record<string, string>>({});
  const [confirmReset, setConfirmReset] = useState<string | null>(null);

  /** Run a mutation; on success reload the list and, for an issue, show its link. */
  async function mutate(
    action: Action,
    path: string,
    body: unknown,
    issuedFor?: { email: string; purpose: string },
  ): Promise<boolean> {
    setPending(true);
    setMessage(undefined);
    const res = await post(path, body);
    setPending(false);
    if (res.status === 401) {
      router.push("/login");
      return false;
    }
    if (res.status === 200 || res.status === 201 || res.status === 204) {
      if (issuedFor) {
        const answered = res.body as
          | {
              token?: unknown;
              expires_at?: unknown;
              invitation?: { token?: unknown; expires_at?: unknown };
            }
          | undefined;
        const invitation = answered?.invitation ?? answered;
        if (typeof invitation?.token === "string" && typeof invitation.expires_at === "string") {
          setIssued({
            ...issuedFor,
            link: `${window.location.origin}/invitation#${invitation.token}`,
            expiresAt: invitation.expires_at,
          });
        }
      }
      router.refresh();
      return true;
    }
    const code = (res.body as { error?: unknown } | undefined)?.error;
    setMessage(refusal(action, res.status, typeof code === "string" ? code : undefined));
    return false;
  }

  async function onCreate(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const created = await mutate(
      "create",
      "/api/users",
      { email, role },
      { email: email.trim().toLowerCase(), purpose: "invite" },
    );
    if (created) setEmail("");
  }

  return (
    <div className="flex flex-col gap-6">
      {issued && (
        <section
          aria-label="New invitation"
          className="flex flex-col gap-2 rounded-md border border-amber-400 bg-amber-50 p-4"
        >
          <p className="text-sm">
            The invitation of {issued.email} ({issued.purpose}), valid until {issued.expiresAt}. It
            is shown once: copy it now and hand it to the user outside CyberGuard. Whoever holds it
            can take the account until it is completed.
          </p>
          <label className="flex flex-col gap-1 text-sm" htmlFor="invitation-link">
            Invitation link
            <input
              id="invitation-link"
              readOnly
              value={issued.link}
              onFocus={(e) => e.currentTarget.select()}
              className="rounded-md border border-slate-300 px-3 py-2 font-mono text-xs"
            />
          </label>
          <div>
            <Button onClick={() => setIssued(null)}>Done</Button>
          </div>
        </section>
      )}

      {message && (
        <p role="alert" className="text-sm text-red-700">
          {message}
        </p>
      )}

      <form className="flex flex-wrap items-end gap-3" onSubmit={onCreate}>
        <label className="flex flex-col gap-1 text-sm" htmlFor="new-user-email">
          Email
          <input
            id="new-user-email"
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            className="rounded-md border border-slate-300 px-3 py-2"
          />
        </label>
        <label className="flex flex-col gap-1 text-sm" htmlFor="new-user-role">
          Role
          <select
            id="new-user-role"
            value={role}
            onChange={(e) => setRole(e.target.value)}
            className="rounded-md border border-slate-300 px-3 py-2"
          >
            {ROLES.map((r) => (
              <option key={r} value={r}>
                {r}
              </option>
            ))}
          </select>
        </label>
        <Button type="submit" disabled={pending}>
          Create user
        </Button>
      </form>

      <table className="w-full text-left text-sm">
        <thead>
          <tr>
            <th className={cell}>Email</th>
            <th className={cell}>Role</th>
            <th className={cell}>Status</th>
            <th className={cell}>Authenticator</th>
            <th className={cell}>Invitation</th>
            <th className={cell}>Actions</th>
          </tr>
        </thead>
        <tbody>
          {page.items.map((u) => {
            const self = u.user_id === selfId;
            const active = u.status === "active";
            const chosen = roles[u.user_id] ?? u.role;
            return (
              <tr key={u.user_id}>
                <td className={cell}>{u.email}</td>
                <td className={cell}>
                  <div className="flex items-center gap-2">
                    <select
                      aria-label={`Role of ${u.email}`}
                      value={chosen}
                      onChange={(e) => setRoles({ ...roles, [u.user_id]: e.target.value })}
                      className="rounded-md border border-slate-300 px-2 py-1"
                    >
                      {ROLES.map((r) => (
                        <option key={r} value={r}>
                          {r}
                        </option>
                      ))}
                    </select>
                    <Button
                      disabled={pending || chosen === u.role}
                      onClick={() =>
                        mutate("role", `/api/users/${u.user_id}/role`, { role: chosen })
                      }
                    >
                      Change role
                    </Button>
                  </div>
                </td>
                <td className={cell}>
                  <div className="flex items-center gap-2">
                    <span>{u.status}</span>
                    {!self && (
                      <Button
                        disabled={pending}
                        onClick={() =>
                          mutate("status", `/api/users/${u.user_id}/status`, {
                            status: active ? "disabled" : "active",
                          })
                        }
                      >
                        {active ? "Disable" : "Enable"}
                      </Button>
                    )}
                  </div>
                </td>
                <td className={cell}>{u.totp_enrolled ? "confirmed" : "not confirmed"}</td>
                <td className={cell}>{invitationText(u.invitation)}</td>
                <td className={cell}>
                  <div className="flex flex-wrap gap-2">
                    {!self && active && !u.totp_enrolled && (
                      <Button
                        disabled={pending}
                        onClick={() =>
                          mutate(
                            "invitation",
                            `/api/users/${u.user_id}/invitation`,
                            {},
                            { email: u.email, purpose: "invite" },
                          )
                        }
                      >
                        New invitation
                      </Button>
                    )}
                    {u.invitation && (
                      <Button
                        disabled={pending}
                        onClick={() =>
                          mutate("withdraw", `/api/users/${u.user_id}/invitation/withdraw`, {})
                        }
                      >
                        Withdraw invitation
                      </Button>
                    )}
                    {!self && active && confirmReset !== u.user_id && (
                      <Button disabled={pending} onClick={() => setConfirmReset(u.user_id)}>
                        Reset
                      </Button>
                    )}
                    {!self && active && confirmReset === u.user_id && (
                      <>
                        <Button
                          disabled={pending}
                          className="bg-red-700"
                          onClick={async () => {
                            setConfirmReset(null);
                            await mutate(
                              "reset",
                              `/api/users/${u.user_id}/reset`,
                              {},
                              { email: u.email, purpose: "reset" },
                            );
                          }}
                        >
                          Confirm reset
                        </Button>
                        <Button disabled={pending} onClick={() => setConfirmReset(null)}>
                          Cancel
                        </Button>
                      </>
                    )}
                  </div>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>

      <nav aria-label="Pages" className="flex gap-4 text-sm">
        {cursor !== undefined && (
          <Link href="/users" className="underline">
            First page
          </Link>
        )}
        {page.next_cursor !== null && (
          <Link
            href={`/users?cursor=${encodeURIComponent(page.next_cursor)}`}
            className="underline"
          >
            Next page
          </Link>
        )}
      </nav>
    </div>
  );
}
