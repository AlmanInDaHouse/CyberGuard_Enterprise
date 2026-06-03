import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { IncidentDetailView } from "../src/components/incident-detail";
import { IncidentsTable } from "../src/components/incidents-table";
import type { IncidentDetail, IncidentListItem } from "../src/lib/api/types";

// dash_ac_002 — render the teachable view (SPEC-009 §AC dash_ac_002): data → DOM,
// no auth, no fetch. Light render under jsdom (React Testing Library) — NO E2E.
//
// HARNESS-FIRST RED: the presentational components are stubs that mount cleanly but
// render no content, so getByText fails to find the grouped alert / MITRE technique
// / incident row — the RED is ABSENT RENDER LOGIC, not a crashed component. GREEN:
// the detail view renders each resolved alert + the MITRE mapping, and the list
// renders one row per incident.

afterEach(cleanup);

const incident: IncidentDetail = {
  incident_id: "11111111-1111-1111-1111-111111111111",
  agent_id: "22222222-2222-2222-2222-222222222222",
  status: "open",
  title: "Execution activity on host",
  cg_mitre: { tactics: ["execution"], techniques: ["T1059.001"] },
  window_start: "2026-06-01T00:00:00.000Z",
  assigned_to: null,
  created_at: "2026-06-01T00:00:00.000Z",
  updated_at: "2026-06-01T00:05:00.000Z",
  alerts: [
    {
      alert_id: "33333333-3333-3333-3333-333333333333",
      title: "Office spawned a script host",
      severity_id: 4,
      status: "new",
      rule_id: "rule.office_spawns_script_host",
      cg_mitre: { tactics: ["execution"], techniques: ["T1059.001"] },
      event_time: "2026-06-01T00:00:00.000Z",
      final_score: 0.9,
    },
  ],
};

const listItems: IncidentListItem[] = [
  {
    incident_id: "11111111-1111-1111-1111-111111111111",
    agent_id: "22222222-2222-2222-2222-222222222222",
    status: "open",
    title: "Execution activity on host",
    cg_mitre: { tactics: ["execution"], techniques: ["T1059.001"] },
    alert_count: 1,
    window_start: "2026-06-01T00:00:00.000Z",
    updated_at: "2026-06-01T00:05:00.000Z",
  },
];

test("dash_ac_002: incident detail renders the grouped alerts + MITRE mapping", () => {
  render(<IncidentDetailView incident={incident} />);
  // a grouped (resolved) alert
  expect(screen.getByText("Office spawned a script host")).toBeInTheDocument();
  // the MITRE technique (the teachable mapping)
  expect(screen.getByText(/T1059\.001/)).toBeInTheDocument();
});

test("dash_ac_002 (list render): incidents list renders a row per incident", () => {
  render(<IncidentsTable items={listItems} />);
  expect(screen.getByText("Execution activity on host")).toBeInTheDocument();
});
