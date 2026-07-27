import { Routes } from "@angular/router";

import { Studio } from "./features/studio/studio";

export const routes: Routes = [
  { path: "", component: Studio },
  // Anything else is a stale link or a typo; the studio is the only page.
  { path: "**", redirectTo: "" },
];
