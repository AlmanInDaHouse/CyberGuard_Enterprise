import { InvitationForm } from "@/components/invitation-form";

/** `/invitation` (SPEC-026 §Operational §9, phase 1): an invitation's completion. It takes no
 *  session; the invitation is in the address's fragment, which the page reads (dash_ac_011). */
export default function InvitationPage() {
  return <InvitationForm />;
}
