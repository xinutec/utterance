import { Routes } from "@angular/router";

import { Calibration } from "./features/calibration/calibration";
import { Studio } from "./features/studio/studio";

export const routes: Routes = [
  { path: "", component: Studio },
  { path: "calibrate", component: Calibration },
  // Anything else is a stale link or a typo; send it to the studio.
  { path: "**", redirectTo: "" },
];
