import { Routes } from "@angular/router";

export const routes: Routes = [
  {
    path: "agents",
    loadComponent: () => import("./pages/agents/agents.component").then((m) => m.AgentsComponent),
  },
  {
    path: "",
    loadComponent: () =>
      import("./pages/home/home.component").then((m) => m.HomeComponent),
  },
  {
    path: "profile",
    loadComponent: () =>
      import("./pages/profile/profile.component").then(
        (m) => m.ProfileComponent
      ),
  },
  {
    path: "studio",
    loadComponent: () =>
      import("./pages/studio/studio.component").then((m) => m.StudioComponent),
  },
  {
    path: "dashboard",
    loadComponent: () =>
      import("./pages/dashboard/dashboard.component").then((m) => m.DashboardComponent),
  },
  {
    path: "jobs",
    loadComponent: () =>
      import("./pages/jobs/jobs.component").then((m) => m.JobsComponent),
  },
  {
    path: "applications",
    loadComponent: () =>
      import("./pages/applications/applications.component").then(
        (m) => m.ApplicationsComponent
      ),
  },
  {
    path: "resume",
    loadComponent: () =>
      import("./pages/resume/resume.component").then((m) => m.ResumeComponent),
  },
  {
    path: "**",
    loadComponent: () =>
      import("./pages/not-found/not-found.component").then(
        (m) => m.NotFoundComponent,
      ),
  },
];
