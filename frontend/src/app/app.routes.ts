import { Routes } from "@angular/router";

import { Calibration } from "./features/calibration/calibration";
import { Compare } from "./features/compare/compare";
import { Studio } from "./features/studio/studio";

export const routes: Routes = [
  { path: "", component: Studio },
  { path: "calibrate", component: Calibration },
  { path: "compare", component: Compare },
  // Anything else is a stale link or a typo; send it to the studio.
  { path: "**", redirectTo: "" },
];
