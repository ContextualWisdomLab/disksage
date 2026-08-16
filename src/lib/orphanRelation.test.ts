import { describe, expect, it } from "vitest";
import type { OrphanRelation } from "./api";
import { locatedInRelation } from "./orphanRelation";

const relation = (predicate: string, object: string): OrphanRelation => ({
  subject: "urn:disk-sage:candidate",
  predicate,
  object,
  source: "disk-sage:test",
});

describe("locatedInRelation", () => {
  it("selects location semantics independently of relation ordering", () => {
    const managedBy = relation("https://disksage.app/ontology#managedBy", "urn:app:example");
    const locatedIn = relation("https://disksage.app/ontology#locatedIn", "/Users/example/Library/Caches");

    expect(locatedInRelation([locatedIn, managedBy])).toEqual(locatedIn);
    expect(locatedInRelation([managedBy, locatedIn])).toEqual(locatedIn);
  });

  it("returns null when the candidate has no location relation", () => {
    expect(
      locatedInRelation([
        relation("https://disksage.app/ontology#managedBy", "urn:app:example"),
      ]),
    ).toBeNull();
  });
});
