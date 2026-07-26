import { Component, inject, OnInit } from "@angular/core";
import { CommonModule } from "@angular/common";
import { Router, RouterModule } from "@angular/router";
import { StudioService, StudioState } from "../../services/studio.service";
import { resolveProfileName } from "../../utils/profile-name";

@Component({
  selector: "ct-dashboard",
  standalone: true,
  imports: [CommonModule, RouterModule],
  template: `
    <div class="dash page-shell">
      <header class="dash__head">
        <div>
          <h1 class="dash__title">Dashboard</h1>
          <p class="dash__sub">Welcome back, {{ displayName }}</p>
        </div>
      </header>

      <section class="dash__stats">
        <div class="stat-card" *ngFor="let s of stats">
          <div class="stat-card__icon" [style.background]="s.bg" [style.color]="s.color">
            <img [src]="s.icon" [alt]="s.label" width="28" height="28" />
          </div>
          <div class="stat-card__body">
            <span class="stat-card__label">{{ s.label }}</span>
            <strong class="stat-card__value">{{ s.value }}</strong>
          </div>
        </div>
      </section>

      <section class="dash__actions">
        <h2 class="dash__section-title">Quick Actions</h2>
        <div class="dash__action-row">
          <button class="action-btn action-btn--primary" (click)="goCollect()">
            <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/></svg>
            Collect & Apply
            <span class="action-arrow">&rarr;</span>
          </button>
          <button class="action-btn" (click)="goApplications()">
            <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>
            View Applications
          </button>
        </div>
      </section>

      <section class="dash__setup">
        <div class="dash__setup-head">
          <h2 class="dash__section-title">Getting Started</h2>
          <span class="dash__setup-pct">{{ setupPercent }}%</span>
        </div>
        <div class="dash__setup-bar">
          <div class="dash__setup-fill" [style.width.%]="setupPercent"></div>
        </div>
        <p class="dash__setup-meta">{{ setupDone }} of {{ setupTotal }} steps complete</p>

        <div class="setup-steps">
          <div class="setup-step" *ngFor="let step of setupSteps" [class.setup-step--done]="step.done">
            <div class="setup-step__dot">
              <svg *ngIf="step.done" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="3"><polyline points="20 6 9 17 4 12"/></svg>
            </div>
            <span class="setup-step__label">{{ step.label }}</span>
          </div>
        </div>
      </section>

      <section class="dash__jobs" *ngIf="scannedJobs.length">
        <h2 class="dash__section-title">Matched Jobs ({{ scannedJobs.length }})</h2>
        <div class="job-list">
          <a class="job-card" *ngFor="let job of scannedJobs" [href]="job.url" target="_blank" rel="noopener">
            <div class="job-card__head">
              <span class="job-card__company">{{ job.company }}</span>
              <span class="job-card__score" *ngIf="job.score">{{ job.score }}/100</span>
            </div>
            <h3 class="job-card__title">{{ job.title }}</h3>
            <p class="job-card__reason" *ngIf="job.score_reason">{{ job.score_reason }}</p>
            <span class="job-card__link">View &rarr;</span>
          </a>
        </div>
      </section>
    </div>
  `,
  styles: [`
    .dash { display: grid; gap: 1.5rem; padding-bottom: 4rem; }

    .dash__head { padding: 0.5rem 0 1rem; }
    .dash__title {
      font-family: var(--font-display); font-size: clamp(2rem, 4vw, 3rem);
      letter-spacing: 0.01em; color: #f4e5cd; margin: 0;
    }
    .dash__sub { color: #baa690; margin: 0.3rem 0 0; font-size: 0.95rem; }

    .dash__stats {
      display: grid; grid-template-columns: repeat(3, 1fr); gap: 0.8rem;
    }
    .stat-card {
      display: flex; align-items: center; gap: 0.9rem; padding: 1.1rem 1.2rem;
      border: 1px solid rgba(209,161,87,0.12); border-radius: 1.2rem;
      background: rgba(255,255,255,0.03); transition: border-color 160ms;
    }
    .stat-card:hover { border-color: rgba(209,161,87,0.25); }
    .stat-card__icon {
      width: 2.6rem; height: 2.6rem; border-radius: 0.8rem; flex-shrink: 0;
      display: grid; place-items: center; overflow: hidden;
    }
    .stat-card__icon img { width: 28px; height: 28px; filter: brightness(0) invert(1); }
    .stat-card__label { font-size: 0.8rem; color: #baa690; display: block; }
    .stat-card__value {
      font-family: var(--font-display); font-size: 1.8rem; color: #f4e5cd;
      display: block; line-height: 1;
    }

    .dash__section-title {
      font-size: 1.1rem; font-weight: 700; color: #f4e5cd; margin: 0 0 0.8rem;
    }

    .dash__actions {
      padding: 1.4rem; border: 1px solid rgba(209,161,87,0.12);
      border-radius: 1.5rem; background: rgba(255,255,255,0.03);
    }
    .dash__action-row { display: flex; flex-wrap: wrap; gap: 0.6rem; }
    .action-btn {
      display: inline-flex; align-items: center; gap: 0.5rem;
      padding: 0.7rem 1.2rem; border-radius: 999px; font-weight: 600; font-size: 0.9rem;
      border: 1px solid rgba(209,161,87,0.2); background: rgba(255,255,255,0.04);
      color: #f4e5cd; cursor: pointer; transition: all 160ms; font: inherit;
    }
    .action-btn:hover { background: rgba(255,255,255,0.08); border-color: rgba(209,161,87,0.35); }
    .action-btn--primary {
      background: linear-gradient(135deg, #ecd4ab, #d1a157); color: #120b07; border-color: #d1a157;
    }
    .action-btn--primary:hover {
      background: linear-gradient(135deg, #f5df90, #d1a157);
      box-shadow: 0 4px 16px rgba(209,161,87,0.3); transform: translateY(-1px);
    }
    .action-arrow { font-size: 1.1rem; }

    .dash__setup {
      padding: 1.4rem; border: 1px solid rgba(209,161,87,0.12);
      border-radius: 1.5rem; background: rgba(255,255,255,0.03);
    }
    .dash__setup-head { display: flex; justify-content: space-between; align-items: baseline; }
    .dash__setup-pct { font-family: var(--font-display); font-size: 1.3rem; color: #d1a157; }
    .dash__setup-bar {
      height: 6px; border-radius: 999px; background: rgba(209,161,87,0.12); overflow: hidden;
    }
    .dash__setup-fill {
      height: 100%; border-radius: 999px;
      background: linear-gradient(90deg, #34d399, #6ee7b7); transition: width 600ms ease;
    }
    .dash__setup-meta { font-size: 0.82rem; color: #baa690; margin: 0.5rem 0 1rem; }

    .setup-steps { display: grid; gap: 0.7rem; }
    .setup-step { display: flex; align-items: center; gap: 0.7rem; }
    .setup-step__dot {
      width: 1.4rem; height: 1.4rem; border-radius: 50%; flex-shrink: 0;
      display: grid; place-items: center;
      border: 2px solid rgba(209,161,87,0.25); background: transparent; transition: all 200ms;
    }
    .setup-step--done .setup-step__dot {
      border-color: #34d399; background: rgba(52,211,153,0.15); color: #34d399;
    }
    .setup-step__label { font-size: 0.92rem; color: #c4b49e; }
    .setup-step--done .setup-step__label { color: #f4e5cd; }

    .dash__jobs {
      padding: 1.4rem; border: 1px solid rgba(209,161,87,0.12);
      border-radius: 1.5rem; background: rgba(255,255,255,0.03);
    }
    .job-list { display: grid; gap: 0.6rem; }
    .job-card {
      display: block; padding: 1rem 1.2rem; border-radius: 1rem; text-decoration: none;
      border: 1px solid rgba(209,161,87,0.15); background: rgba(255,255,255,0.03);
      transition: border-color 160ms, background 160ms;
    }
    .job-card:hover { border-color: rgba(209,161,87,0.35); background: rgba(255,255,255,0.05); }
    .job-card__head { display: flex; justify-content: space-between; align-items: center; }
    .job-card__company { font-size: 0.82rem; color: #baa690; font-weight: 600; }
    .job-card__score {
      font-size: 0.75rem; padding: 0.15rem 0.5rem; border-radius: 999px;
      background: rgba(52,211,153,0.12); color: #34d399; font-weight: 600;
    }
    .job-card__title { margin: 0.3rem 0 0.2rem; font-size: 1rem; color: #f4e5cd; font-weight: 600; }
    .job-card__reason { margin: 0; font-size: 0.82rem; color: #baa690; line-height: 1.4; }
    .job-card__link {
      display: inline-block; margin-top: 0.4rem; font-size: 0.82rem; color: #d1a157; font-weight: 600;
    }

    @media (max-width: 720px) {
      .dash__stats { grid-template-columns: repeat(2, 1fr); }
    }
    @media (max-width: 480px) {
      .dash__stats { grid-template-columns: 1fr; }
    }
  `],
})
export class DashboardComponent implements OnInit {
  private readonly studio = inject(StudioService);
  private readonly router = inject(Router);

  state: StudioState | null = null;
  hasProfile = false;

  stats = [
    { label: "Total Jobs", value: 0, icon: "https://img.icons8.com/?id=482&format=png&size=32", bg: "rgba(255,255,255,0.06)", color: "#f4e5cd" },
    { label: "Applied", value: 0, icon: "https://img.icons8.com/?id=11695&format=png&size=32", bg: "rgba(52,211,153,0.12)", color: "#34d399" },
    { label: "Failed", value: 0, icon: "https://img.icons8.com/?id=3062&format=png&size=32", bg: "rgba(248,113,113,0.12)", color: "#f87171" },
    { label: "Pending", value: 0, icon: "https://img.icons8.com/?id=15849&format=png&size=32", bg: "rgba(251,191,36,0.12)", color: "#fbbf24" },
    { label: "Success Rate", value: "0%", icon: "https://img.icons8.com/?id=4497&format=png&size=32", bg: "rgba(52,211,153,0.12)", color: "#34d399" },
    { label: "Memories", value: 0, icon: "https://img.icons8.com/?id=2070&format=png&size=32", bg: "rgba(168,85,247,0.12)", color: "#a855f7" },
  ];

  setupSteps = [
    { label: "Configure AI provider", done: true },
    { label: "Set resume path", done: true },
    { label: "Set up your profile", done: false },
  ];

  scannedJobs: any[] = [];
  scannedCompanies = 0;

  get setupDone(): number { return this.setupSteps.filter((s) => s.done).length; }
  get setupTotal(): number { return this.setupSteps.length; }
  get setupPercent(): number { return this.setupTotal ? Math.round((this.setupDone / this.setupTotal) * 100) : 0; }

  get displayName(): string {
    return resolveProfileName(
      this.state?.profile?.fullName,
      this.state?.firstName,
      this.state?.lastName,
    ) || "there";
  }

  ngOnInit(): void {
    const local = this.studio.load();
    if (local?.completedAt) {
      this.state = local;
      this.hasProfile = true;
      this.setupSteps[2].done = true;
      this.updateMemoriesStat();
    }

    const jobsData = localStorage.getItem("careertwin-jobs");
    if (jobsData) {
      try {
        const parsed = JSON.parse(jobsData);
        this.scannedJobs = parsed.jobs || [];
        this.scannedCompanies = parsed.scanned_companies || 0;
        this.stats[0].value = this.scannedJobs.length;
      } catch {}
    }

    this.studio.loadFromBackend().subscribe((remote) => {
      if (remote?.completedAt) {
        this.state = remote;
        this.hasProfile = true;
        this.setupSteps[2].done = true;
        this.updateMemoriesStat();
      }
    });
  }

  /** "Memories" = how many concrete facts the AI has learned about the user
   * from their resume/profile (skills, languages, experience, education,
   * certificates) — not a hardcoded placeholder. */
  private updateMemoriesStat(): void {
    const p = this.state?.profile;
    if (!p) return;
    this.stats[5].value =
      p.skills.length + p.languages.length + p.experience.length + p.education.length + p.certificates.length;
  }

  initials(): string {
    const n = resolveProfileName(
      this.state?.profile?.fullName,
      this.state?.firstName,
      this.state?.lastName,
    );
    if (!n) return "CT";
    const p = n.split(/\s+/);
    return (p[0][0] + (p[1]?.[0] ?? "")).toUpperCase();
  }

  goCollect(): void { this.router.navigate(["/jobs"]); }
  goApplications(): void { this.router.navigate(["/applications"]); }
}
