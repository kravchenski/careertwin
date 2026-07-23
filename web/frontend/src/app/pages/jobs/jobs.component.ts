import { Component, inject, NgZone, OnInit } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";
import { RouterModule } from "@angular/router";
import { HttpClient } from "@angular/common/http";
import { forkJoin, of } from "rxjs";
import { catchError } from "rxjs/operators";
import { StudioService, StudioState } from "../../services/studio.service";
import { ProfileService } from "../../services/profile.service";

interface CollectedJob {
  id: string;
  source: string;
  url: string;
  title: string;
  company: string;
  location: string;
  description: string;
  match_score: number;
  matching_skills: string[];
  missing_skills: string[];
  collected_at: string;
  status: "new" | "applied" | "skipped" | "failed";
  score?: number;
  score_reason?: string;
  employment_type?: string;
}

interface Company {
  id: number;
  name: string;
  description: string;
  website: string;
  careers: string;
  internships: string;
  certifications: string;
  learning: string;
  github: string;
  linkedin: string;
  glassdoor: string;
  wellfound: string;
  ats_platform: string;
}

interface JobSearchPayload {
  source: string;
  query: string;
  location: string;
  experience_level: string;
  work_type: string;
  posted_within: string;
  limit: number;
  intern: boolean;
  companies: string[];
}

type TabKind = "collect" | "review" | "history";

@Component({
  selector: "ct-jobs",
  standalone: true,
  imports: [CommonModule, FormsModule, RouterModule],
  template: `
    <div class="jc page-shell">
      <header class="jc__head">
        <div class="jc__head-row">
          <h1 class="jc__title">Jobs</h1>
          <div class="jc__head-stats">
            <span>{{ collectedCount }} jobs collected</span>
            <span class="jc__dot">&middot;</span>
            <span>{{ appliedCount }} applied</span>
            <span class="jc__dot">&middot;</span>
            <span>{{ pendingCount }} pending</span>
          </div>
        </div>
      </header>

      <nav class="jc__tabs">
        <button class="jc__tab" [class.jc__tab--active]="activeTab === 'collect'" (click)="activeTab = 'collect'">
          <span class="jc__tab-num">1</span>
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/></svg>
          Collect
        </button>
        <div class="jc__tab-line"></div>
        <button class="jc__tab" [class.jc__tab--active]="activeTab === 'review'" (click)="activeTab = 'review'">
          <span class="jc__tab-num">2</span>
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2"><polygon points="5 3 19 12 5 21 5 3"/></svg>
          Review & Apply
        </button>
        <div class="jc__tab-line"></div>
        <button class="jc__tab" [class.jc__tab--active]="activeTab === 'history'" (click)="activeTab = 'history'">
          <span class="jc__tab-num">3</span>
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>
          History
        </button>
      </nav>

      <!-- ==================== COLLECT TAB ==================== -->
      <div *ngIf="activeTab === 'collect'" class="jc__panel">
        <div class="jc__collector">
          <h2 class="jc__panel-title">Job Collector</h2>
          <p class="jc__panel-desc">
            Select companies and start collecting matching jobs from their career pages.
          </p>

          <h3 class="jc__field-label">Target Companies</h3>
          <div class="jc__company-section">
            <p class="jc__company-note">
              Searching all {{ dbCompanies.length }} companies by default. Add one below if it's missing.
            </p>
            <div class="jc__company-add-row">
              <input
                [(ngModel)]="newCompanyName"
                class="jc__input"
                placeholder="Add company name (e.g. 'Stripe')"
              />
              <input
                [(ngModel)]="newCompanyUrl"
                class="jc__input"
                placeholder="Career page URL (optional)"
              />
              <button
                class="jc__start-btn"
                [disabled]="!newCompanyName.trim() || addingCompany"
                (click)="addCompany()"
              >
                {{ addingCompany ? 'Adding...' : '+ Add' }}
              </button>
            </div>
          </div>

          <p class="jc__target-note" *ngIf="targetJobTitles.length">
            Searching for <strong>{{ targetJobTitles.join(' + ') }}</strong> internships/entry roles from your profile.
          </p>

          <div class="jc__form-grid">
            <label class="jc__field">
              <span class="jc__field-label">Posted within</span>
              <select [(ngModel)]="searchPayload.posted_within" class="jc__select">
                <option value="any">Any time</option>
                <option value="day">Past 24 hours</option>
                <option value="week">Past week</option>
                <option value="month">Past month</option>
              </select>
            </label>
            <label class="jc__field">
              <span class="jc__field-label">Work type</span>
              <select [(ngModel)]="searchPayload.work_type" class="jc__select">
                <option value="internship">Internship</option>
                <option value="any">Any</option>
                <option value="part-time">Part-time</option>
                <option value="contract">Contract</option>
              </select>
            </label>
            <label class="jc__field">
              <span class="jc__field-label">Job type</span>
              <select [(ngModel)]="searchPayload.location" class="jc__select">
                <option value="any">Any</option>
                <option value="remote">Remote</option>
                <option value="onsite">On-site</option>
                <option value="hybrid">Hybrid</option>
              </select>
            </label>
          </div>

          <div class="jc__search-row">
            <button
              class="jc__start-btn jc__start-btn--full"
              [disabled]="collecting || !companiesLoaded"
              (click)="startCollecting()"
            >
              <svg *ngIf="!collecting && companiesLoaded" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2"><polygon points="5 3 19 12 5 21 5 3"/></svg>
              <span *ngIf="!companiesLoaded">Loading companies...</span>
              <span *ngIf="companiesLoaded && !collecting">Start</span>
              <span *ngIf="companiesLoaded && collecting">Stop</span>
            </button>
          </div>
        </div>

        <!-- Collection progress -->
        <div class="jc__progress" *ngIf="collecting">
          <div class="jc__progress-head">
            <span>Collected {{ collectedDuringRun }} of {{ searchPayload.limit }} jobs</span>
            <span>{{ collectPercent }}%</span>
          </div>
          <div class="jc__progress-bar">
            <div class="jc__progress-fill" [style.width.%]="collectPercent"></div>
          </div>

          <div class="jc__log">
            <div class="jc__log-head">
              <span>&gt;_ Collection Log</span>
              <span class="jc__log-spinner" *ngIf="collecting"></span>
            </div>
            <pre class="jc__log-body">{{ logText }}</pre>
          </div>
        </div>

        <!-- Screenshot preview -->
        <div class="jc__screenshots" *ngIf="screenshots.length">
          <div class="jc__shot" *ngFor="let ss of screenshots">
            <img [src]="ss" [alt]="ss" />
          </div>
        </div>
      </div>

      <!-- ==================== REVIEW TAB ==================== -->
      <div *ngIf="activeTab === 'review'" class="jc__panel">
        <div class="jc__generating-banner" *ngIf="generatingId">
          <span class="jc__log-spinner"></span>
          Generating your application — cover letter and tailored CV can take a couple of minutes.
        </div>
        <div class="jc__generating-banner jc__generating-banner--error" *ngIf="generationFailed">
          {{ generationFailed }}
          <button class="jc__banner-dismiss" (click)="generationFailed = null">&times;</button>
        </div>
        <div class="jc__empty" *ngIf="!jobs.length">
          <svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="currentColor" stroke-width="1.5" style="color:#d1a157">
            <circle cx="11" r="8"/><path d="m21 21-4.35-4.35"/>
          </svg>
          <h3>No jobs collected yet</h3>
          <p>Go to the Collect tab and start gathering matching jobs.</p>
          <button class="jc__empty-btn" (click)="activeTab = 'collect'">Go to Collector</button>
        </div>

        <div class="jc__review-filters" *ngIf="jobs.length">
          <input
            [(ngModel)]="reviewSearchQuery"
            class="jc__input jc__input--search"
            placeholder="Search by title, company, location..."
          />
          <select [(ngModel)]="reviewCityFilter" class="jc__select jc__select--city">
            <option value="all">All cities</option>
            <option *ngFor="let city of availableCities" [value]="city">{{ city }}</option>
          </select>
          <span class="jc__review-count">{{ filteredJobs.length }} of {{ jobs.length }} jobs</span>
        </div>

        <div class="jc__job-list" *ngIf="jobs.length">
          <div class="jc__job-card" *ngFor="let job of paginatedJobs; let i = index" [class.jc__job-card--applied]="job.status === 'applied'" [class.jc__job-card--expanded]="expandedJobId === job.id">
            <div class="jc__job-header-row" (click)="toggleJobExpand(job.id)">
              <span class="jc__job-arrow" [class.jc__job-arrow--open]="expandedJobId === job.id">&#9654;</span>
              <img
                [src]="getCompanyLogo(job.company)"
                [alt]="job.company"
                class="jc__job-logo"
                (error)="onLogoError($event)"
              />
              <div class="jc__job-info">
                <h3 class="jc__job-title">{{ cleanTitle(job.title) }}</h3>
                <p class="jc__job-company">{{ job.company }}<span *ngIf="job.location"> · {{ shortLocation(job.location) }}</span></p>
              </div>
              <span *ngIf="job.employment_type" class="jc__job-type-badge jc__job-type-badge--header" [ngClass]="'type-' + job.employment_type">{{ job.employment_type }}</span>
            </div>

            <div class="jc__job-expanded" *ngIf="expandedJobId === job.id">
              <div class="jc__job-content-row">
                <div class="jc__job-desc-wrap">
                  <div *ngIf="job.description && cleanDescription(job.description)" class="jc__job-desc-structured">
                    <div *ngIf="getMetadata(cleanDescription(job.description)).length" class="jc__desc-meta">
                      <span *ngFor="let m of getMetadata(cleanDescription(job.description))" class="jc__desc-meta-tag">
                        <span class="jc__desc-meta-key">{{ m.key }}</span>{{ m.value }}
                      </span>
                    </div>
                    <ng-container *ngFor="let section of parseDescription(cleanDescription(job.description), job.title)">
                      <h4 *ngIf="section.title" class="jc__desc-section-title" [class]="'jc__desc-section-title--' + sectionIcon(section.title)">
                        <span class="jc__desc-icon"></span>{{ section.title }}
                      </h4>
                      <div *ngIf="section.items.length && isSkillsSection(section.title, section.items)" class="jc__desc-chips">
                        <span class="jc__desc-chip" *ngFor="let item of section.items">{{ item }}</span>
                      </div>
                      <ul *ngIf="section.items.length && !isSkillsSection(section.title, section.items)" class="jc__desc-list">
                        <li *ngFor="let item of section.items">{{ item }}</li>
                      </ul>
                      <p *ngIf="!section.items.length && section.text" class="jc__desc-text">{{ section.text }}</p>
                    </ng-container>
                  </div>
                  <p class="jc__job-desc jc__job-desc--empty" *ngIf="!job.description || !cleanDescription(job.description)">
                    <a [href]="job.url" target="_blank" rel="noreferrer" class="jc__job-link">View full description on company site</a>
                  </p>
                </div>
              </div>

              <div class="jc__job-actions">
                <a class="jc__apply-btn jc__apply-btn--outline" [href]="job.url" target="_blank" rel="noreferrer" (click)="$event.stopPropagation()">View listing</a>
                <button
                  class="jc__apply-btn"
                  [disabled]="generatingId === job.id"
                  (click)="generateApplication(job); $event.stopPropagation()"
                >
                  {{ generatingId === job.id ? "Generating..." : "Generate Application" }}
                </button>
              </div>
            </div>
          </div>
        </div>

        <div class="jc__pagination" *ngIf="totalPages > 1">
          <button class="jc__page-btn" [disabled]="currentPage === 1" (click)="goToPage(currentPage - 1)">&#8249;</button>
          <span class="jc__page-status">Page {{ currentPage }} of {{ totalPages }}</span>
          <button class="jc__page-btn" [disabled]="currentPage === totalPages" (click)="goToPage(currentPage + 1)">&#8250;</button>
        </div>
      </div>

      <!-- ==================== HISTORY TAB ==================== -->
      <div *ngIf="activeTab === 'history'" class="jc__panel">
        <div class="jc__empty" *ngIf="!appliedJobs.length">
          <svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="currentColor" stroke-width="1.5" style="color:#d1a157">
            <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/>
          </svg>
          <h3>No applications yet</h3>
          <p>Jobs you apply to will appear here with status tracking.</p>
        </div>

        <div class="jc__job-list" *ngIf="appliedJobs.length">
          <div class="jc__job-card jc__job-card--history" *ngFor="let job of appliedJobs">
            <div class="jc__job-header">
              <div class="jc__job-info">
                <h3 class="jc__job-title">{{ job.title }}</h3>
                <p class="jc__job-company">{{ job.company }}</p>
              </div>
              <span class="jc__status-badge" [class]="'jc__status-badge--' + job.status">{{ job.status }}</span>
            </div>
            <p class="jc__job-desc">Applied {{ job.collected_at | date:"medium" }}</p>
          </div>
        </div>
      </div>

      <!-- ==================== APPLICATION MODAL ==================== -->
      <div class="jc__modal-overlay" *ngIf="generatedApplication" (click)="closeApplicationModal()">
        <div class="jc__modal" (click)="$event.stopPropagation()">
          <div class="jc__modal-header">
            <h2>Generated application</h2>
            <button class="jc__modal-close" (click)="closeApplicationModal()">&times;</button>
          </div>
          <div class="jc__modal-body">
            <div class="jc__modal-section" *ngIf="!generatedApplication.cover_letter">
              <p class="jc__modal-empty">The AI didn't return usable content this time — the model may have run out of room to think, or LM Studio isn't running. Try again, or give the model more time.</p>
            </div>
            <div class="jc__modal-section" *ngIf="generatedApplication.cover_letter">
              <h3>Cover letter</h3>
              <pre class="jc__modal-text">{{ generatedApplication.cover_letter }}</pre>
            </div>
            <div class="jc__modal-section" *ngIf="applicationAnswers.length">
              <h3>Application questions</h3>
              <dl class="jc__answer-list">
                <ng-container *ngFor="let a of applicationAnswers">
                  <dt>{{ a.label }}</dt>
                  <dd>{{ a.value }}</dd>
                </ng-container>
              </dl>
            </div>
            <div class="jc__modal-section" *ngIf="generatedApplication.resume_highlights?.length">
              <h3>Resume highlights</h3>
              <ul>
                <li *ngFor="let h of generatedApplication.resume_highlights">{{ h }}</li>
              </ul>
            </div>
            <div class="jc__modal-section jc__approve-status" *ngIf="approveStatus">
              <p [class.jc__approve-status--error]="approveError">{{ approveStatus }}</p>
            </div>
          </div>
          <div class="jc__modal-footer">
            <a *ngIf="generatedApplication.pdf_url" [href]="generatedApplication.pdf_url" target="_blank" class="jc__apply-btn jc__apply-btn--outline">Download cover letter</a>
            <a *ngIf="generatedApplication.resume_pdf_url" [href]="generatedApplication.resume_pdf_url" target="_blank" class="jc__apply-btn jc__apply-btn--outline">Download tailored CV</a>
            <button class="jc__apply-btn jc__apply-btn--outline" (click)="copyApplication()">Copy to Clipboard</button>
            <button
              class="jc__apply-btn jc__apply-btn--approve"
              [disabled]="!generatedApplication.cover_letter || approving"
              (click)="approveAndApply()"
            >
              {{ approving ? "Filling application..." : "Approve & auto-fill" }}
            </button>
          </div>
        </div>
      </div>
    </div>
  `,
  styles: [`
    .jc { display: grid; gap: 1.2rem; padding-bottom: 4rem; }

    .jc__head-row { display: flex; align-items: baseline; gap: 1rem; flex-wrap: wrap; }
    .jc__title {
      font-family: var(--font-display); font-size: clamp(2rem, 4vw, 2.8rem);
      color: #f4e5cd; margin: 0;
    }
    .jc__head-stats { font-size: 0.88rem; color: #baa690; }
    .jc__dot { margin: 0 0.2rem; }

    .jc__tabs {
      display: flex; align-items: center; gap: 0;
      padding: 0.4rem; border-radius: 1rem;
      background: rgba(255,255,255,0.03); border: 1px solid rgba(209,161,87,0.1);
    }
    .jc__tab {
      display: flex; align-items: center; gap: 0.45rem; padding: 0.65rem 1.1rem;
      border-radius: 0.75rem; border: none; background: transparent;
      color: #baa690; font: inherit; font-size: 0.88rem; font-weight: 500;
      cursor: pointer; transition: all 160ms; white-space: nowrap;
    }
    .jc__tab:hover { color: #f4e5cd; background: rgba(255,255,255,0.04); }
    .jc__tab--active { color: #f4e5cd; background: rgba(209,161,87,0.14); font-weight: 600; }
    .jc__tab-num {
      width: 1.3rem; height: 1.3rem; border-radius: 50%; font-size: 0.72rem; font-weight: 700;
      display: grid; place-items: center;
      border: 1px solid rgba(209,161,87,0.25); color: #baa690;
    }
    .jc__tab--active .jc__tab-num { border-color: #d1a157; color: #d1a157; }
    .jc__tab-line {
      width: 1px; height: 1.2rem; background: rgba(209,161,87,0.15); margin: 0 0.3rem;
    }

    .jc__panel {
      padding: 1.5rem; border: 1px solid rgba(209,161,87,0.12);
      border-radius: 1.5rem; background: rgba(255,255,255,0.03);
    }
    .jc__panel-title {
      font-family: var(--font-display); font-size: 1.5rem; color: #f4e5cd; margin: 0 0 0.4rem;
    }
    .jc__panel-desc { color: #baa690; line-height: 1.65; margin: 0 0 1.2rem; font-size: 0.92rem; }

    .jc__login-note {
      padding: 0.9rem 1rem; border-radius: 0.8rem; margin-bottom: 1.2rem;
      background: rgba(209,161,87,0.06); border: 1px solid rgba(209,161,87,0.12);
      color: #c4b49e; font-size: 0.88rem; line-height: 1.6;
    }
    .jc__login-note strong { color: #d1a157; }

    .jc__field-label {
      font-size: 0.78rem; text-transform: uppercase; letter-spacing: 0.1em;
      color: #d1a157; font-weight: 600; margin-bottom: 0.5rem; display: block;
    }

    .jc__source-chips { display: flex; flex-wrap: wrap; gap: 0.5rem; margin-bottom: 1.2rem; }
    .jc__source-chip {
      padding: 0.55rem 1rem; border-radius: 999px; font: inherit; font-size: 0.88rem;
      border: 1px solid rgba(209,161,87,0.2); background: rgba(255,255,255,0.04);
      color: #f4e5cd; cursor: pointer; transition: all 150ms;
    }
    .jc__source-chip:hover { border-color: rgba(209,161,87,0.4); background: rgba(209,161,87,0.06); }
    .jc__source-chip--active {
      border-color: #d1a157; background: rgba(209,161,87,0.18); color: #f5df90; font-weight: 600;
    }

    .jc__company-section { margin-bottom: 1.2rem; }
    .jc__company-note { color: #baa690; font-size: 0.85rem; margin: 0 0 0.8rem; }
    .jc__company-add-row { display: flex; gap: 0.5rem; }
    .jc__company-add-row .jc__input { flex: 1; }

    .jc__target-note {
      margin: 0 0 1rem; padding: 0.7rem 1rem; border-radius: 0.8rem;
      background: rgba(209,161,87,0.06); border: 1px solid rgba(209,161,87,0.15);
      color: #c4b49e; font-size: 0.85rem;
    }
    .jc__target-note strong { color: #f5df90; }

    .jc__form-grid {
      display: grid; grid-template-columns: repeat(3, 1fr); gap: 0.8rem; margin-bottom: 1rem;
    }
    .jc__field { display: grid; gap: 0.35rem; }
    .jc__field--grow { flex: 1; }
    .jc__field--num { width: 6rem; }

    .jc__select, .jc__input {
      width: 100%; padding: 0.7rem 0.9rem; border-radius: 0.8rem;
      border: 1px solid rgba(209,161,87,0.2); background: rgba(255,255,255,0.05);
      color: #f4e5cd; font: inherit; outline: none; transition: border-color 150ms;
      color-scheme: dark;
    }
    .jc__select {
      -webkit-appearance: none; appearance: none;
      background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' fill='%23baa690'%3E%3Cpath d='M2 4l4 4 4-4'/%3E%3C/svg%3E");
      background-repeat: no-repeat;
      background-position: right 0.8rem center;
      padding-right: 2.2rem;
    }
    .jc__select option { background: #1a1210; color: #f4e5cd; }
    .jc__select:focus, .jc__input:focus { border-color: #d1a157; }
    .jc__input::placeholder { color: rgba(186,166,144,0.4); }

    .jc__search-row {
      display: flex; gap: 0.7rem; align-items: flex-end;
    }
    .jc__start-btn {
      display: flex; align-items: center; gap: 0.4rem; padding: 0.7rem 1.4rem;
      border-radius: 999px; font: inherit; font-weight: 700; font-size: 0.92rem;
      border: 1px solid rgba(209,161,87,0.3); background: rgba(209,161,87,0.12);
      color: #f5df90; cursor: pointer; transition: all 160ms; white-space: nowrap;
    }
    .jc__start-btn:hover:not(:disabled) { background: rgba(209,161,87,0.2); transform: translateY(-1px); }
    .jc__start-btn:disabled { opacity: 0.5; cursor: not-allowed; }
    .jc__start-btn--full { width: 100%; justify-content: center; }

    .jc__progress { margin-top: 1.2rem; }
    .jc__progress-head { display: flex; justify-content: space-between; font-size: 0.85rem; color: #baa690; margin-bottom: 0.5rem; }
    .jc__progress-bar { height: 6px; border-radius: 999px; background: rgba(209,161,87,0.12); overflow: hidden; }
    .jc__progress-fill { height: 100%; border-radius: 999px; background: linear-gradient(90deg, #d1a157, #f5df90); transition: width 400ms ease; }

    .jc__log {
      margin-top: 1rem; border-radius: 0.8rem; overflow: hidden;
      border: 1px solid rgba(209,161,87,0.1);
    }
    .jc__log-head {
      display: flex; align-items: center; gap: 0.5rem; padding: 0.6rem 0.9rem;
      background: rgba(255,255,255,0.04); font-size: 0.82rem; color: #baa690;
      border-bottom: 1px solid rgba(209,161,87,0.08);
    }
    .jc__log-spinner {
      width: 0.8rem; height: 0.8rem; border-radius: 50%;
      border: 1.5px solid rgba(209,161,87,0.2); border-top-color: #d1a157;
      animation: spin 0.8s linear infinite;
    }
    @keyframes spin { to { transform: rotate(360deg); } }

    .jc__generating-banner {
      display: flex; align-items: center; gap: 0.6rem; padding: 0.8rem 1rem;
      border-radius: 0.8rem; margin-bottom: 1rem; font-size: 0.88rem;
      background: rgba(209,161,87,0.08); border: 1px solid rgba(209,161,87,0.2); color: #f5df90;
    }
    .jc__generating-banner--error {
      background: rgba(248,113,113,0.08); border-color: rgba(248,113,113,0.25); color: #f87171;
      justify-content: space-between;
    }
    .jc__banner-dismiss {
      background: none; border: none; color: inherit; font-size: 1.2rem; cursor: pointer;
      padding: 0 0.3rem; line-height: 1;
    }

    .jc__log-body {
      padding: 0.9rem; margin: 0; font-family: "SF Mono", "Fira Code", monospace;
      font-size: 0.78rem; line-height: 1.65; color: #c4b49e;
      background: rgba(0,0,0,0.3); max-height: 22rem; overflow-y: auto;
      white-space: pre-wrap; word-break: break-word;
    }

    .jc__screenshots { display: flex; gap: 0.5rem; flex-wrap: wrap; margin-top: 1rem; }
    .jc__shot img { width: 120px; border-radius: 0.6rem; border: 1px solid rgba(209,161,87,0.15); }

    .jc__empty {
      display: flex; flex-direction: column; align-items: center; text-align: center;
      padding: 3rem 1rem; gap: 0.6rem;
    }
    .jc__empty h3 { color: #f4e5cd; margin: 0.5rem 0 0; font-size: 1.1rem; }
    .jc__empty p { color: #baa690; margin: 0; font-size: 0.92rem; }
    .jc__empty-btn {
      margin-top: 0.5rem; padding: 0.6rem 1.4rem; border-radius: 999px;
      border: 1px solid rgba(209,161,87,0.3); background: rgba(209,161,87,0.12);
      color: #f5df90; font: inherit; font-weight: 600; cursor: pointer;
    }
    .jc__empty-btn:hover { background: rgba(209,161,87,0.2); }

    .jc__review-filters { display: flex; gap: 0.6rem; align-items: center; margin-bottom: 1rem; }
    .jc__input--search { flex: 1; min-width: 0; }
    .jc__select--city { width: 180px; flex-shrink: 0; }
    .jc__review-count { font-size: 0.82rem; color: #baa690; white-space: nowrap; flex-shrink: 0; }

    .jc__pagination {
      display: flex; align-items: center; justify-content: center; gap: 1rem;
      margin-top: 1.2rem;
    }
    .jc__page-btn {
      width: 2.2rem; height: 2.2rem; border-radius: 50%; display: grid; place-items: center;
      border: 1px solid rgba(209,161,87,0.25); background: rgba(255,255,255,0.03);
      color: #f5df90; font-size: 1.1rem; cursor: pointer; transition: all 150ms;
    }
    .jc__page-btn:hover:not(:disabled) { border-color: rgba(209,161,87,0.5); background: rgba(209,161,87,0.08); }
    .jc__page-btn:disabled { opacity: 0.35; cursor: not-allowed; }
    .jc__page-status { font-size: 0.85rem; color: #baa690; }

    .jc__job-list { display: grid; gap: 0.6rem; }
    .jc__job-card {
      padding: 0; border-radius: 1rem;
      border: 1px solid rgba(209,161,87,0.08); background: #161210;
      transition: all 160ms; overflow: hidden;
    }
    .jc__job-card:hover { border-color: rgba(209,161,87,0.2); }
    .jc__job-card--applied { border-color: rgba(52,211,153,0.2); }
    .jc__job-card--history { padding: 0.9rem; }

    .jc__job-header-row {
      display: flex; align-items: center; gap: 0.8rem; padding: 1rem 1.2rem;
      cursor: pointer; transition: background 120ms;
    }
    .jc__job-header-row:hover { background: rgba(255,255,255,0.02); }

    .jc__job-arrow {
      font-size: 0.6rem; color: #6b5d4d; flex-shrink: 0;
      transition: transform 200ms ease; display: inline-block; width: 1rem; text-align: center;
    }
    .jc__job-arrow--open { transform: rotate(90deg); }

    .jc__job-logo {
      width: 40px; height: 40px; border-radius: 10px; flex-shrink: 0;
      object-fit: cover; background: rgba(255,255,255,0.05);
    }
    .jc__job-logo--fallback {
      display: flex; align-items: center; justify-content: center;
      font-weight: 700; font-size: 1rem; color: #fff;
    }

    .jc__job-info { flex: 1; min-width: 0; }
    .jc__job-title { margin: 0; font-size: 0.95rem; color: #f4e5cd; font-weight: 600; }
    .jc__job-company { margin: 0.15rem 0 0; color: #8b7d6b; font-size: 0.82rem; overflow-wrap: break-word; }

    .jc__job-expanded {
      padding: 0 1.2rem 1rem; border-top: 1px solid rgba(209,161,87,0.06);
    }

    .jc__job-score-row {
      display: flex; align-items: center; gap: 0.8rem; margin-top: 0.6rem;
    }
    .jc__job-content-row {
      display: flex; gap: 1rem; margin-top: 0.6rem; align-items: flex-start;
    }
    .jc__job-content-row .jc__job-desc-wrap { flex: 1; min-width: 0; margin-top: 0; }
    .jc__job-type-badge {
      display: inline-block; padding: 0.1rem 0.4rem; border-radius: 0.3rem;
      font-size: 0.65rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em;
      flex-shrink: 0;
    }
    .jc__job-type-badge--header { align-self: center; }
    .type-remote { background: rgba(52,211,153,0.12); border: 1px solid rgba(52,211,153,0.25); color: #6ee7b7; }
    .type-hybrid { background: rgba(251,191,36,0.12); border: 1px solid rgba(251,191,36,0.25); color: #fcd34d; }
    .type-onsite, .type-on-site { background: rgba(96,165,250,0.12); border: 1px solid rgba(96,165,250,0.25); color: #93c5fd; }

    .jc__job-desc-wrap { margin-top: 0.6rem; }
    .jc__job-desc { color: #baa690; margin: 0; line-height: 1.7; font-size: 0.85rem; white-space: pre-wrap; }
    .jc__job-desc--empty { color: #6b5d4d; font-style: italic; }
    .jc__job-desc-structured { font-size: 0.85rem; color: #baa690; line-height: 1.6; }
    .jc__desc-meta { display: flex; flex-wrap: wrap; gap: 0.4rem; margin-bottom: 0.8rem; }
    .jc__desc-meta-tag {
      display: inline-flex; align-items: center; gap: 0.3rem;
      padding: 0.3rem 0.6rem; border-radius: 999px; font-size: 0.78rem;
      background: rgba(209,161,87,0.08); border: 1px solid rgba(209,161,87,0.18); color: #d4c4a8;
    }
    .jc__desc-meta-key { color: #baa690; font-weight: 500; }

    .jc__desc-section-title {
      display: flex; align-items: center; gap: 0.45rem;
      color: #f5df90; font-size: 0.8rem; font-weight: 700; text-transform: uppercase;
      letter-spacing: 0.06em; margin: 1.1rem 0 0.5rem; padding-bottom: 0.35rem;
      border-bottom: 1px solid rgba(209,161,87,0.18);
    }
    .jc__desc-section-title:first-child { margin-top: 0; }
    .jc__desc-icon {
      display: inline-flex; align-items: center; justify-content: center;
      width: 1.15rem; height: 1.15rem; border-radius: 0.4rem; flex-shrink: 0;
      background: rgba(209,161,87,0.12); color: #d1a157; font-size: 0.72rem; font-weight: 700;
    }
    .jc__desc-section-title--tag .jc__desc-icon::before { content: '#'; }
    .jc__desc-section-title--check .jc__desc-icon::before { content: '✓'; }
    .jc__desc-section-title--list .jc__desc-icon::before { content: '›'; }
    .jc__desc-section-title--gift .jc__desc-icon::before { content: '★'; }
    .jc__desc-section-title--star .jc__desc-icon::before { content: '☆'; }
    .jc__desc-section-title--dot .jc__desc-icon::before { content: '•'; }

    .jc__desc-chips { display: flex; flex-wrap: wrap; gap: 0.4rem; margin: 0.2rem 0 0; }
    .jc__desc-chip {
      padding: 0.3rem 0.65rem; border-radius: 999px; font-size: 0.78rem; font-weight: 500;
      background: rgba(209,161,87,0.09); border: 1px solid rgba(209,161,87,0.22); color: #e8d4ae;
    }

    .jc__desc-list { margin: 0.2rem 0 0; padding: 0; list-style: none; }
    .jc__desc-list li {
      position: relative; padding-left: 1.1rem; margin-bottom: 0.4rem; color: #c4b49e; line-height: 1.6;
    }
    .jc__desc-list li::before {
      content: '✓'; position: absolute; left: 0; top: 0.05rem; color: #d1a157; font-weight: 700; font-size: 0.8rem;
    }
    .jc__desc-text { margin: 0.2rem 0; color: #baa690; line-height: 1.65; }

    .jc__job-actions { display: flex; gap: 0.6rem; align-items: center; margin-top: 0.8rem; }
    .jc__job-link { color: #f5df90; text-decoration: underline; font-size: 0.85rem; }
    .jc__apply-btn {
      padding: 0.45rem 0.9rem; border-radius: 999px; border: 1px solid rgba(209,161,87,0.3);
      background: rgba(209,161,87,0.12); color: #f5df90; font: inherit; font-weight: 600;
      font-size: 0.82rem; cursor: pointer; transition: all 150ms; text-decoration: none;
      display: inline-flex; align-items: center; gap: 0.4rem;
    }
    .jc__apply-btn:hover:not(:disabled) { background: rgba(209,161,87,0.2); }
    .jc__apply-btn:disabled { opacity: 0.5; cursor: not-allowed; }
    .jc__apply-btn--outline { background: transparent; border-color: rgba(209,161,87,0.5); }
    .jc__apply-btn--approve {
      background: #d1a157; border-color: #d1a157; color: #1a1210; margin-left: auto;
    }
    .jc__apply-btn--approve:hover:not(:disabled) { background: #f5df90; border-color: #f5df90; }

    .jc__approve-status p {
      color: #6ee7b7; font-size: 0.88rem; line-height: 1.6; margin: 0;
      padding: 0.8rem 1rem; border-radius: 0.6rem;
      background: rgba(52,211,153,0.08); border: 1px solid rgba(52,211,153,0.2);
    }
    .jc__approve-status p.jc__approve-status--error {
      color: #f87171; background: rgba(248,113,113,0.08); border-color: rgba(248,113,113,0.25);
    }

    .jc__status-badge {
      padding: 0.3rem 0.7rem; border-radius: 999px; font-size: 0.78rem; font-weight: 600;
      border: 1px solid rgba(209,161,87,0.15); background: rgba(255,255,255,0.04); color: #baa690;
    }
    .jc__status-badge--applied { border-color: rgba(52,211,153,0.3); color: #34d399; background: rgba(52,211,153,0.08); }
    .jc__status-badge--failed { border-color: rgba(248,113,113,0.3); color: #f87171; background: rgba(248,113,113,0.08); }

    .jc__modal-overlay {
      position: fixed; inset: 0; z-index: 1000;
      background: rgba(0,0,0,0.7); display: grid; place-items: center;
      padding: 1rem;
    }
    .jc__modal {
      background: #1a1210; border: 1px solid rgba(209,161,87,0.2);
      border-radius: 1.5rem; max-width: 700px; width: 100%; max-height: 85vh;
      display: flex; flex-direction: column; overflow: hidden;
    }
    .jc__modal-header {
      display: flex; align-items: center; justify-content: space-between;
      padding: 1.2rem 1.5rem; border-bottom: 1px solid rgba(209,161,87,0.1);
    }
    .jc__modal-header h2 { margin: 0; font-family: var(--font-display); color: #f4e5cd; font-size: 1.2rem; }
    .jc__modal-close {
      background: none; border: none; color: #baa690; font-size: 1.5rem; cursor: pointer;
      padding: 0.2rem 0.5rem; border-radius: 0.5rem;
    }
    .jc__modal-close:hover { color: #f4e5cd; background: rgba(255,255,255,0.05); }
    .jc__modal-body { padding: 1.5rem; overflow-y: auto; flex: 1; }
    .jc__modal-section { margin-bottom: 1.2rem; }
    .jc__modal-section h3 { margin: 0 0 0.5rem; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.08em; color: #d1a157; }
    .jc__modal-section p, .jc__modal-section li { color: #d4c4a8; line-height: 1.8; font-size: 0.95rem; font-family: "Times New Roman", Times, serif; }
    .jc__modal-section ul { padding-left: 1.2rem; }
    .jc__answer-list {
      display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr);
      row-gap: 0.7rem; column-gap: 1rem; margin: 0;
    }
    .jc__answer-list dt {
      color: #baa690; font-size: 0.82rem; font-weight: 600;
      align-self: center;
    }
    .jc__answer-list dd {
      margin: 0; color: #f4e5cd; font-size: 0.92rem; font-weight: 500; align-self: center;
    }
    @media (max-width: 520px) {
      .jc__answer-list { grid-template-columns: 1fr; row-gap: 0.2rem; }
      .jc__answer-list dd { margin-bottom: 0.5rem; }
    }
    .jc__modal-text {
      white-space: pre-wrap; word-break: break-word; margin: 0;
      font-family: "Times New Roman", Times, serif; font-size: 1rem; color: #d4c4a8; line-height: 1.8;
      background: rgba(0,0,0,0.15); padding: 1.2rem; border-radius: 0.6rem;
      border: 1px solid rgba(209,161,87,0.1);
    }
    .jc__modal-empty {
      color: #baa690; font-size: 0.88rem; text-align: center;
      padding: 1.5rem; background: rgba(0,0,0,0.15); border-radius: 0.6rem;
      border: 1px dashed rgba(209,161,87,0.2);
    }
    .jc__modal-footer {
      padding: 1rem 1.5rem; border-top: 1px solid rgba(209,161,87,0.1);
      display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 0.6rem;
    }

    @media (max-width: 720px) {
      .jc__form-grid { grid-template-columns: 1fr 1fr; }
      .jc__search-row { flex-direction: column; }
      .jc__field--num { width: 100%; }
    }
  `],
})
export class JobsComponent implements OnInit {
  private readonly http = inject(HttpClient);
  private readonly studio = inject(StudioService);
  private readonly ngZone = inject(NgZone);
  private readonly profileService = inject(ProfileService);
  readonly api = "/api/v1";

  activeTab: TabKind = "collect";
  state: StudioState | null = null;

  selectedSource = "linkedin";
  searchPayload: JobSearchPayload = {
    source: "linkedin",
    query: "",
    location: "any",
    experience_level: "internship",
    work_type: "internship",
    posted_within: "week",
    limit: 100,
    intern: true,
    companies: [],
  };

  collecting = false;
  collectedDuringRun = 0;
  collectPercent = 0;
  logText = "";
  screenshots: string[] = [];

  jobs: CollectedJob[] = [];
  generatingId = "";
  generationFailed: string | null = null;
  generatedApplication: any = null;
  generatedApplicationJob: CollectedJob | null = null;
  approving = false;
  approveStatus: string | null = null;
  approveError = false;
  expandedJobId: string | null = null;

  dbCompanies: Company[] = [];
  companiesLoaded = false;
  newCompanyName = "";
  newCompanyUrl = "";
  reviewSearchQuery = "";
  reviewCityFilter = "all";
  addingCompany = false;

  readonly sources = [
    { id: "all", name: "All Sources" },
    { id: "linkedin", name: "LinkedIn" },
  ];

  get collectedCount(): number { return this.jobs.length; }
  get appliedCount(): number { return this.jobs.filter((j) => j.status === "applied").length; }
  get pendingCount(): number { return this.jobs.filter((j) => j.status === "new").length; }

  get filteredJobs(): CollectedJob[] {
    let jobs = this.jobs.filter((j) => j.status !== "applied");

    if (this.reviewSearchQuery.trim()) {
      const q = this.reviewSearchQuery.toLowerCase();
      jobs = jobs.filter(j =>
        j.title.toLowerCase().includes(q) ||
        j.company.toLowerCase().includes(q) ||
        (j.location && j.location.toLowerCase().includes(q)) ||
        (j.description && j.description.toLowerCase().includes(q))
      );
    }

    if (this.reviewCityFilter && this.reviewCityFilter !== "all") {
      const city = this.normalizeCityKey(this.reviewCityFilter);
      jobs = jobs.filter(j => {
        const loc = this.normalizeCityKey(j.location || "");
        const title = this.normalizeCityKey(j.title || "");
        const isRemote = j.employment_type === "remote" || loc.includes("remote") || title.includes("remote");
        if (isRemote) return true;
        return loc.includes(city) || title.includes(city);
      });
    }

    return jobs;
  }

  readonly PAGE_SIZE = 20;
  currentPage = 1;

  get totalPages(): number {
    return Math.max(1, Math.ceil(this.filteredJobs.length / this.PAGE_SIZE));
  }

  get paginatedJobs(): CollectedJob[] {
    const all = this.filteredJobs;
    if (this.currentPage > this.totalPages) this.currentPage = this.totalPages;
    const start = (this.currentPage - 1) * this.PAGE_SIZE;
    return all.slice(start, start + this.PAGE_SIZE);
  }

  goToPage(page: number): void {
    this.currentPage = Math.min(Math.max(1, page), this.totalPages);
  }

  private normalizeCityKey(city: string): string {
    return city.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase().trim();
  }

  get availableCities(): string[] {
    const knownCities = ['Gdansk', 'Krakow', 'Warsaw', 'Wroclaw', 'Poznan', 'Lodz', 'Katowice',
      'Tashkent', 'Batumi', 'Tbilisi', 'Sofia', 'Remote'];
    const cities = new Map<string, string>();
    for (const c of this.state?.preferences?.onsiteCities || []) {
      if (c) cities.set(this.normalizeCityKey(c), c);
    }
    for (const j of this.jobs) {
      if (j.location) {
        const parts = j.location.split(/[,\/]/);
        if (parts.length > 0) {
          const name = parts[0].trim();
          const key = this.normalizeCityKey(name);
          if (name && !cities.has(key)) cities.set(key, name);
        }
      }
      for (const city of knownCities) {
        if (j.title.includes(city)) {
          const key = this.normalizeCityKey(city);
          if (!cities.has(key)) cities.set(key, city);
        }
      }
    }
    return Array.from(cities.values()).sort();
  }
  get appliedJobs(): CollectedJob[] { return this.jobs.filter((j) => j.status === "applied" || j.status === "failed"); }

  ngOnInit(): void {
    const local = this.studio.load();
    this.state = local;
    this.prefillSearch();
    this.loadCompanies();
    this.loadCollectedJobs();
    this.resumePendingApplication();

    if (this.state?.profile?.skills?.length) {
      this.http.post(`${this.api}/v1/profile`, {
        username: "default",
        full_name: this.state.profile.fullName || "",
        skills: this.state.profile.skills,
        summary: this.state.profile.summary || "",
      }).subscribe({ next: () => {}, error: () => {} });
    }

    this.studio.loadFromBackend().subscribe((remote) => {
      if (remote?.completedAt) {
        this.state = remote;
        this.prefillSearch();
      }
    });
  }

  private loadCompanies(): void {
    this.http.get<any>(`${this.api}/companies`).subscribe({
      next: (res) => {
        if (res?.companies) {
          this.dbCompanies = res.companies;
          // Search all companies by default — no manual selection step.
          this.searchPayload.companies = this.dbCompanies.map((c) => c.name);
        }
        this.companiesLoaded = true;
      },
      error: () => {
        this.companiesLoaded = true;
      },
    });
  }

  private loadCollectedJobs(): void {
    this.http.get<any>(`${this.api}/jobs/collected?limit=500`).subscribe({
      next: (res) => {
        if (res?.jobs) {
          for (const job of res.jobs) {
            if (!this.jobs.find((j) => j.url === job.url)) {
              this.jobs.push({
                id: job.id || crypto.randomUUID(),
                source: job.source || '',
                url: job.url || '',
                title: job.title || '',
                company: job.company || '',
                location: job.location || '',
                description: job.description || '',
                match_score: job.match_score || 0,
                matching_skills: job.matching_skills || [],
                missing_skills: job.missing_skills || [],
                collected_at: job.collected_at || '',
                status: job.status || 'new',
              });
            }
          }
        }
      },
      error: () => {},
    });
  }

  // CareerTwin targets students — the search is always scoped to
  // internships/entry roles regardless of onboarding answers, so there's no
  // "experience level" filter to expose in the UI.
  get targetJobTitles(): string[] {
    return this.state?.preferences?.jobTitles?.slice(0, 3) || [];
  }

  // The answers real application forms ask most often — pulled straight from
  // the candidate's own onboarding answers rather than AI-generated prose.
  get applicationAnswers(): { label: string; value: string }[] {
    const prefs = this.state?.preferences;
    if (!prefs) return [];
    const answers: { label: string; value: string }[] = [];

    if (prefs.workAuth?.length) {
      answers.push({
        label: "Work authorization",
        value: prefs.workAuth.map((w) => `${w.status} (${w.country})`).join(", "),
      });
    }
    if (prefs.willingToRelocate !== null && prefs.willingToRelocate !== undefined) {
      answers.push({ label: "Willing to relocate", value: prefs.willingToRelocate ? "Yes" : "No" });
    }
    if (prefs.startDate) {
      answers.push({ label: "Available start date", value: prefs.startDate });
    }
    if (prefs.salary?.amount) {
      answers.push({ label: "Salary expectation", value: `${prefs.salary.amount} ${prefs.salary.currency || ""}`.trim() });
    }
    if (prefs.workMode?.length) {
      answers.push({ label: "Work mode", value: prefs.workMode.join(", ") });
    }
    return answers;
  }

  private prefillSearch(): void {
    if (!this.state) return;
    this.searchPayload.experience_level = "internship";
    this.searchPayload.intern = true;
    if (this.state.preferences?.remoteLocations?.length) {
      this.searchPayload.location = "remote";
    }
    // Pre-set city filter from onboarding preferences — only for on-site
    // preferences. Remote work isn't tied to a city, so it stays "all".
    if (this.state.preferences?.onsiteCities?.length) {
      this.reviewCityFilter = this.state.preferences.onsiteCities[0];
    }
  }

  addCompany(): void {
    if (!this.newCompanyName.trim()) return;
    this.addingCompany = true;
    this.http.post<any>(`${this.api}/companies/add`, {
      name: this.newCompanyName.trim(),
      careers: this.newCompanyUrl.trim(),
    }).subscribe({
      next: (res) => {
        if (res?.company && !this.dbCompanies.find((c) => c.name === res.company.name)) {
          this.dbCompanies.push(res.company);
          this.searchPayload.companies.push(res.company.name);
        }
        this.newCompanyName = "";
        this.newCompanyUrl = "";
        this.addingCompany = false;
      },
      error: () => {
        this.addingCompany = false;
      },
    });
  }

  startCollecting(): void {
    if (this.collecting) {
      this.collecting = false;
      return;
    }

    // Company-based ATS search is the only supported path now — if the
    // companies list failed to load, don't silently fall back to the
    // legacy direct-scrape endpoint (no progress streaming, effectively dead).
    if (this.searchPayload.companies.length === 0) {
      this.logText = "Couldn't load the companies list — refresh the page and try again.";
      return;
    }

    this.collecting = true;
    this.collectedDuringRun = 0;
    this.collectPercent = 0;
    this.searchPayload.source = this.selectedSource;

    // Use MCP endpoint when companies are selected (ATS-native search)
    const useMCP = this.searchPayload.companies.length > 0;
    const endpoint = useMCP ? `${this.api}/jobs/mcp/search` : `${this.api}/jobs/collect`;

    // Mix all target job titles from the profile into one combined search
    // instead of only searching the first one.
    const titles = this.targetJobTitles.length ? this.targetJobTitles : [this.searchPayload.query];

    const buildPayload = (title: string) => useMCP
      ? {
          companies: this.searchPayload.companies,
          query: title,
          intern_only: this.searchPayload.intern,
          limit: this.searchPayload.limit,
        }
      : {
          username: "default",
          ...this.searchPayload,
          query: title,
        };

    // Clear existing jobs when searching specific companies
    if (useMCP) {
      this.jobs = [];
    }

    this.logText = useMCP
      ? `Searching ATS platforms for ${this.searchPayload.companies.length} companies (${titles.filter(Boolean).join(", ") || "all roles"})...\n`
      : `Starting collection on ${this.selectedSource === "all" ? "all sources" : this.selectedSource}...\n`;

    // Real progress instead of sitting at 0% until the whole batch resolves —
    // the backend streams a log line per company as its search completes.
    let collectSse: EventSource | null = null;
    if (useMCP) {
      collectSse = new EventSource(`${this.api}/jobs/scan/logs`);
      // EventSource callbacks fire outside Angular's zone, so updates here
      // never triggered change detection — the log/progress state was
      // updating in memory but the view never re-rendered.
      collectSse.onmessage = (event) => {
        this.ngZone.run(() => {
          try {
            const entry = JSON.parse(event.data);
            if (!entry?.msg) return;
            const match = /\((\d+)\/(\d+)\)$/.exec(entry.msg);
            if (match) {
              this.collectPercent = Math.round((Number(match[1]) / Number(match[2])) * 100);
            }
            this.logText += `\n${entry.msg}`;
          } catch {
            // ignore malformed/keepalive SSE frames
          }
        });
      };
    }

    const requests = titles.map((title) =>
      this.http.post<any>(endpoint, buildPayload(title)).pipe(catchError(() => of(null)))
    );

    forkJoin(requests).subscribe((results) => {
      collectSse?.close();
      let totalFound = 0;
      const sourcesQueried: string[] = [];
      const screenshotsFound: string[] = [];
      for (const res of results) {
        if (!res?.jobs) continue;
        for (const job of res.jobs) {
          if (!this.jobs.find((j) => j.url === job.url)) {
            this.jobs.push({
              id: crypto.randomUUID(),
              ...job,
              collected_at: new Date().toISOString(),
              status: "new",
            });
            totalFound++;
          }
        }
        if (res.sources_queried) sourcesQueried.push(...res.sources_queried);
        if (res.screenshots) screenshotsFound.push(...res.screenshots);
      }
      this.collectedDuringRun = totalFound;
      this.collectPercent = 100;
      const src = useMCP ? ` (${Array.from(new Set(sourcesQueried)).join(", ") || "ATS"})` : "";
      this.logText += `\nCollection complete. Found ${totalFound} matching jobs${src}.`;
      if (screenshotsFound.length) this.screenshots = screenshotsFound;
      if (totalFound > 0) this.activeTab = "review";
      this.collecting = false;
    });
  }

  stripHtml(html: string): string {
    if (!html) return '';
    let s = html;
    s = s.replace(/<br\s*\/?>/gi, '\n');
    s = s.replace(/<\/(p|div|li|h[1-6])>/gi, '\n');
    s = s.replace(/<li[^>]*>/gi, '\n• ');
    s = s.replace(/<[^>]+>/g, '');
    s = s.replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&nbsp;/g, ' ');
    return s.replace(/\n{3,}/g, '\n\n').trim();
  }

  // Skills/tech-stack sections read better as scannable chips than a bulleted
  // paragraph list — matches how the original job site usually shows them.
  // But a title merely containing the word "skills" (e.g. Deloitte's
  // "Nice-to-Have Skills", whose items are full descriptive sentences like
  // "Graph Databases: Experience with graph databases...") shouldn't force
  // chip styling — only actually short, tag-like items should render as
  // chips, regardless of what the section happens to be titled.
  isSkillsSection(title: string, items: string[] = []): boolean {
    if (!title) return false;
    const titleMatches = /skill|tech(nolog|nical)?\s*stack|tools?\b|technolog/i.test(title);
    if (!titleMatches) return false;
    if (!items.length) return true;
    return items.every(item => item.length <= 40 && !item.includes(': '));
  }

  sectionIcon(title: string): string {
    const t = (title || '').toLowerCase();
    if (this.isSkillsSection(t)) return 'tag';
    if (/requirement|qualif|who you are|bring/.test(t)) return 'check';
    if (/responsib|what you.?ll do|role/.test(t)) return 'list';
    if (/benefit|offer|perk|compensation|salary/.test(t)) return 'gift';
    if (/nice to have/.test(t)) return 'star';
    return 'dot';
  }

  parseDescription(desc: string, jobTitle: string = ''): { title: string; items: string[]; text: string }[] {
    if (!desc) return [];
    let text = desc.replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&nbsp;/g, ' ');

    const sectionHeaders = [
      'what you\'ll do', 'what you bring', 'what you\'ll get', 'what we offer',
      'responsibilities', 'requirements', 'qualifications', 'benefits',
      'nice to have', 'about the role', 'about us', 'who you are',
      'our tech stack', 'tech stack', 'skills', 'technologies', 'tools',
      'perks', 'compensation', 'salary',
      'job description', 'we are looking for', 'who we are looking for',
      'we offer', 'our values', 'your future role', 'future role',
      'about',
    ];

    // Remove junk sections
    const junkPatterns = [
      /contact our recruiter.*$/i, /apply for internship.*$/i,
      /company website.*$/i, /recruiter outreach.*$/i,
      /linkedin\s*email.*$/i, /my network.*$/i,
      /job boards.*$/i, /social media.*$/i, /events.*$/i,
      /university.*\/college.*$/i, /recommendations.*$/i,
    ];
    for (const pattern of junkPatterns) {
      text = text.replace(pattern, '');
    }

    const lines = text.split('\n').map(l => l.trim()).filter(l => l.length > 0);
    const sections: { title: string; items: string[]; text: string }[] = [];
    let currentTitle = '';
    let currentItems: string[] = [];
    let currentText: string[] = [];
    // Some ATS pages (e.g. EPAM) put the duplicate title, "Remote in <city>",
    // and bare category tags on their own separate leading lines before the
    // real body starts — keep skipping until real content begins.
    let inMetaHeader = true;
    let justSawWorkModeLine = false;

    for (const line of lines) {
      const cleanLine = line.replace(/^##\s*/, '').trim();

      if (inMetaHeader) {
        const isTitleDup = jobTitle && cleanLine.toLowerCase() === jobTitle.toLowerCase().trim();
        const isWorkModeLine = /^(remote|hybrid|on-?site)\s+in\s+/i.test(cleanLine);
        const isBareCategoryLine = justSawWorkModeLine
          && cleanLine.length <= 60
          && !/[.!?:]$/.test(cleanLine)
          && !sectionHeaders.some(h => cleanLine.toLowerCase().startsWith(h));

        if (isTitleDup || isWorkModeLine) {
          justSawWorkModeLine = isWorkModeLine;
          continue;
        }
        if (isBareCategoryLine) {
          justSawWorkModeLine = false;
          continue;
        }
        inMetaHeader = false;
      }

      // Non-letters become a space (not deleted) so a hyphenated header like
      // "Nice-to-Have" still normalizes to "nice to have" and matches the
      // vocabulary list below instead of collapsing into "nicetohave".
      const lower = cleanLine.toLowerCase().replace(/[^a-z\s']/g, ' ').replace(/\s+/g, ' ').trim();
      // Require a short line (a real header, not a full sentence that merely
      // starts with a header phrase like "we are looking for a candidate...").
      const isTitle = sectionHeaders.some(h => lower.startsWith(h)) && (cleanLine.length <= 30 || cleanLine.endsWith(':'));

      if (isTitle) {
        finalizeSection();
        currentTitle = cleanLine.replace(/:$/, '').trim();
        currentItems = [];
        currentText = [];
      } else if (line.startsWith('•') || line.startsWith('-') || line.startsWith('*') || line.match(/^\d+[\.\)]\s/)) {
        const item = line.replace(/^[•\-*]\s*/, '').replace(/^\d+[\.\)]\s*/, '').trim();
        if (item.length > 0) currentItems.push(item);
      } else if (cleanLine.length > 0) {
        currentText.push(cleanLine);
      }
    }
    finalizeSection();
    return sections;

    // Some ATS/career pages collapse list items into one run-on line with no
    // bullet markers or sentence punctuation at all (e.g. "Create and manage
    // cloud resources in AWS Support data ingestion processes..."). Detect
    // that shape via known bullet lead-in verbs and re-split it into a real
    // list instead of rendering it as an unreadable wall of text.
    function finalizeSection() {
      if (!currentTitle && !currentText.length && !currentItems.length) return;
      let items = currentItems.map(ensurePunctuation);
      let text = currentText.join(' ').trim();
      if (!items.length && text) {
        const implicit = splitImplicitList(text);
        if (implicit) {
          items = implicit;
          text = '';
        }
      }
      sections.push({ title: currentTitle, items, text });
    }

    function ensurePunctuation(item: string): string {
      const s = item.trim();
      if (!s || /[.!?:;]$/.test(s)) return s;
      return s + '.';
    }

    function splitImplicitList(text: string): string[] | null {
      const leadVerbs = [
        'Create', 'Build', 'Develop', 'Design', 'Implement', 'Maintain', 'Manage', 'Support', 'Assist',
        'Collaborate', 'Contribute', 'Help', 'Suggest', 'Engage', 'Own', 'Lead', 'Drive', 'Ensure', 'Work',
        'Coordinate', 'Perform', 'Provide', 'Monitor', 'Review', 'Write', 'Communicate', 'Test', 'Deploy',
        'Configure', 'Document', 'Research', 'Investigate', 'Identify', 'Optimize', 'Improve', 'Plan',
        'Prepare', 'Conduct', 'Execute', 'Handle', 'Utilize', 'Apply', 'Understand', 'Participate',
        'Analyze', 'Analyse', 'Define', 'Deliver', 'Partner', 'Mentor', 'Track', 'Troubleshoot',
        // Common requirement-list lead words (noun/adjective-led bullets,
        // e.g. "Educational background..." rather than an imperative verb).
        'Experience', 'Educational', 'Knowledge', 'Foundation', 'Understanding', 'Familiarity',
        'Willingness', 'Organized', 'Ability', 'Proficiency', 'Bachelor', 'Master', 'Fluency',
        'Comfortable', 'Passion', 'Interest', 'Degree', 'Familiar', 'Proven', 'Excellent', 'Strong',
        'Solid', 'Demonstrated', 'Good', 'Basic', 'Advanced', 'Prior',
      ];
      const verbAlt = leadVerbs.join('|');
      const boundaryRe = new RegExp(`\\s+(?=(?:${verbAlt})\\b)`, 'g');
      const countRe = new RegExp(`\\b(?:${verbAlt})\\b`, 'g');
      const matches = text.match(countRe);
      if (!matches || matches.length < 2) return null;
      const parts = text.split(boundaryRe).map(p => p.trim()).filter(Boolean);
      if (parts.length < 2) return null;
      return parts.map(ensurePunctuation);
    }
  }

  toSentenceCase(str: string): string {
    if (!str) return str;
    // If ALL CAPS, convert to sentence case
    if (str === str.toUpperCase() && str.length > 10) {
      return str.toLowerCase().replace(/(^|[.!?]\s+)([a-z])/g, (_, sep, c) => sep + c.toUpperCase());
    }
    return str;
  }

  getCompanyColor(name: string): string {
    const colors = ['#6366f1', '#8b5cf6', '#a855f7', '#d946ef', '#ec4899', '#f43f5e', '#ef4444', '#f97316', '#eab308', '#22c55e', '#14b8a6', '#06b6d4', '#3b82f6'];
    let hash = 0;
    for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash);
    return colors[Math.abs(hash) % colors.length];
  }

  cleanDescription(desc: string): string {
    if (!desc) return '';
    let s = desc;
    s = s.replace(/Apply\s*Interested\?/gi, '');
    s = s.replace(/Interested\?/gi, '');
    s = s.replace(/\d{2}D-\d{3}-\d{3}/g, '');
    s = s.replace(/\(Aleja[^)]*\)/g, '');
    s = s.replace(/\(ul\.[^)]*\)/g, '');
    s = s.replace(/(Internship|Full-time|Part-time|Contract|Temporary)\s*(Remote|Hybrid|On-site|Onsite)?\s*:/gi, ': ');
    if (s.length < 200 && !s.includes(' ') && s.length > 20) {
      return '';
    }
    return s.trim();
  }

  cleanTitle(title: string): string {
    if (!title) return '';
    const cities = ['Gdansk', 'Krakow', 'Warsaw', 'Wroclaw', 'Poznan', 'Lodz', 'Katowice',
      'Tashkent', 'Batumi', 'Tbilisi', 'Sofia', 'Remote', 'Berlin', 'Munich', 'Vienna', 'Prague',
      'Bucharest', 'Budapest', 'Tallinn', 'Riga', 'Vilnius', 'Zagreb', 'Ljubljana', 'Bratislava'];
    let result = title;
    for (const city of cities) {
      if (result.endsWith(city) && result.length > city.length + 2) {
        result = result.slice(0, -city.length) + ' · ' + city;
        break;
      }
    }
    return result;
  }

  getMetadata(desc: string): { key: string; value: string }[] {
    if (!desc) return [];
    const firstLine = desc.split('\n')[0].trim();
    if (!firstLine.startsWith('Location:')) return [];
    const parts = firstLine.split('·').map(p => p.trim());
    return parts.map(p => {
      const idx = p.indexOf(':');
      if (idx > 0) return { key: p.slice(0, idx + 1).trim(), value: p.slice(idx + 1).trim() };
      return { key: '', value: p };
    });
  }

  toggleJobExpand(id: string): void {
    if (this.expandedJobId === id) {
      this.expandedJobId = null;
      return;
    }
    this.expandedJobId = id;

    // The initial description that comes back from the company-search scrape
    // often carries site nav/contact-widget junk that never goes through the
    // crawl4ai cleanup pipeline. Always fetch the properly-cleaned version on
    // EVERY expand (not just the first) — the backend's cleanup logic keeps
    // improving, and a card expanded once during an earlier page session
    // must never be permanently stuck showing whatever it first fetched.
    const matchedJob = this.jobs.find(j => j.id === id);
    if (matchedJob) {
      const fetchUrl = matchedJob.url;
      const jobIdx = this.jobs.findIndex(j => j.id === id);
      this.http.post<any>(`${this.api}/jobs/mcp/fetch-description`, { url: fetchUrl, title: matchedJob.title }).subscribe({
        next: (res: any) => {
          if (jobIdx >= 0 && res?.description) {
            this.jobs[jobIdx].description = res.description;
          }
        },
        error: () => {},
      });
    }
  }

  shortLocation(location: string): string {
    if (!location) return '';
    const entries = location.split(';').map((s) => s.trim()).filter(Boolean);
    if (entries.length <= 2) return entries.join('; ');
    return `${entries.slice(0, 2).join('; ')} +${entries.length - 2} more`;
  }

  getCompanyLogo(company: string): string {
    const key = company.toLowerCase().replace(/[^a-z0-9]/g, '');
    const domainMap: Record<string, string> = {
      'figma': 'figma.com', 'google': 'google.com', 'meta': 'meta.com',
      'microsoft': 'microsoft.com', 'amazon': 'amazon.com', 'apple': 'apple.com',
      'netflix': 'netflix.com', 'spotify': 'spotify.com', 'stripe': 'stripe.com',
      'shopify': 'shopify.com', 'airbnb': 'airbnb.com', 'uber': 'uber.com',
      'slack': 'slack.com', 'twitter': 'x.com', 'github': 'github.com',
      'linkedin': 'linkedin.com', 'nvidia': 'nvidia.com', 'tesla': 'tesla.com',
      'adobe': 'adobe.com', 'salesforce': 'salesforce.com', 'ibm': 'ibm.com',
      'oracle': 'oracle.com', 'databricks': 'databricks.com', 'cloudflare': 'cloudflare.com',
      'canva': 'canva.com', 'dropbox': 'dropbox.com', 'mongodb': 'mongodb.com',
      'snowflake': 'snowflake.com', 'atlassian': 'atlassian.com',
      'adyen': 'adyen.com', 'coinbase': 'coinbase.com', 'pinterest': 'pinterest.com',
      'gitlab': 'gitlab.com', 'elastic': 'elastic.co', 'hashicorp': 'hashicorp.com',
      'docker': 'docker.com', 'digitalocean': 'digitalocean.com', 'discord': 'discord.com',
      'datadog': 'datadoghq.com', 'twilio': 'twilio.com', 'plaid': 'plaid.com',
      'rippling': 'rippling.com', 'robinhood': 'robinhood.com', 'snap': 'snap.com',
      'zoom': 'zoom.us', 'zscaler': 'zscaler.com', 'crowdstrike': 'crowdstrike.com',
      'servicenow': 'servicenow.com', 'okta': 'okta.com', 'docusign': 'docusign.com',
      'paypal': 'paypal.com', 'jpmorgan': 'jpmorganchase.com', 'goldmansachs': 'goldmansachs.com',
      'morganstanley': 'morganstanley.com', 'visa': 'visa.com', 'mastercard': 'mastercard.com',
      'walmart': 'walmart.com', 'target': 'target.com', 'cisco': 'cisco.com',
      'intel': 'intel.com', 'amd': 'amd.com', 'nike': 'nike.com',
      'samsung': 'samsung.com', 'sony': 'sony.com',
      'booking': 'booking.com', 'klarna': 'klarna.com',
      'revolut': 'revolut.com', 'n26': 'n26.com', 'wise': 'wise.com',
      'monzo': 'monzo.com', 'starling': 'starlingbank.com', 'hsbc': 'hsbc.com',
      'barclays': 'barclays.com', 'deezer': 'deezer.com', 'criteo': 'criteo.com',
      'amplitude': 'amplitude.com', 'hubspot': 'hubspot.com',
      'zendesk': 'zendesk.com', 'mailchimp': 'mailchimp.com',
      'intercom': 'intercom.com', 'drift': 'drift.com',
    };
    const domain = domainMap[key] || `${key}.com`;
    return `https://api.companyenrich.com/logo/${domain}`;
  }

  onLogoError(event: Event): void {
    const img = event.target as HTMLImageElement;
    img.style.display = 'none';
  }

  generateApplication(job: CollectedJob): void {
    this.generatingId = job.id;
    this.generationFailed = null;
    const profile = this.profileService.load();
    const payload = {
      firstName: this.state?.firstName || "",
      lastName: this.state?.lastName || "",
      email: profile.email || this.state?.profile?.email || "",
      title: profile.title || "",
      phone: profile.phone || this.state?.profile?.phone || "",
      location: profile.location || "",
      github: profile.github || this.state?.github || "",
      linkedin: profile.linkedin || "",
      website: profile.website || this.state?.website || "",
      summary: profile.summary || "",
      jobTitles: this.state?.preferences?.jobTitles || [],
      skills: profile.skills?.length ? profile.skills : (this.state?.profile?.skills || []),
      languages: profile.languages || [],
      experienceLevel: this.state?.preferences?.experienceLevel || "",
      onsiteCities: this.state?.preferences?.onsiteCities || [],
      remoteCities: this.state?.preferences?.remoteLocations || [],
      salary: this.state?.preferences?.salary || {},
      experience: profile.experience || [],
      education: profile.education || [],
      certificates: profile.certificates || [],
      jobs: [{ title: job.title, company: job.company, url: job.url, description: job.description || "" }],
      maxApplications: 1,
    };
    this.http.post<any>(`${this.api}/jobs/generate-application`, payload).subscribe({
      next: (res) => {
        if (res?.task_id) {
          localStorage.setItem(this.PENDING_APPLICATION_KEY, JSON.stringify({
            taskId: res.task_id,
            jobId: job.id,
            jobUrl: job.url,
            jobTitle: job.title,
            company: job.company,
          }));
          this.pollApplicationStatus(res.task_id, job);
        } else {
          this.generatingId = "";
        }
      },
      error: () => {
        this.generatingId = "";
      },
    });
  }

  private readonly PENDING_APPLICATION_KEY = "careertwin-pending-application";

  // AI generation can genuinely take a couple of minutes, but it must not
  // poll forever with zero feedback if the backend task never resolves.
  private static readonly MAX_POLL_ATTEMPTS = 180; // 180 * 5s = 15 minutes

  private pollApplicationStatus(taskId: string, job: CollectedJob, attempt: number = 1): void {
    this.http.get<any>(`${this.api}/jobs/generate-application/status/${taskId}`).subscribe({
      next: (res) => {
        if (res?.status === "done") {
          const app = res.result?.applications?.[0];
          if (app) {
            this.generatedApplication = app;
            this.generatedApplicationJob = job;
            this.approveStatus = null;
            this.approveError = false;
          }
          this.generatingId = "";
          localStorage.removeItem(this.PENDING_APPLICATION_KEY);
        } else if (res?.status === "error" || res?.status === "not_found") {
          this.generatingId = "";
          this.generationFailed = res?.status === "error"
            ? `Generation failed: ${res.error || "unknown error"}`
            : "Lost track of this generation (the server may have restarted). Please try again.";
          localStorage.removeItem(this.PENDING_APPLICATION_KEY);
        } else if (attempt >= JobsComponent.MAX_POLL_ATTEMPTS) {
          this.generatingId = "";
          this.generationFailed = "Generation is taking too long (over 15 minutes) — the AI provider (LM Studio) may be unresponsive. Try again once it's ready.";
          localStorage.removeItem(this.PENDING_APPLICATION_KEY);
        } else {
          setTimeout(() => this.pollApplicationStatus(taskId, job, attempt + 1), 5000);
        }
      },
      error: () => {
        if (attempt >= JobsComponent.MAX_POLL_ATTEMPTS) {
          this.generatingId = "";
          this.generationFailed = "Lost connection while checking generation status. Try again.";
          localStorage.removeItem(this.PENDING_APPLICATION_KEY);
          return;
        }
        setTimeout(() => this.pollApplicationStatus(taskId, job, attempt + 1), 5000);
      },
    });
  }

  // If the user reloads the page (or reopens the tab) while an application
  // is still being generated server-side, pick the poll back up instead of
  // losing track of it — the backend task keeps running regardless.
  private resumePendingApplication(): void {
    const raw = localStorage.getItem(this.PENDING_APPLICATION_KEY);
    if (!raw) return;
    try {
      const pending = JSON.parse(raw);
      if (!pending?.taskId) return;
      const job: CollectedJob = this.jobs.find((j) => j.url === pending.jobUrl) || {
        id: pending.jobId,
        source: "",
        url: pending.jobUrl,
        title: pending.jobTitle,
        company: pending.company,
        location: "",
        description: "",
        match_score: 0,
        matching_skills: [],
        missing_skills: [],
        collected_at: "",
        status: "new",
      };
      this.generatingId = job.id;
      this.pollApplicationStatus(pending.taskId, job);
    } catch {
      localStorage.removeItem(this.PENDING_APPLICATION_KEY);
    }
  }

  closeApplicationModal(): void {
    this.generatedApplication = null;
    this.generatedApplicationJob = null;
    this.approveStatus = null;
    this.approveError = false;
    this.approving = false;
  }

  approveAndApply(): void {
    if (!this.generatedApplicationJob || !this.generatedApplication?.cover_letter || this.approving) return;
    this.approving = true;
    this.approveStatus = null;
    this.approveError = false;
    const job = this.generatedApplicationJob;

    this.http.post<any>(`${this.api}/jobs/apply`, {
      job_url: job.url,
      username: "default",
      full_name: `${this.state?.firstName || ""} ${this.state?.lastName || ""}`.trim(),
      email: this.state?.profile?.email || "",
      phone: this.state?.profile?.phone || "",
      cover_letter: this.generatedApplication.cover_letter,
      auto_submit: false,
    }).subscribe({
      next: (res) => {
        this.approving = false;
        if (res?.success) {
          job.status = "applied";
          const filled = res.filled_fields?.length ? ` Filled: ${res.filled_fields.join(", ")}.` : "";
          this.approveStatus = `${res.message || "Application form filled."}${filled} Review and submit it in the opened browser session.`;
        } else {
          this.approveError = true;
          this.approveStatus = res?.error || "Couldn't auto-fill the application. Open the listing and apply manually.";
        }
      },
      error: (err) => {
        this.approving = false;
        this.approveError = true;
        this.approveStatus = err?.error?.detail || "Couldn't auto-fill the application. Open the listing and apply manually.";
      },
    });
  }

  copyApplication(): void {
    if (!this.generatedApplication) return;
    navigator.clipboard.writeText(this.generatedApplication.cover_letter || "");
  }
}
