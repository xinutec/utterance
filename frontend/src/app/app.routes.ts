import { Routes } from "@angular/router";

import { Calibration } from "./features/calibration/calibration";
import { Compare } from "./features/compare/compare";
import { Studio } from "./features/studio/studio";

export const routes: Routes = [
  { path: "", component: Studio },
  // Drilled in from the studio's menu, so up returns there.
  { path: "calibrate", component: Calibration, data: { up: { path: "/", label: "studio" } } },
  { path: "compare", component: Compare, data: { up: { path: "/", label: "studio" } } },
  // Anything else is a stale link or a typo; send it to the studio.
  { path: "**", redirectTo: "" },
];
