import { AfterViewInit, Component, inject } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";
import { HttpClientModule } from "@angular/common/http";
import { RouterLink } from "@angular/router";
import { DomSanitizer, SafeResourceUrl } from "@angular/platform-browser";
import gsap from "gsap";
import {
  ApplyJobResponse,
  DiscoverJobsResponse,
  DiscoveredJob,
  ResumeService,
  TailorResponse,
} from "../../services/resume.service";
import { ProfileService, UserProfile } from "../../services/profile.service";
import { resolveProfileName } from "../../utils/profile-name";

type SourceMode = "upload" | "manual";
type Step = "profile" | "jobs" | "package";

@Component({
  selector: "ct-resume",
  standalone: true,
  imports: [CommonModule, FormsModule, HttpClientModule, RouterLink],
  template: `
    <div class="resume-studio page-shell">
      <section class="resume-shell">
        <header class="resume-head">
          <div class="resume-copy">
            <div class="eyebrow">Resume Studio</div>
            <h1 class="resume-title">Profile in. Matching vacancies out. Tailored package next.</h1>
            <p class="resume-subtitle">
              The agent enriches your profile from resume, GitHub and website,
              finds matching roles, researches the company, chooses the language, and generates
              both CV and cover letter.
            </p>
          </div>

          <div class="resume-head__actions">
            <a routerLink="/profile" class="badge-button badge-button--link">
              <span>Profile</span>
              <strong>Open profile</strong>
              <small>Manual edit</small>
            </a>
          </div>
        </header>

        <section class="browser-session-banner" *ngIf="browserSession">
          <div>
            <span class="browser-session-banner__label">Agent Browser Session</span>
            <strong>{{ browserSession.session || "active-session" }}</strong>
          </div>
          <div>
            <span class="browser-session-banner__label">Provider</span>
            <strong>{{ browserSession.provider || "agent-browser" }}</strong>
          </div>
          <div>
            <span class="browser-session-banner__label">Status</span>
            <strong>{{ browserSession.status || "ready" }}</strong>
          </div>
          <p>{{ browserSession.detail || "Visible browser automation session is available." }}</p>
        </section>

        <div class="step-strip">
          <button class="step-pill" type="button" [class.step-pill--active]="step === 'profile'" (click)="step = 'profile'">1. Profile source</button>
          <button class="step-pill" type="button" [class.step-pill--active]="step === 'jobs'" [disabled]="!canOpenJobsStep()" (click)="step = 'jobs'">2. Matching vacancies</button>
          <button class="step-pill" type="button" [class.step-pill--active]="step === 'package'" [disabled]="!selectedJob" (click)="step = 'package'">3. Package output</button>
        </div>

        <div class="resume-grid" *ngIf="step === 'profile'">
          <section class="panel-card panel-card--span-7">
            <h2>1. Candidate source</h2>

            <div class="mode-switch">
              <button
                class="mode-card"
                type="button"
                [class.mode-card--active]="mode === 'upload'"
                (click)="setMode('upload')"
              >
                <span class="mode-card__kicker">Existing resume</span>
                <strong>I already have a PDF</strong>
                <small>Upload resume, then enrich from public sources.</small>
              </button>
              <button
                class="mode-card"
                type="button"
                [class.mode-card--active]="mode === 'manual'"
                (click)="setMode('manual')"
              >
                <span class="mode-card__kicker">Manual profile</span>
                <strong>I want to describe myself</strong>
                <small>Fill base identity and skills in any language.</small>
              </button>
            </div>

            <div *ngIf="mode === 'upload'" class="upload-zone">
              <input #resumeInput class="sr-only" type="file" accept=".pdf,.doc,.docx,.txt" (change)="onFileSelected($event)" />
              <button class="upload-cta" type="button" (click)="resumeInput.click()">Choose resume file</button>
              <div class="upload-meta">
                <strong>{{ selectedFileName || "No file selected yet" }}</strong>
                <span>Supported: PDF, DOCX, TXT</span>
              </div>
            </div>

            <div class="form-grid" *ngIf="mode === 'manual'">
              <label class="field field--full">
                <span>Full name</span>
                <input [(ngModel)]="form.fullName" placeholder="Maksim Krauchyk" />
              </label>
              <label class="field">
                <span>Email</span>
                <input [(ngModel)]="form.email" placeholder="you@example.com" />
              </label>
              <label class="field">
                <span>Phone</span>
                <input [(ngModel)]="form.phone" placeholder="+48 123 456 789" />
              </label>
              <label class="field">
                <span>GitHub</span>
                <input [(ngModel)]="form.github" placeholder="https://github.com/username" />
              </label>
              <label class="field">
                <span>LinkedIn</span>
                <input [(ngModel)]="form.linkedin" placeholder="https://linkedin.com/in/username" />
              </label>
              <label class="field field--full">
                <span>Website / portfolio</span>
                <input [(ngModel)]="form.website" placeholder="https://your-site.dev" />
              </label>
              <label class="field field--full">
                <span>What do you do?</span>
                <textarea
                  [(ngModel)]="form.summary"
                  rows="7"
                  placeholder="Describe your stack, projects and achievements in any language: Python, FastAPI, Angular, aiogram, Docker, Redis, CUDA, LLM agents..."
                ></textarea>
              </label>
            </div>
          </section>

          <section class="panel-card panel-card--span-5">
            <h2>Enrichment sources</h2>
            <div class="form-grid">
              <label class="field field--full" *ngIf="mode === 'upload'">
                <span>GitHub</span>
                <input [(ngModel)]="form.github" placeholder="https://github.com/username" />
              </label>
              <label class="field field--full" *ngIf="mode === 'upload'">
                <span>LinkedIn</span>
                <input [(ngModel)]="form.linkedin" placeholder="https://linkedin.com/in/username" />
              </label>
              <label class="field field--full" *ngIf="mode === 'upload'">
                <span>Website / portfolio</span>
                <input [(ngModel)]="form.website" placeholder="https://your-site.dev" />
              </label>
              <label class="field">
                <span>Provider</span>
                <select [(ngModel)]="form.provider" (ngModelChange)="syncProviderModel($event)">
                  <option *ngFor="let provider of providers" [value]="provider.id">{{ provider.id }}</option>
                </select>
              </label>
              <label class="field">
                <span>Model</span>
                <input [(ngModel)]="form.model" placeholder="qwythos-9b-v2" />
              </label>
            </div>

            <div class="provider-status-grid" *ngIf="providerStatuses.length">
              <article
                class="provider-pill"
                *ngFor="let provider of providerStatuses"
                [class.provider-pill--online]="provider.status === 'connected'"
              >
                <strong>{{ provider.id }}</strong>
                <span>{{ provider.status }}</span>
                <small>{{ provider.reason || provider.model || provider.default_model || "" }}</small>
              </article>
            </div>

            <div class="action-stack">
              <button class="generate-btn" type="button" (click)="discoverJobs()" [disabled]="discovering || !canDiscover()">
                <span *ngIf="!discovering">Find matching vacancies</span>
                <span *ngIf="discovering">Searching job boards and ranking roles...</span>
              </button>
              <p class="status-text" *ngIf="discoveryStatus">{{ discoveryStatus }}</p>
            </div>
          </section>
        </div>

        <div class="resume-grid" *ngIf="step === 'jobs'">
          <section class="panel-card panel-card--span-12">
            <div class="section-head">
              <div>
                <h2>2. Matching vacancies</h2>
                <p class="panel-note">
                  {{ discoverResult?.query ? ("Search query: " + discoverResult?.query) : "The agent ranks vacancies against your profile and resume context." }}
                </p>
              </div>
              <div class="section-head__actions">
                <div class="source-chip-row">
                  <button
                    *ngFor="let source of availableSources"
                    class="source-chip"
                    type="button"
                    [class.source-chip--active]="selectedSources.includes(source.id)"
                    (click)="toggleSource(source.id)"
                  >
                    {{ source.name }}
                  </button>
                </div>
                <button class="generate-btn generate-btn--ghost generate-btn--inline" type="button" (click)="discoverJobs()" [disabled]="discovering || !canDiscover()">
                  Refresh vacancies
                </button>
              </div>
            </div>

            <div class="job-list" *ngIf="discoverResult?.jobs?.length; else emptyJobs">
              <article
                class="job-card"
                *ngFor="let job of discoverResult?.jobs"
                [class.job-card--selected]="selectedJob?.url === job.url"
              >
                <div class="job-card__top">
                  <div>
                    <div class="job-card__meta">{{ job.source }} · {{ job.company || "Unknown company" }}</div>
                    <h3>{{ job.title }}</h3>
                  </div>
                  <div class="score-badge">{{ job.match_score }}%</div>
                </div>

                <p class="job-card__desc">{{ job.description || "No description extracted yet." }}</p>

                <div class="chip-row" *ngIf="job.matching_skills.length">
                  <span class="chip" *ngFor="let skill of job.matching_skills">{{ skill }}</span>
                </div>

                <div class="job-card__footer">
                  <a class="text-link" [href]="job.url" target="_blank" rel="noreferrer">Open listing</a>
                  <button class="generate-btn generate-btn--inline" type="button" (click)="selectJob(job)">
                    {{ selectedJob?.url === job.url ? "Selected" : "Use this vacancy" }}
                  </button>
                </div>
              </article>
            </div>

            <ng-template #emptyJobs>
              <p class="empty-state">No vacancies found yet. Check the provider/backend status and try refresh.</p>
            </ng-template>
          </section>
        </div>

        <div class="resume-grid" *ngIf="step === 'package' && selectedJob">
          <section class="panel-card panel-card--span-7">
            <h2>3. Package setup</h2>
            <div class="selected-vacancy">
              <strong>{{ selectedJob.title }}</strong>
              <span>{{ selectedJob.company || "Unknown company" }} · {{ selectedJob.source }} · {{ selectedJob.match_score }}% match</span>
            </div>

            <div class="form-grid">
              <label class="field">
                <span>Template</span>
                <select [(ngModel)]="form.template">
                  <option *ngFor="let template of templates" [value]="template.id">{{ template.name }}</option>
                </select>
              </label>
              <label class="field">
                <span>Language</span>
                <select [(ngModel)]="form.language">
                  <option value="auto">Auto detect</option>
                  <option *ngFor="let language of languages" [value]="language.code">{{ language.name }}</option>
                </select>
              </label>
            </div>

            <div class="action-row">
              <button class="generate-btn" type="button" (click)="generatePackage()" [disabled]="generating || !selectedJob">
                <span *ngIf="!generating">Generate CV + cover letter</span>
                <span *ngIf="generating">Building tailored application package...</span>
              </button>
              <button class="generate-btn generate-btn--ghost" type="button" (click)="openAutomationPreview()" [disabled]="openingAutomation || !result?.cover_letter">
                <span *ngIf="!openingAutomation">Open automation preview</span>
                <span *ngIf="openingAutomation">Opening job form in browser...</span>
              </button>
            </div>

            <div class="progress" *ngIf="generating || progress > 0">
              <div class="progress__bar" [style.width.%]="progress"></div>
            </div>
            <p class="status-text" *ngIf="statusText">{{ statusText }}</p>

            <div class="automation-card" *ngIf="automationResult">
              <h3>Automation preview</h3>
              <p>{{ automationResult.message || automationResult.status || automationResult.error }}</p>
              <div class="chip-row" *ngIf="automationResult.filled_fields?.length">
                <span class="chip" *ngFor="let item of automationResult.filled_fields">{{ item }}</span>
              </div>
              <div class="trace-list" *ngIf="automationResult.agent_trace?.length">
                <article class="trace-item" *ngFor="let item of automationResult.agent_trace">
                  <div class="trace-item__top">
                    <strong>Step {{ item.step }} · {{ item.action }}</strong>
                    <span>{{ item.status }}</span>
                  </div>
                  <p>{{ item.reason || "No reasoning returned." }}</p>
                  <small *ngIf="item.target">Target: {{ item.target }}</small>
                  <small *ngIf="item.value_preview">Value: {{ item.value_preview }}</small>
                </article>
              </div>
            </div>
          </section>

          <section class="panel-card panel-card--span-5">
            <h2>Agent output</h2>
            <div class="diag-card" *ngFor="let item of diagnosticsEntries(activeDiagnostics)">
              <strong>{{ item.key }}</strong>
              <span [class.diag-ok]="item.value.status === 'success'" [class.diag-warn]="item.value.status === 'warning'" [class.diag-error]="item.value.status === 'error'">
                {{ item.value.status }}
              </span>
              <p>{{ item.value.detail }}</p>
            </div>
          </section>
        </div>
      </section>

      <section class="results-shell" *ngIf="result">
        <div class="results-head">
          <div>
            <div class="eyebrow">Generated package</div>
            <h2>{{ result.job.title }}</h2>
            <p>
              {{ result.job.company || "Target company" }} ·
              Language: <strong>{{ result.language_label }}</strong>
            </p>
          </div>
          <div class="result-links">
            <a class="download-btn" [href]="apiBase + result.pdf_url" target="_blank" rel="noreferrer">Open resume PDF</a>
            <a class="download-btn download-btn--ghost" [href]="apiBase + result.cover_letter_pdf_url" target="_blank" rel="noreferrer">Open cover letter PDF</a>
          </div>
        </div>

        <div class="metrics-grid">
          <article class="metric-card">
            <span>ATS score</span>
            <strong>{{ result.ats_score }}</strong>
          </article>
          <article class="metric-card">
            <span>Job match</span>
            <strong>{{ result.job_match_score }}</strong>
          </article>
          <article class="metric-card">
            <span>Readiness</span>
            <strong>{{ result.readiness_score }}</strong>
          </article>
          <article class="metric-card">
            <span>Language</span>
            <strong>{{ result.language_label }}</strong>
          </article>
        </div>

        <div class="results-grid">
          <article id="preview-document" class="panel-card panel-card--soft panel-card--wide">
            <div class="preview-head">
              <h3>Resume preview</h3>
              <div class="preview-actions">
                <button class="preview-toggle" type="button" [class.preview-toggle--active]="previewMode === 'html'" (click)="previewMode = 'html'">HTML</button>
                <button class="preview-toggle" type="button" [class.preview-toggle--active]="previewMode === 'pdf'" (click)="previewMode = 'pdf'">PDF</button>
              </div>
            </div>
            <iframe *ngIf="previewMode === 'html'" class="preview-frame" [src]="trustedHtmlUrl" title="Resume HTML preview"></iframe>
            <iframe *ngIf="previewMode === 'pdf'" class="preview-frame preview-frame--pdf" [src]="trustedPdfUrl" title="Resume PDF preview"></iframe>
          </article>

          <article class="panel-card panel-card--soft">
            <h3>Company research</h3>
            <p class="panel-note">{{ result.company_research.reasoning }}</p>
            <p>{{ result.company_research.company_summary || "No company summary available." }}</p>
          </article>

          <article class="panel-card panel-card--soft">
            <h3>Selected GitHub projects</h3>
            <div class="project-card" *ngFor="let project of result.resume.projects">
              <strong>{{ project.name }}</strong>
              <p>{{ project.description || "No description provided." }}</p>
              <a *ngIf="project.url" [href]="project.url" target="_blank" rel="noreferrer">{{ project.url }}</a>
            </div>
            <p *ngIf="!result.resume.projects.length">No projects were extracted from GitHub or the resume source.</p>
          </article>

          <article class="panel-card panel-card--soft">
            <h3>Experience highlights</h3>
            <div class="project-card" *ngFor="let item of result.resume.experience">
              <strong>{{ item.position }}</strong>
              <p>{{ item.company }}</p>
              <pre>{{ item.description }}</pre>
            </div>
          </article>

          <article id="preview-cover" class="panel-card panel-card--soft panel-card--wide">
            <h3>Cover letter</h3>
            <pre>{{ result.cover_letter }}</pre>
          </article>
        </div>
      </section>
    </div>

  `,
  styles: [`
    .sr-only { position: absolute; width: 1px; height: 1px; margin: -1px; overflow: hidden; clip: rect(0,0,0,0); border: 0; }
    .resume-studio { display: grid; gap: 1.5rem; padding-bottom: 4rem; }
    .resume-shell, .results-shell {
      background:
        radial-gradient(circle at top right, rgba(214, 161, 85, 0.18), transparent 24%),
        linear-gradient(180deg, rgba(31, 19, 11, 0.98), rgba(15, 9, 5, 0.99));
      border: 1px solid rgba(244, 229, 205, 0.08);
      border-radius: 2rem;
      padding: 1.35rem;
      box-shadow: 0 26px 90px rgba(10, 6, 3, 0.34);
    }
    .resume-head, .results-head {
      display: flex;
      gap: 1rem;
      justify-content: space-between;
      align-items: flex-start;
      margin-bottom: 1.25rem;
      flex-wrap: wrap;
    }
    .resume-head__actions { display: grid; gap: 0.75rem; min-width: 16rem; }
    .resume-copy { max-width: 52rem; }
    .resume-title {
      font-family: "Bebas Neue", sans-serif;
      font-size: clamp(2.35rem, 5vw, 4.7rem);
      line-height: 0.92;
      margin: 0.35rem 0 0.75rem;
      letter-spacing: 0.03em;
      text-transform: uppercase;
    }
    .resume-subtitle, .results-head p, .panel-note, .job-card__desc {
      color: rgba(244, 229, 205, 0.7);
      line-height: 1.7;
    }
    .badge-button {
      min-width: 16rem;
      border-radius: 1.4rem;
      border: 1px solid rgba(244, 229, 205, 0.1);
      background: rgba(255, 255, 255, 0.04);
      padding: 1rem;
      text-decoration: none;
      color: inherit;
      cursor: pointer;
      text-align: left;
      transition: transform 180ms ease, border-color 180ms ease, background 180ms ease;
    }
    .badge-button:hover { transform: translateY(-2px); border-color: rgba(236, 212, 171, 0.3); }
    .badge-button--action { background: linear-gradient(180deg, rgba(36, 130, 196, 0.16), rgba(17, 72, 111, 0.2)); }
    .badge-button span, .metric-card span, .mode-card__kicker {
      display: block;
      font-size: 0.75rem;
      text-transform: uppercase;
      letter-spacing: 0.16em;
      color: rgba(244, 229, 205, 0.56);
    }
    .badge-button strong, .metric-card strong { display: block; margin-top: 0.35rem; font-size: 1.05rem; }
    .badge-button small { display: block; margin-top: 0.35rem; color: #f5df90; word-break: break-word; }
    .step-strip { display: flex; flex-wrap: wrap; gap: 0.65rem; margin-bottom: 1rem; }
    .browser-session-banner {
      margin-bottom: 1rem;
      border-radius: 1.4rem;
      border: 1px solid rgba(90, 169, 230, 0.24);
      background: linear-gradient(180deg, rgba(36, 130, 196, 0.12), rgba(14, 47, 73, 0.16));
      padding: 1rem;
      display: grid;
      grid-template-columns: repeat(3, minmax(0, 1fr));
      gap: 0.85rem;
    }
    .browser-session-banner__label {
      display: block;
      font-size: 0.72rem;
      letter-spacing: 0.14em;
      text-transform: uppercase;
      color: rgba(244, 229, 205, 0.56);
      margin-bottom: 0.25rem;
    }
    .browser-session-banner p {
      grid-column: 1 / -1;
      margin: 0;
      color: rgba(244, 229, 205, 0.82);
      line-height: 1.6;
    }
    .step-pill {
      border-radius: 999px;
      border: 1px solid rgba(244, 229, 205, 0.1);
      background: rgba(255, 255, 255, 0.03);
      color: var(--color-foreground);
      padding: 0.65rem 1rem;
      cursor: pointer;
      font-weight: 600;
    }
    .step-pill--active { background: #ecd4ab; color: #120b07; border-color: #ecd4ab; }
    .step-pill:disabled { opacity: 0.45; cursor: not-allowed; }
    .resume-grid, .results-grid { display: grid; grid-template-columns: repeat(12, minmax(0, 1fr)); gap: 1rem; }
    .panel-card {
      grid-column: span 4;
      border-radius: 1.5rem;
      border: 1px solid rgba(244, 229, 205, 0.08);
      background: rgba(255, 255, 255, 0.025);
      padding: 1rem;
      min-width: 0;
    }
    .panel-card--span-12 { grid-column: span 12; }
    .panel-card--span-7 { grid-column: span 7; }
    .panel-card--span-5 { grid-column: span 5; }
    .panel-card--soft { background: rgba(255, 255, 255, 0.02); }
    .panel-card--wide { grid-column: span 8; }
    .panel-card h2, .panel-card h3 {
      margin: 0 0 0.85rem;
      font-size: 0.96rem;
      text-transform: uppercase;
      letter-spacing: 0.14em;
      color: #f5df90;
    }
    .section-head {
      display: flex;
      align-items: flex-start;
      justify-content: space-between;
      gap: 1rem;
      margin-bottom: 1rem;
      flex-wrap: wrap;
    }
    .section-head__actions { display: grid; gap: 0.75rem; justify-items: end; }
    .mode-switch { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 0.8rem; margin-bottom: 1rem; }
    .mode-card {
      border: 1px solid rgba(244, 229, 205, 0.1);
      background: rgba(255, 255, 255, 0.03);
      border-radius: 1.4rem;
      padding: 1rem;
      color: var(--color-foreground);
      text-align: left;
      cursor: pointer;
      min-width: 0;
    }
    .mode-card strong, .mode-card small { display: block; }
    .mode-card strong { margin-top: 0.4rem; font-size: 1rem; }
    .mode-card small { margin-top: 0.4rem; color: rgba(244, 229, 205, 0.68); line-height: 1.55; }
    .mode-card--active {
      border-color: rgba(236, 212, 171, 0.34);
      background: linear-gradient(180deg, rgba(236, 212, 171, 0.12), rgba(236, 212, 171, 0.04));
    }
    .upload-zone {
      display: flex;
      gap: 0.85rem;
      align-items: center;
      border: 1px dashed rgba(244, 229, 205, 0.16);
      border-radius: 1.35rem;
      padding: 1rem;
      background: rgba(255, 255, 255, 0.03);
      margin-top: 0.5rem;
      flex-wrap: wrap;
    }
    .upload-cta {
      border: none;
      border-radius: 999px;
      background: #ecd4ab;
      color: #120b07;
      padding: 0.85rem 1.15rem;
      font-weight: 700;
      cursor: pointer;
      flex-shrink: 0;
    }
    .upload-meta { min-width: 0; display: grid; gap: 0.25rem; }
    .upload-meta strong {
      overflow-wrap: anywhere;
      color: var(--color-foreground);
    }
    .upload-meta span { color: rgba(244, 229, 205, 0.6); font-size: 0.9rem; }
    .form-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 0.85rem; }
    .field { display: grid; gap: 0.38rem; min-width: 0; }
    .field--full { grid-column: 1 / -1; }
    .field span { font-size: 0.8rem; color: rgba(244, 229, 205, 0.6); text-transform: uppercase; letter-spacing: 0.12em; }
    .field input, .field textarea, .field select {
      width: 100%;
      min-width: 0;
      border: 1px solid rgba(244, 229, 205, 0.12);
      border-radius: 1rem;
      background: rgba(255, 255, 255, 0.04);
      color: var(--color-foreground);
      padding: 0.85rem 0.95rem;
      outline: none;
      font: inherit;
    }
    .field textarea { resize: vertical; min-height: 8rem; }
    .field select {
      appearance: none;
      background-image:
        linear-gradient(45deg, transparent 50%, #f5df90 50%),
        linear-gradient(135deg, #f5df90 50%, transparent 50%);
      background-position:
        calc(100% - 18px) calc(50% - 3px),
        calc(100% - 12px) calc(50% - 3px);
      background-size: 6px 6px, 6px 6px;
      background-repeat: no-repeat;
      padding-right: 2.4rem;
      color-scheme: dark;
    }
    .provider-status-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 0.7rem; margin-top: 1rem; }
    .provider-pill {
      border-radius: 1rem;
      border: 1px solid rgba(244, 229, 205, 0.12);
      padding: 0.8rem;
      background: rgba(255, 255, 255, 0.03);
      display: grid;
      gap: 0.2rem;
    }
    .provider-pill--online { border-color: rgba(52, 211, 153, 0.35); background: rgba(52, 211, 153, 0.08); }
    .provider-pill span, .provider-pill small { color: rgba(244, 229, 205, 0.64); overflow-wrap: anywhere; }
    .action-stack { margin-top: 1rem; }
    .action-row { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 0.75rem; margin-top: 1rem; }
    .result-links { display: grid; gap: 0.65rem; min-width: 15rem; }
    .generate-btn, .download-btn {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      border: none;
      border-radius: 999px;
      background: #ecd4ab;
      color: #120b07;
      padding: 0.95rem 1.25rem;
      font-weight: 700;
      cursor: pointer;
      text-decoration: none;
      transition: transform 180ms ease, opacity 180ms ease;
      width: 100%;
    }
    .generate-btn--inline { width: auto; }
    .generate-btn--ghost, .download-btn--ghost {
      background: rgba(236, 212, 171, 0.08);
      color: var(--color-foreground);
      border: 1px solid rgba(236, 212, 171, 0.2);
    }
    .generate-btn:disabled { opacity: 0.55; cursor: not-allowed; }
    .generate-btn:not(:disabled):hover, .download-btn:hover { transform: translateY(-1px); }
    .source-chip-row, .chip-row { display: flex; gap: 0.45rem; flex-wrap: wrap; }
    .source-chip {
      border-radius: 999px;
      border: 1px solid rgba(244, 229, 205, 0.12);
      background: rgba(255, 255, 255, 0.03);
      color: var(--color-foreground);
      padding: 0.5rem 0.8rem;
      cursor: pointer;
    }
    .source-chip--active {
      background: rgba(236, 212, 171, 0.12);
      border-color: rgba(236, 212, 171, 0.3);
      color: #f5df90;
    }
    .job-list { display: grid; gap: 0.9rem; }
    .job-card {
      border: 1px solid rgba(244, 229, 205, 0.08);
      border-radius: 1.3rem;
      padding: 1rem;
      background: rgba(255, 255, 255, 0.025);
    }
    .job-card--selected { border-color: rgba(236, 212, 171, 0.34); background: rgba(236, 212, 171, 0.06); }
    .job-card__top, .job-card__footer, .preview-head {
      display: flex;
      align-items: flex-start;
      justify-content: space-between;
      gap: 1rem;
      flex-wrap: wrap;
    }
    .job-card__meta { color: rgba(244, 229, 205, 0.58); font-size: 0.82rem; text-transform: uppercase; letter-spacing: 0.12em; }
    .job-card h3 { margin: 0.35rem 0 0; font-size: 1.1rem; color: var(--color-foreground); }
    .score-badge, .chip {
      padding: 0.45rem 0.7rem;
      border-radius: 999px;
      background: rgba(236, 212, 171, 0.1);
      border: 1px solid rgba(236, 212, 171, 0.16);
      color: #f5df90;
      font-size: 0.85rem;
    }
    .job-card__footer { margin-top: 0.9rem; }
    .text-link, .project-card a { color: #f5df90; overflow-wrap: anywhere; }
    .selected-vacancy {
      border: 1px solid rgba(244, 229, 205, 0.08);
      border-radius: 1.1rem;
      padding: 0.9rem 1rem;
      background: rgba(255, 255, 255, 0.03);
      display: grid;
      gap: 0.25rem;
      margin-bottom: 1rem;
    }
    .selected-vacancy span { color: rgba(244, 229, 205, 0.66); }
    .progress {
      height: 0.35rem;
      background: rgba(255, 255, 255, 0.06);
      border-radius: 999px;
      overflow: hidden;
      margin-top: 0.85rem;
    }
    .progress__bar { height: 100%; background: linear-gradient(90deg, #d1a157, #f5df90); transition: width 220ms ease; }
    .status-text { margin-top: 0.8rem; color: rgba(244, 229, 205, 0.72); line-height: 1.6; }
    .metrics-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 0.85rem; margin-bottom: 1rem; }
    .metric-card {
      border-radius: 1.2rem;
      border: 1px solid rgba(244, 229, 205, 0.12);
      background: rgba(255, 255, 255, 0.04);
      padding: 1rem;
    }
    .preview-actions { display: flex; gap: 0.5rem; }
    .preview-toggle {
      border: 1px solid rgba(244, 229, 205, 0.12);
      background: rgba(255, 255, 255, 0.04);
      color: var(--color-foreground);
      border-radius: 999px;
      padding: 0.45rem 0.8rem;
      cursor: pointer;
    }
    .preview-toggle--active { background: #ecd4ab; color: #120b07; border-color: #ecd4ab; }
    .preview-frame {
      width: 100%;
      min-height: 46rem;
      border: 1px solid rgba(244, 229, 205, 0.08);
      border-radius: 1rem;
      background: white;
    }
    .preview-frame--pdf { background: #111; }
    pre {
      white-space: pre-wrap;
      word-break: break-word;
      font-family: "Instrument Sans", sans-serif;
      margin: 0;
      line-height: 1.7;
    }
    .project-card + .project-card, .diag-card + .diag-card {
      margin-top: 0.9rem;
      padding-top: 0.9rem;
      border-top: 1px solid rgba(244, 229, 205, 0.08);
    }
    .project-card p, .diag-card p { margin: 0.35rem 0 0; color: rgba(244, 229, 205, 0.68); }
    .diag-card span {
      display: inline-block;
      margin-top: 0.35rem;
      font-size: 0.8rem;
      text-transform: uppercase;
      letter-spacing: 0.12em;
    }
    .diag-ok { color: #34d399; }
    .diag-warn { color: #fbbf24; }
    .diag-error { color: #f87171; }
    .empty-state { margin: 0; color: rgba(244, 229, 205, 0.68); font-style: italic; }
    .automation-card {
      margin-top: 1rem;
      border: 1px solid rgba(52, 211, 153, 0.22);
      border-radius: 1.1rem;
      padding: 1rem;
      background: rgba(52, 211, 153, 0.06);
    }
    .trace-list {
      display: grid;
      gap: 0.7rem;
      margin-top: 0.9rem;
    }
    .trace-item {
      border-radius: 0.95rem;
      border: 1px solid rgba(244, 229, 205, 0.08);
      background: rgba(14, 20, 18, 0.42);
      padding: 0.8rem 0.9rem;
    }
    .trace-item__top {
      display: flex;
      justify-content: space-between;
      gap: 0.75rem;
      align-items: center;
      flex-wrap: wrap;
    }
    .trace-item p {
      margin: 0.45rem 0 0;
      color: rgba(244, 229, 205, 0.76);
      line-height: 1.55;
    }
    .trace-item small {
      display: block;
      margin-top: 0.3rem;
      color: rgba(244, 229, 205, 0.58);
      overflow-wrap: anywhere;
    }
    .import-modal {
      position: fixed;
      inset: 0;
      z-index: 100;
      display: grid;
      place-items: center;
      background: rgba(8, 5, 3, 0.7);
      backdrop-filter: blur(6px);
      padding: 1rem;
    }
    .import-card {
      width: min(34rem, 100%);
      border-radius: 1.6rem;
      border: 1px solid rgba(244,229,205,0.15);
      background: #15100a;
      padding: 1.5rem;
      box-shadow: 0 30px 80px rgba(10,6,3,0.5);
    }
    .import-card h3 { margin: 0 0 0.5rem; font-size: 1.2rem; }
    .import-note { color: rgba(244, 229, 205, 0.72); line-height: 1.6; margin: 0 0 1rem; font-size: 0.9rem; }
    .import-actions { display: flex; gap: 0.6rem; justify-content: flex-end; margin-top: 1rem; flex-wrap: wrap; }
    @media (max-width: 1100px) {
      .browser-session-banner { grid-template-columns: 1fr; }
      .metrics-grid, .provider-status-grid { grid-template-columns: 1fr; }
      .panel-card, .panel-card--wide, .panel-card--span-7, .panel-card--span-5, .panel-card--span-12 { grid-column: span 12; }
    }
    @media (max-width: 760px) {
      .resume-shell, .results-shell { padding: 1rem; border-radius: 1.5rem; }
      .form-grid, .action-row, .mode-switch, .metrics-grid { grid-template-columns: 1fr; }
      .generate-btn--inline { width: 100%; }
    }
  `],
})
export class ResumeComponent implements AfterViewInit {
  private readonly resumeService = inject(ResumeService);
  private readonly profileService = inject(ProfileService);
  private readonly sanitizer = inject(DomSanitizer);
  readonly apiBase = "/api";

  step: Step = "profile";
  mode: SourceMode = "upload";

  providers: Array<{ id: string; default_model: string }> = [];
  providerStatuses: Array<{ id: string; status: string; reason?: string; model?: string; default_model?: string }> = [];
  templates: Array<{ id: string; name: string }> = [
    { id: "clean-slate", name: "Clean Slate" },
    { id: "graphite", name: "Graphite" },
    { id: "signal", name: "Signal" },
    { id: "north", name: "North" },
    { id: "baseline", name: "Baseline" },
  ];
  languages: Array<{ code: string; name: string }> = [
    { code: "en", name: "English" },
    { code: "pl", name: "Polish" },
    { code: "ru", name: "Russian" },
  ];
  availableSources = [
    { id: "linkedin", name: "LinkedIn" },
    { id: "justjoin", name: "JustJoinIT" },
    { id: "nofluffjobs", name: "No Fluff Jobs" },
    { id: "remoteok", name: "RemoteOK" },
  ];
  selectedSources = ["linkedin", "justjoin", "nofluffjobs", "remoteok"];

  selectedFile: File | null = null;
  selectedFileName = "";
  discoveryStatus = "";
  statusText = "";

  discovering = false;
  generating = false;
  openingAutomation = false;
  progress = 0;

  discoverResult: DiscoverJobsResponse | null = null;
  selectedJob: DiscoveredJob | null = null;
  result: TailorResponse | null = null;
  automationResult: ApplyJobResponse | null = null;
  activeDiagnostics: Record<string, { status: string; detail: string; [key: string]: unknown }> = {};
  browserSession: { provider?: string; session?: string; status?: string; detail?: string } | null = null;

  previewMode: "html" | "pdf" = "html";
  trustedHtmlUrl: SafeResourceUrl | null = null;
  trustedPdfUrl: SafeResourceUrl | null = null;

  form = {
    fullName: "",
    email: "",
    phone: "",
    location: "",
    github: "",
    linkedin: "",
    website: "",
    summary: "",
    jobUrl: "",
    jobTitle: "",
    companyName: "",
    jobDescription: "",
    template: "signal",
    provider: "lmstudio",
    model: "qwythos-9b-v2",
    language: "auto",
  };

  ngAfterViewInit(): void {
    this.hydrateFromSavedProfile();

    this.resumeService.getTemplates().subscribe({
      next: (response) => {
        if (response.templates?.length) this.templates = response.templates;
      },
    });
    this.resumeService.getLanguages().subscribe({
      next: (response) => {
        if (response.languages?.length) this.languages = response.languages;
      },
    });
    this.resumeService.getProviders().subscribe({
      next: (response) => {
        this.providers = response.providers || [];
        const current = this.providers.find((item) => item.id === this.form.provider);
        if (current && !this.form.model.trim()) {
          this.form.model = current.default_model;
        }
      },
    });
    this.resumeService.getProviderStatuses().subscribe({
      next: (response) => {
        this.providerStatuses = response.providers || [];
      },
    });

    gsap.from(".resume-shell, .results-shell, .panel-card, .mode-card", {
      y: 24,
      opacity: 0,
      duration: 0.55,
      stagger: 0.05,
      ease: "power3.out",
    });
  }

  setMode(mode: SourceMode): void {
    this.mode = mode;
    this.result = null;
    this.discoverResult = null;
    this.selectedJob = null;
    this.step = "profile";
  }

  onFileSelected(event: Event): void {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0] || null;
    this.selectedFile = file;
    this.selectedFileName = file?.name || "";
  }

  canDiscover(): boolean {
    if (this.mode === "upload") {
      return !!this.selectedFile;
    }
    return !!this.form.fullName.trim() && !!this.form.summary.trim();
  }

  canOpenJobsStep(): boolean {
    return !!this.discoverResult?.jobs?.length;
  }

  toggleSource(sourceId: string): void {
    if (this.selectedSources.includes(sourceId)) {
      if (this.selectedSources.length > 1) {
        this.selectedSources = this.selectedSources.filter((item) => item !== sourceId);
      }
      return;
    }
    this.selectedSources = [...this.selectedSources, sourceId];
  }

  discoverJobs(): void {
    if (!this.canDiscover()) return;

    this.discovering = true;
    this.discoveryStatus = "Extracting your profile and searching vacancies across selected sources...";
    this.discoverResult = null;
    this.selectedJob = null;
    this.result = null;
    this.automationResult = null;

    this.resumeService.discoverJobs(this.buildPayload(), 12, this.selectedSources).subscribe({
      next: (response) => {
        this.discovering = false;
        this.discoverResult = response;
        this.activeDiagnostics = response.agent_diagnostics || {};
        this.discoveryStatus = response.jobs.length
          ? `Found ${response.jobs.length} matching vacancies.`
          : "No matching vacancies were found.";
        this.step = "jobs";
      },
      error: () => {
        this.discovering = false;
        this.discoveryStatus = "Vacancy search failed. Check backend availability and selected providers.";
      },
    });
  }

  selectJob(job: DiscoveredJob): void {
    this.selectedJob = job;
    this.form.jobUrl = job.url;
    this.form.jobTitle = job.title;
    this.form.companyName = job.company;
    this.form.jobDescription = job.description;
    this.result = null;
    this.automationResult = null;
    this.step = "package";
  }

  generatePackage(): void {
    if (!this.selectedJob) return;

    this.generating = true;
    this.progress = 8;
    this.statusText = "Collecting profile context and company information...";
    this.result = null;
    this.automationResult = null;

    const progressTimer = setInterval(() => {
      this.progress = Math.min(this.progress + 11, 88);
      if (this.progress >= 24) this.statusText = "Enriching with GitHub and website data...";
      if (this.progress >= 48) this.statusText = "Researching company and choosing output language...";
      if (this.progress >= 68) this.statusText = "Generating tailored CV, cover letter and ATS-ready output...";
    }, 320);

    this.resumeService.tailorResume(this.buildPayload()).subscribe({
      next: (response) => {
        clearInterval(progressTimer);
        this.generating = false;
        this.result = response;
        this.activeDiagnostics = response.agent_diagnostics || {};
        this.previewMode = "html";
        this.trustedHtmlUrl = this.sanitizer.bypassSecurityTrustResourceUrl(`${this.apiBase}${response.html_url}`);
        this.trustedPdfUrl = this.sanitizer.bypassSecurityTrustResourceUrl(`${this.apiBase}${response.pdf_url}`);
        this.progress = 100;
        this.statusText = "CV package generated successfully.";
        setTimeout(() => {
          document.getElementById("preview-document")?.scrollIntoView({ behavior: "smooth", block: "start" });
        }, 180);
      },
      error: () => {
        clearInterval(progressTimer);
        this.generating = false;
        this.progress = 0;
        this.statusText = "Package generation failed. Check API/provider availability.";
      },
    });
  }

  openAutomationPreview(): void {
    if (!this.result || !this.selectedJob) return;

    this.openingAutomation = true;
    this.automationResult = null;
    this.statusText = "Opening the vacancy in the automated browser and pre-filling the form...";

    this.resumeService.applyToJob({
      job_url: this.selectedJob.url,
      full_name: this.result.resume.full_name || this.form.fullName,
      email: this.form.email,
      phone: this.form.phone,
      cover_letter: this.result.cover_letter,
      provider: this.form.provider,
      model: this.form.model,
      auto_submit: false,
    }).subscribe({
      next: (response) => {
        this.openingAutomation = false;
        this.automationResult = response;
        this.browserSession = response.browser_session || this.browserSession;
        this.statusText = response.success
          ? (response.message || "The browser session is ready for manual review and submit.")
          : (response.error || "Automation preview failed.");
      },
      error: () => {
        this.openingAutomation = false;
        this.statusText = "Automation preview failed. Check agent-browser or Chromium availability.";
      },
    });
  }

  diagnosticsEntries(diagnostics: Record<string, { status: string; detail: string }>): Array<{ key: string; value: { status: string; detail: string } }> {
    return Object.entries(diagnostics || {}).map(([key, value]) => ({
      key,
      value,
    }));
  }

  syncProviderModel(providerId: string): void {
    const provider = this.providers.find((item) => item.id === providerId);
    if (provider) {
      this.form.model = provider.default_model;
    }
  }

  private buildPayload() {
    return {
      mode: this.mode,
      resumeFile: this.selectedFile,
      fullName: this.form.fullName,
      email: this.form.email,
      phone: this.form.phone,
      location: this.form.location,
      github: this.form.github,
      linkedin: this.form.linkedin,
      website: this.form.website,
      summary: this.form.summary,
      jobUrl: this.form.jobUrl,
      jobTitle: this.form.jobTitle,
      companyName: this.form.companyName,
      jobDescription: this.form.jobDescription,
      template: this.form.template,
      provider: this.form.provider,
      model: this.form.model,
      language: this.form.language,
    };
  }

  private hydrateFromSavedProfile(): void {
    const profile = this.profileService.load();
    this.applyImportedProfile(profile);
  }

  private applyImportedProfile(profile: UserProfile): void {
    this.form.fullName = resolveProfileName(profile.fullName) || resolveProfileName(this.form.fullName);
    this.form.email = profile.email || this.form.email;
    this.form.phone = profile.phone || this.form.phone;
    this.form.location = profile.location || this.form.location;
    this.form.github = profile.github || this.form.github;
    this.form.linkedin = profile.linkedin || this.form.linkedin;
    this.form.website = profile.website || this.form.website;
    if (profile.summary) {
      this.form.summary = profile.summary;
    } else if (profile.skills.length) {
      this.form.summary = profile.skills.join(", ");
    }
  }
}
