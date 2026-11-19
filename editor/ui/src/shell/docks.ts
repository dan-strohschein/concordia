// Which docks are shown (View menu), and the window grid that results.

/** The toggleable docks, in View menu order. */
export const DOCKS = ["Scene", "FileSystem", "Inspector", "Agent Activity", "Team Questions", "Output"] as const;
export type Dock = (typeof DOCKS)[number];
export type DockSet = Record<Dock, boolean>;

export const ALL_DOCKS: DockSet = Object.fromEntries(DOCKS.map((d) => [d, true])) as DockSet;

/** The window's grid tracks: side columns and the bottom panel collapse when all their docks are hidden. */
export function windowGrid(shown: DockSet): { columns: string; rows: string } {
  const left = shown.Scene || shown.FileSystem ? "250px" : "0px";
  const right = shown.Inspector || shown["Agent Activity"] || shown["Team Questions"] ? "300px" : "0px";
  return { columns: `${left} 1fr ${right}`, rows: `30px 1fr ${shown.Output ? "140px" : "0px"}` };
}
