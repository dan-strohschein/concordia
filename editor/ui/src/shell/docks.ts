// Which docks are shown (View menu), and the window grid that results.

/** The toggleable docks, in View menu order. */
export const DOCKS = [
  "Scene",
  "FileSystem",
  "Inspector",
  "Agent Activity",
  "Team Questions",
  "Output",
  "Studio",
] as const;
export type Dock = (typeof DOCKS)[number];
export type DockSet = Record<Dock, boolean>;

export const ALL_DOCKS: DockSet = Object.fromEntries(DOCKS.map((d) => [d, true])) as DockSet;

/** Docks that exist only in Agent Studio projects. */
export const TEAM_DOCKS: readonly Dock[] = ["Team Questions", "Studio"];

/** The docks actually on screen: team docks only in team mode. */
export function visibleDocks(shown: DockSet, teamMode: boolean): DockSet {
  return Object.fromEntries(DOCKS.map((d) => [d, shown[d] && (teamMode || !TEAM_DOCKS.includes(d))])) as DockSet;
}

/** The window's grid tracks: side columns and the bottom panel collapse when all their docks are hidden. */
export function windowGrid(visible: DockSet): { columns: string; rows: string } {
  const left = visible.Scene || visible.FileSystem ? "250px" : "0px";
  const right = visible.Inspector || visible["Agent Activity"] || visible["Team Questions"] ? "300px" : "0px";
  const bottom = visible.Studio ? "280px" : visible.Output ? "140px" : "0px";
  return { columns: `${left} 1fr ${right}`, rows: `30px 1fr ${bottom}` };
}
