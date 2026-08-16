import type { OrphanRelation } from "./api";

/** Return the candidate location relation without depending on relation-array ordering. */
export function locatedInRelation(relations: OrphanRelation[]): OrphanRelation | null {
  return relations.find((relation) => relation.predicate.endsWith("locatedIn")) ?? null;
}
