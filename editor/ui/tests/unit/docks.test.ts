// Dock visibility and the window grid (View menu; Studio docks only in team mode, M3 phase 3.8).
import { ALL_DOCKS, visibleDocks, windowGrid } from "../../src/shell/docks";

test("team_docks_are_hidden_outside_team_mode", () => {
  const solo = visibleDocks(ALL_DOCKS, false);
  expect(solo["Team Questions"]).toBe(false);
  expect(solo.Studio).toBe(false);
  expect(solo.Output).toBe(true);
  expect(visibleDocks(ALL_DOCKS, true).Studio).toBe(true);
});

test("the_bottom_panel_grows_for_the_studio_board", () => {
  expect(windowGrid(visibleDocks(ALL_DOCKS, false)).rows).toBe("30px 1fr 140px");
  expect(windowGrid(visibleDocks(ALL_DOCKS, true)).rows).toBe("30px 1fr 280px");
  const none = { ...ALL_DOCKS, Output: false, Graph: false, Studio: false };
  expect(windowGrid(visibleDocks(none, true)).rows).toBe("30px 1fr 0px");
  // The Graph panel is tall while it shows (M4 4.11a).
  expect(windowGrid(visibleDocks(ALL_DOCKS, false), true).rows).toBe("30px 1fr 280px");
});
