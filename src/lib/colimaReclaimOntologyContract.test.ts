import { describe, expect, it } from "vitest";
import type { ColimaReclaimPlan } from "./api";

const backendOntologyClass: ColimaReclaimPlan["ontology_class"] =
  "https://disksage.app/ontology#ColimaDownloadCache";

describe("Colima reclaim ontology contract", () => {
  it("accepts the cache class emitted by the public Rust planner", () => {
    expect(backendOntologyClass).toBe("https://disksage.app/ontology#ColimaDownloadCache");
  });
});
