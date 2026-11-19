// An actor, with its role's color when it is an agent (the generated agent's color, ADR-0083).
import { agentRole } from "./studio";

export function RoleBadge({ actor, color }: { actor: string; color: string | undefined }) {
  const role = agentRole(actor);
  if (role === undefined) return <span style={{ opacity: 0.7 }}>{actor}</span>;
  return (
    <span
      aria-label={`role ${role}`}
      title={role}
      style={{
        padding: "0 4px",
        borderRadius: 3,
        border: "1px solid",
        borderColor: color ?? "#777",
        color: color ?? "inherit",
      }}
    >
      {actor}
    </span>
  );
}
