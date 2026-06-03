import { redirect } from "next/navigation";

/** Landing → the incidents list (the dashboard's home view, SPEC-009 §Operational §3). */
export default function Home() {
  redirect("/incidents");
}
