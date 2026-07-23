import {
  Component,
  inject,
  OnInit,
  OnDestroy,
  ViewChild,
  ElementRef,
  ViewEncapsulation,
} from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";
import { Router } from "@angular/router";
import {
  StudioService,
  StudioState,
  StudioPreferences,
  WorkAuthEntry,
} from "../../services/studio.service";
import { ProfileService } from "../../services/profile.service";
import { resolveProfileName } from "../../utils/profile-name";

type StepKind =
  | "about"
  | "single"
  | "multi"
  | "titles"
  | "number"
  | "date"
  | "boolean"
  | "countries"
  | "cities"
  | "workauth"
  | "email-consent"
  | "loader";

interface Choice {
  value: string;
  title: string;
  desc?: string;
  emoji?: string;
}

interface Step {
  key: string;
  kind: StepKind;
  title: string;
  eyebrow?: string;
  subtitle?: string;
  hint?: string;
  options?: Choice[];
  placeholder?: string;
  showIf?: (s: StudioState) => boolean;
}

const EMPLOYMENT: Choice[] = [
  { value: "unemployed_need", title: "Unemployed and really need a job" },
  { value: "unemployed_caln", title: "Unemployed but not stressed about it" },
  { value: "badly_employed", title: "Badly employed and in need of a job switch" },
  { value: "employed_open", title: "Employed but open to greener pastures" },
];

const EXPERIENCE: Choice[] = [
  { value: "entry", title: "Entry-level", desc: "Just starting out" },
  { value: "first_role", title: "First full-time role in the field", desc: "Landing your first official role" },
  { value: "junior", title: "Junior/Associate", desc: "Works independently on well-scoped tasks" },
  { value: "mid", title: "Mid-level", desc: "Owns projects, may mentor others" },
  { value: "senior", title: "Senior/Lead", desc: "Leads complex work and sets direction" },
  { value: "director", title: "Director", desc: "Oversees teams/functions and drives strategy" },
  { value: "executive", title: "Executive", desc: "VP/C-level leadership and org-wide decisions" },
];

const INDUSTRIES: Choice[] = [
  { value: "it", emoji: "💻", title: "Information Technology & Telecommunications" },
  { value: "healthcare", emoji: "🏥", title: "Healthcare & Life Sciences" },
  { value: "finance", emoji: "💰", title: "Financial Services & Insurance" },
  { value: "sales", emoji: "📈", title: "Sales & Business Development" },
  { value: "marketing", emoji: "📣", title: "Marketing, Advertising & Public Relations" },
  { value: "retail", emoji: "🛒", title: "Retail & Wholesale Trade" },
  { value: "education", emoji: "🎓", title: "Education & Training" },
  { value: "hr", emoji: "👥", title: "Human Resources & Recruiting" },
  { value: "hospitality", emoji: "🏨", title: "Hospitality, Tourism & Leisure" },
  { value: "engineering", emoji: "⚙️", title: "Engineering & Manufacturing" },
  { value: "legal", emoji: "⚖️", title: "Legal & Compliance" },
  { value: "media", emoji: "🎬", title: "Media, Design & Creative" },
  { value: "government", emoji: "🏛️", title: "Government & Public Sector" },
  { value: "nonprofit", emoji: "🤝", title: "Nonprofit & NGO" },
  { value: "energy", emoji: "🔋", title: "Energy & Utilities" },
  { value: "science", emoji: "🔬", title: "Science & Research" },
  { value: "transport", emoji: "🚆", title: "Transport & Logistics" },
  { value: "construction", emoji: "🏗️", title: "Construction & Real Estate" },
  { value: "consulting", emoji: "💡", title: "Consulting & Professional Services" },
  { value: "automotive", emoji: "🚗", title: "Automotive & Aerospace" },
  { value: "agriculture", emoji: "🌾", title: "Agriculture & Environment" },
  { value: "telecom", emoji: "📡", title: "Telecommunications" },
];

const WORK_TYPES: Choice[] = [
  { value: "internship", title: "Internship" },
  { value: "full_time", title: "Full-time" },
  { value: "part_time", title: "Part-time" },
  { value: "contract", title: "Contract/Freelance" },
];

const WORK_MODE: Choice[] = [
  {
    value: "remote",
    title: "Full remote",
    desc: "Work from anywhere. We'll search for remote roles in the countries you pick next.",
    emoji: "🌐",
  },
  {
    value: "onsite",
    title: "On-site / Hybrid",
    desc: "We'll find office and hybrid roles in cities you'd commute to.",
    emoji: "🏙️",
  },
];

const REGIONS: Choice[] = [
  { value: "North America", emoji: "🌎", title: "North America" },
  { value: "South America", emoji: "🌎", title: "South America" },
  { value: "Europe", emoji: "🌍", title: "Europe" },
  { value: "Asia", emoji: "🌏", title: "Asia" },
  { value: "Oceania", emoji: "🌏", title: "Oceania" },
  { value: "Africa", emoji: "🌍", title: "Africa" },
  { value: "Worldwide", emoji: "🌐", title: "Worldwide" },
];

const COUNTRIES: Choice[] = [
  "Poland", "Germany", "United Kingdom", "United States", "Netherlands",
  "Sweden", "Norway", "France", "Spain", "Italy", "Ireland", "Denmark",
  "Finland", "Portugal", "Canada", "Australia", "Switzerland", "Austria",
  "Belgium", "Estonia", "Lithuania", "Latvia", "Czechia", "Romania",
].map((c) => ({ value: c, title: c }));

const WORK_AUTH_STATUS = [
  "Citizen", "Permanent Resident", "Authorized to Work",
  "Work Visa / Sponsorship", "Need Sponsorship", "Student Visa",
];

const YNM: Choice[] = [
  { value: "yes", title: "Yes" },
  { value: "no", title: "No" },
  { value: "prefer_not", title: "I do not wish to provide this information" },
];

const FLAGS: Record<string, string> = {
  Poland: "🇵🇱", Germany: "🇩🇪", "United Kingdom": "🇬🇧", "United States": "🇺🇸",
  Netherlands: "🇳🇱", Sweden: "🇸🇪", Norway: "🇳🇴", France: "🇫🇷", Spain: "🇪🇸",
  Italy: "🇮🇪", Ireland: "🇮🇪", Denmark: "🇩🇰", Finland: "🇫🇮", Portugal: "🇵🇹",
  Canada: "🇨🇦", Australia: "🇦🇺", Switzerland: "🇨🇭", Austria: "🇦🇹",
  Belgium: "🇧🇪", Estonia: "🇪🇪", Lithuania: "🇱🇹", Latvia: "🇱🇻", Czechia: "🇨🇿", Romania: "🇷🇴",
};

@Component({
  selector: "ct-studio",
  standalone: true,
  imports: [CommonModule, FormsModule],
  encapsulation: ViewEncapsulation.None,
  template: `
    <div class="s-root page-shell">
      <!-- ===================== WIZARD ===================== -->
      <ng-container *ngIf="mode === 'wizard'">
        <header class="s-head">
          <button class="s-back" type="button" (click)="exitToHome()" aria-label="Back to home">
            <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M15 18l-6-6 6-6" />
            </svg>
          </button>
          <div class="s-progress">
            <div class="s-progress-meta">
              <span class="s-eyebrow">CAREER TWIN STUDIO</span>
              <span class="s-step-count">Step {{ stepIndex + 1 }} / {{ visibleSteps.length }}</span>
            </div>
            <div class="s-progress-track">
              <div class="s-progress-fill" [style.width.%]="progressPercent"></div>
            </div>
          </div>
        </header>

        <section class="s-stage" #stage>
          <!-- ABOUT YOU -->
          <div class="s-card" *ngIf="current.kind === 'about'">
            <span class="s-eyebrow">{{ current.eyebrow }}</span>
            <h1 class="s-title">{{ current.title }}</h1>
            <p class="s-sub" *ngIf="current.subtitle">{{ current.subtitle }}</p>
            <div class="s-grid">
              <label class="s-field">
                <span>First name</span>
                <input [(ngModel)]="state.firstName" placeholder="John" />
              </label>
              <label class="s-field">
                <span>Last name</span>
                <input [(ngModel)]="state.lastName" placeholder="Doe" />
              </label>
              <label class="s-field">
                <span>GitHub <em>(optional)</em></span>
                <input [(ngModel)]="state.github" placeholder="https://github.com/username" />
                <small class="s-field-error" *ngIf="githubError">{{ githubError }}</small>
              </label>
              <label class="s-field">
                <span>Website / Portfolio <em>(optional)</em></span>
                <input [(ngModel)]="state.website" placeholder="https://your-site.dev" />
              </label>
            </div>
            <p class="s-hint" *ngIf="current.hint">{{ current.hint }}</p>
          </div>

          <!-- SINGLE CHOICE -->
          <div class="s-card" *ngIf="current.kind === 'single'">
            <span class="s-eyebrow">{{ current.eyebrow }}</span>
            <h1 class="s-title">{{ current.title }}</h1>
            <p class="s-sub" *ngIf="current.subtitle">{{ current.subtitle }}</p>
            <div class="s-choice-list">
              <button
                type="button"
                class="s-choice"
                *ngFor="let opt of current.options"
                [class.s-choice--active]="getStr(current.key) === opt.value"
                (click)="setStr(current.key, opt.value)"
              >
                <span class="s-choice-marker"></span>
                <span class="s-choice-body">
                  <span class="s-choice-title">{{ opt.title }}</span>
                  <span class="s-choice-desc" *ngIf="opt.desc">{{ opt.desc }}</span>
                </span>
              </button>
            </div>
          </div>

          <!-- MULTI CHOICE -->
          <div class="s-card" *ngIf="current.kind === 'multi'">
            <span class="s-eyebrow">{{ current.eyebrow }}</span>
            <h1 class="s-title">{{ current.title }}</h1>
            <p class="s-sub" *ngIf="current.subtitle">{{ current.subtitle }}</p>
            <div class="s-any-row" *ngIf="current.key === 'workTypes'">
              <button type="button" class="s-choice s-choice--box" [class.s-choice--active]="state.preferences.workTypes.length === 3" (click)="toggleAllWorkTypes()">
                <span class="s-choice-title">Any (Select All)</span>
              </button>
            </div>
            <div class="s-choice-grid">
              <button
                type="button"
                class="s-choice s-choice--box"
                *ngFor="let opt of current.options"
                [class.s-choice--active]="hasStr(current.key, opt.value)"
                (click)="toggleStr(current.key, opt.value)"
              >
                <span class="s-choice-emoji" *ngIf="opt.emoji">{{ opt.emoji }}</span>
                <span class="s-choice-title">{{ opt.title }}</span>
                <span class="s-choice-desc" *ngIf="opt.desc">{{ opt.desc }}</span>
              </button>
            </div>
          </div>

          <!-- JOB TITLES -->
          <div class="s-card" *ngIf="current.kind === 'titles'">
            <span class="s-eyebrow">{{ current.eyebrow }}</span>
            <h1 class="s-title">{{ current.title }}</h1>
            <p class="s-sub" *ngIf="current.subtitle">{{ current.subtitle }}</p>
            <div class="s-autocomplete">
              <input
                class="s-input"
                [ngModel]="titlesText"
                (ngModelChange)="onTitlesInput($event)"
                (focus)="titlesFocused=true"
                (blur)="onTitlesBlur()"
                placeholder="Start typing a job title..."
              />
              <div class="s-dropdown" *ngIf="titlesFocused && filteredTitles.length">
                <button
                  type="button"
                  class="s-dropdown-item"
                  *ngFor="let t of filteredTitles"
                  (mousedown)="pickTitle(t)"
                >{{ t }}</button>
              </div>
            </div>
            <div class="s-chips" *ngIf="state.preferences.jobTitles.length">
              <span class="s-chip" *ngFor="let t of state.preferences.jobTitles; let i = index">{{ t }}
                <button class="s-chip-x" (click)="removeTitle(i)">×</button>
              </span>
            </div>
            <p class="s-warn" *ngIf="titlesWarning">{{ titlesWarning }}</p>
            <p class="s-hint" *ngIf="current.hint && !titlesWarning">{{ current.hint }}</p>
          </div>

          <!-- NUMBER / SALARY -->
          <div class="s-card" *ngIf="current.kind === 'number'">
            <span class="s-eyebrow">{{ current.eyebrow }}</span>
            <h1 class="s-title">{{ current.title }}</h1>
            <p class="s-sub" *ngIf="current.subtitle">{{ current.subtitle }}</p>
            <div class="s-salary">
              <div class="s-salary-display">
                <span class="s-salary-cur">{{ state.preferences.salary.currency }}</span>
                <span class="s-salary-amount">{{ state.preferences.salary.amount | number }}</span>
              </div>
              <input
                type="range"
                min="60000"
                max="400000"
                step="5000"
                [(ngModel)]="state.preferences.salary.amount"
                class="s-salary-range"
              />
              <div class="s-salary-row">
                <select [(ngModel)]="state.preferences.salary.currency" class="s-salary-sel">
                  <option>PLN</option>
                  <option>USD</option>
                  <option>EUR</option>
                  <option>GBP</option>
                </select>
              </div>
            </div>
            <p class="s-hint" *ngIf="current.hint">{{ current.hint }}</p>
          </div>

          <!-- DATE -->
          <div class="s-card" *ngIf="current.kind === 'date'">
            <span class="s-eyebrow">{{ current.eyebrow }}</span>
            <h1 class="s-title">{{ current.title }}</h1>
            <p class="s-sub" *ngIf="current.subtitle">{{ current.subtitle }}</p>
            <input type="date" class="s-date" [(ngModel)]="state.preferences.startDate" />
            <label class="s-check">
              <input type="checkbox" [ngModel]="asapChecked" (ngModelChange)="onAsap($event)" />
              <span>asap</span>
            </label>
          </div>

          <!-- BOOLEAN -->
          <div class="s-card" *ngIf="current.kind === 'boolean'">
            <span class="s-eyebrow">{{ current.eyebrow }}</span>
            <h1 class="s-title">{{ current.title }}</h1>
            <p class="s-sub" *ngIf="current.subtitle">{{ current.subtitle }}</p>
            <div class="s-choice-list">
              <button type="button" class="s-choice" [class.s-choice--active]="getBool(current.key) === true" (click)="setBool(current.key, true)">
                <span class="s-choice-marker"></span>
                <span class="s-choice-body"><span class="s-choice-title">Yes</span></span>
              </button>
              <button type="button" class="s-choice" [class.s-choice--active]="getBool(current.key) === false" (click)="setBool(current.key, false)">
                <span class="s-choice-marker"></span>
                <span class="s-choice-body"><span class="s-choice-title">No</span></span>
              </button>
            </div>
          </div>

          <!-- COUNTRIES (remote) -->
          <div class="s-card" *ngIf="current.kind === 'countries'">
            <span class="s-eyebrow">{{ current.eyebrow }}</span>
            <h1 class="s-title">{{ current.title }}</h1>
            <p class="s-sub" *ngIf="current.subtitle">{{ current.subtitle }}</p>
            <div class="s-quick-row">
              <button
                type="button"
                class="s-quick-chip"
                *ngFor="let r of regions"
                [class.s-quick-chip--active]="hasStr(current.key, r.value)"
                (click)="toggleStr(current.key, r.value)"
              >{{ r.emoji }} {{ r.title }}</button>
            </div>
            <input
              class="s-input"
              [(ngModel)]="countryQuery"
              (ngModelChange)="filterCountries()"
              placeholder="Type to search country..."
            />
            <div class="s-choice-grid s-choice-grid--sm" *ngIf="filteredCountries.length">
              <button
                type="button"
                class="s-choice s-choice--box"
                *ngFor="let c of filteredCountries"
                [class.s-choice--active]="hasStr(current.key, c.value)"
                (click)="toggleStr(current.key, c.value)"
              ><span class="s-choice-title">{{ c.title }}</span></button>
            </div>
            <div class="s-chips" *ngIf="getStrArray(current.key).length">
              <span class="s-chip" *ngFor="let c of getStrArray(current.key)">{{ c }}
                <button class="s-chip-x" (click)="toggleStr(current.key, c)">×</button>
              </span>
            </div>
          </div>

          <!-- CITIES (onsite) -->
          <div class="s-card" *ngIf="current.kind === 'cities'">
            <span class="s-eyebrow">{{ current.eyebrow }}</span>
            <h1 class="s-title">{{ current.title }}</h1>
            <p class="s-sub" *ngIf="current.subtitle">{{ current.subtitle }}</p>
            <div class="s-autocomplete">
              <input
                class="s-input"
                [(ngModel)]="cityQuery"
                (ngModelChange)="onCityInput($event)"
                (keyup.enter)="addCity()"
                (focus)="cityFocused=true"
                (blur)="onCityBlur()"
                placeholder="Type a city name..."
              />
              <div class="s-dropdown" *ngIf="cityFocused && filteredCities.length">
                <button
                  type="button"
                  class="s-dropdown-item"
                  *ngFor="let c of filteredCities"
                  (mousedown)="pickCity(c)"
                >{{ c }}</button>
              </div>
            </div>
            <div class="s-chips" *ngIf="getStrArray(current.key).length">
              <span class="s-chip" *ngFor="let c of getStrArray(current.key)">{{ c }}
                <button class="s-chip-x" (click)="removeCity(c)">×</button>
              </span>
            </div>
            <p class="s-hint" *ngIf="current.hint">{{ current.hint }}</p>
          </div>

          <!-- WORK AUTH -->
          <div class="s-card" *ngIf="current.kind === 'workauth'">
            <span class="s-eyebrow">{{ current.eyebrow }}</span>
            <h1 class="s-title">{{ current.title }}</h1>
            <p class="s-sub" *ngIf="current.subtitle">{{ current.subtitle }}</p>
            <div class="s-auth-list">
              <div class="s-auth-row" *ngFor="let entry of state.preferences.workAuth; let i = index">
                <span class="s-auth-flag">{{ getFlag(entry.country) }}</span>
                <span class="s-auth-country">{{ entry.country }}</span>
                <select [(ngModel)]="entry.status" class="s-auth-select">
                  <option value="">Select status</option>
                  <option *ngFor="let s of workAuthStatus" [value]="s">{{ s }}</option>
                </select>
                <button class="s-auth-remove" (click)="removeAuth(i)">×</button>
              </div>
            </div>
            <div class="s-add-row">
              <select [(ngModel)]="authCountry" class="s-auth-select">
                <option value="">Add a country...</option>
                <option *ngFor="let c of countries" [value]="c.value">{{ c.title }}</option>
              </select>
              <button class="s-add-btn" type="button" (click)="addAuth()">+ Add</button>
            </div>
          </div>

          <!-- EMAIL CONSENT -->
          <div class="s-card" *ngIf="current.kind === 'email-consent'">
            <span class="s-eyebrow">{{ current.eyebrow }}</span>
            <h1 class="s-title">{{ current.title }}</h1>
            <p class="s-sub" *ngIf="current.subtitle">{{ current.subtitle }}</p>
            <label class="s-field s-field--full">
              <span>Email</span>
              <input [(ngModel)]="state.preferences.contactEmail" placeholder="example@gmail.com" />
            </label>
          </div>

          <!-- LOADER -->
          <div class="s-card s-card--loader" *ngIf="current.kind === 'loader'">
            <div class="s-loader-orb"></div>
            <h1 class="s-title s-title--center">{{ current.title }}</h1>
            <p class="s-sub s-sub--center" *ngIf="current.subtitle">{{ current.subtitle }}</p>
            <div class="s-loader-bar">
              <div class="s-loader-fill" [style.width.%]="progress"></div>
            </div>
            <ul class="s-gather-log">
              <li *ngFor="let line of gatherLog" [class.s-done]="line.done">
                <span class="s-gather-dot"></span>{{ line.text }}
              </li>
            </ul>
            <p class="s-loader-success" *ngIf="progress === 100">
              Successfully updated — taking you to your dashboard...
            </p>
          </div>
        </section>

        <footer class="s-foot" *ngIf="current.kind !== 'loader'">
          <button class="s-foot-btn s-foot-btn--ghost" type="button" [disabled]="stepIndex === 0" (click)="back()">
            Back
          </button>
          <button class="s-foot-btn s-foot-btn--primary" type="button" [disabled]="!canProceed()" (click)="next()">
            {{ isLast() ? "Finish" : "Continue" }}
          </button>
        </footer>
      </ng-container>

      <!-- ===================== SUMMARY ===================== -->
      <ng-container *ngIf="mode === 'summary'">
        <div class="s-summary">
          <div class="s-summary-hero">
            <div class="s-summary-avatar">{{ initials() }}</div>
            <div class="s-summary-id">
              <h1>{{ state.profile.fullName || "Your Twin" }}</h1>
              <p>{{ state.profile.title || summaryTitles() }}</p>
              <div class="s-summary-meta">
                <span *ngIf="state.profile.location">{{ state.profile.location }}</span>
                <span *ngIf="state.profile.email">{{ state.profile.email }}</span>
                <span *ngIf="state.preferences.contactEmail">{{ state.preferences.contactEmail }}</span>
              </div>
            </div>
            <button class="s-edit-btn" type="button" (click)="retake()">Edit Studio</button>
            <button class="s-edit-btn" type="button" (click)="exportProfile()">Export</button>
            <button class="s-edit-btn" type="button" (click)="fileInput.click()">Import</button>
            <input #fileInput type="file" accept="application/json,.json" hidden (change)="importProfile($event)" />
          </div>
          <p class="s-warn" *ngIf="importError">{{ importError }}</p>

          <div class="s-summary-stats">
            <div class="s-stat"><b>{{ state.profile.experience.length }}</b><span>Experience</span></div>
            <div class="s-stat"><b>{{ state.profile.education.length }}</b><span>Education</span></div>
            <div class="s-stat"><b>{{ state.profile.skills.length }}</b><span>Skills</span></div>
            <div class="s-stat"><b>{{ state.preferences.jobTitles.length }}</b><span>Target roles</span></div>
          </div>

          <section class="s-panel">
            <h3>Preferences</h3>
            <dl class="s-kv">
              <div><dt>Employment</dt><dd>{{ labelFor('employmentStatus', state.preferences.employmentStatus) }}</dd></div>
              <div><dt>Target titles</dt><dd>{{ state.preferences.jobTitles.join(", ") || "—" }}</dd></div>
              <div><dt>Experience</dt><dd>{{ labelFor('experienceLevel', state.preferences.experienceLevel) }}</dd></div>
              <div><dt>Salary</dt><dd>{{ state.preferences.salary.currency }} {{ state.preferences.salary.amount | number }}</dd></div>
              <div><dt>Industries</dt><dd>{{ labelsFor('industries', state.preferences.industries) }}</dd></div>
              <div><dt>Work type</dt><dd>{{ labelsFor('workTypes', state.preferences.workTypes) }}</dd></div>
              <div><dt>Start</dt><dd>{{ state.preferences.startDate || "—" }}</dd></div>
              <div><dt>Relocate</dt><dd>{{ state.preferences.willingToRelocate === null ? "—" : (state.preferences.willingToRelocate ? "Yes" : "No") }}</dd></div>
              <div><dt>Work mode</dt><dd>{{ labelsFor('workMode', state.preferences.workMode) }}</dd></div>
              <div><dt>Remote</dt><dd>{{ state.preferences.remoteLocations.join(", ") || "—" }}</dd></div>
              <div><dt>On-site</dt><dd>{{ state.preferences.onsiteCities.join(", ") || "—" }}</dd></div>
              <div><dt>Work auth</dt><dd>{{ workAuthText() }}</dd></div>
            </dl>
          </section>

          <section class="s-panel" *ngIf="state.profile.summary">
            <h3>Summary</h3>
            <p class="s-plain">{{ state.profile.summary }}</p>
          </section>

          <section class="s-panel" *ngIf="state.profile.skills.length">
            <h3>Skills</h3>
            <div class="s-chips">
              <span class="s-chip" *ngFor="let s of state.profile.skills">{{ s }}</span>
            </div>
          </section>

          <section class="s-panel s-panel--wide" *ngIf="state.profile.experience.length">
            <h3>Experience</h3>
            <div class="s-item" *ngFor="let it of state.profile.experience">
              <div class="s-item-title">{{ it.role }} · {{ it.company }}</div>
              <div class="s-item-sub">{{ it.start }} – {{ it.end || "Present" }}</div>
              <p class="s-plain">{{ it.description }}</p>
            </div>
          </section>

          <section class="s-panel s-panel--wide" *ngIf="state.profile.education.length">
            <h3>Education</h3>
            <div class="s-item" *ngFor="let it of state.profile.education">
              <div class="s-item-title">{{ it.school }}</div>
              <div class="s-item-sub">{{ it.degree }} · {{ it.field }}</div>
            </div>
          </section>
        </div>
      </ng-container>
    </div>
  `,
  styles: [],
})
export class StudioComponent implements OnInit {
  private readonly studio = inject(StudioService);
  private readonly profileService = inject(ProfileService);
  private readonly router = inject(Router);

  state: StudioState = this.studio.emptyState();
  mode: "wizard" | "summary" = "wizard";
  stepIndex = 0;
  submitting = false;
  progress = 0;
  gatherLog: { text: string; done: boolean }[] = [];
  scanLogs: { msg: string; level: string; time: string }[] = [];
  scanConnecting = false;
  @ViewChild("consoleBody") consoleBody?: ElementRef;
  private scanSse: EventSource | null = null;

  countryQuery = "";
  filteredCountries = COUNTRIES;
  cityQuery = "";
  authCountry = "";
  titlesText = "";
  titlesWarning = "";
  importError: string | null = null;
  githubError = "";
  titlesFocused = false;
  filteredTitles: string[] = [];
  cityFocused = false;
  filteredCities: string[] = [];

  private static toInternTitle(title: string): string | null {
    if (/\b(junior|middle|senior|staff|principal|lead|director|vp|chief|head)\b/i.test(title)) return null;
    const m = title.match(/^(.*?)\s(Developer|Engineer|Designer|Analyst|Administrator|Architect|Scientist|Specialist|Manager)$/);
    return m ? `${m[1]} Intern` : null;
  }

  // The search sends these titles as literal query keywords — "Python Developer"
  // pulls senior/generic listings, "Python Intern" actually finds internships.
  // Intern variants are generated from the stack-specific titles below and
  // placed first so they show up ahead of the generic title when searching.
  readonly JOB_TITLES: string[] = (() => {
    const base = [
    "Python Developer","Python Automation Engineer","Python Backend Developer","Python Full Stack Developer",
    "JavaScript Developer","TypeScript Developer","React Developer","React Native Developer","Angular Developer","Vue.js Developer","Next.js Developer",
    "Node.js Developer","Node.js Backend Developer","Express.js Developer",
    "Java Developer","Java Backend Developer","Java Full Stack Developer","Spring Boot Developer",
    "C# Developer",".NET Developer",".NET Backend Developer","ASP.NET Developer",
    "Go Developer","Golang Backend Developer","Rust Developer","C++ Developer",
    "iOS Developer","Swift Developer","SwiftUI Developer","Android Developer","Kotlin Developer","Flutter Developer","React Native Mobile Developer",
    "Full Stack Developer","Frontend Developer","Backend Developer","Web Developer","Software Engineer","Senior Software Engineer","Staff Software Engineer","Principal Software Engineer",
    "DevOps Engineer","Site Reliability Engineer","SRE","Cloud Engineer","Cloud Architect","AWS Engineer","Azure Engineer","GCP Engineer",
    "Data Engineer","Data Scientist","Machine Learning Engineer","ML Engineer","AI Engineer","Deep Learning Engineer","NLP Engineer","Computer Vision Engineer",
    "Database Administrator","DBA","SQL Developer","Data Analyst","Business Intelligence Analyst",
    "QA Engineer","QA Automation Engineer","Test Engineer","SDET","Quality Assurance Engineer",
    "Security Engineer","Cybersecurity Analyst","Penetration Tester","Application Security Engineer",
    "UX Designer","UI Designer","UX/UI Designer","Product Designer","Interaction Designer","Visual Designer","Graphic Designer","Motion Designer",
    "Product Manager","Technical Product Manager","Program Manager","Project Manager","Scrum Master","Agile Coach",
    "Solutions Architect","Systems Architect","Enterprise Architect","Integration Architect",
    "Technical Writer","Developer Advocate","Developer Relations Engineer","DevRel Engineer",
    "IT Support Specialist","Systems Administrator","Network Engineer","Linux Administrator",
    "Blockchain Developer","Web3 Developer","Smart Contract Developer","Solidity Developer",
    "Embedded Systems Engineer","Firmware Engineer","IoT Engineer","Hardware Engineer",
    "Research Scientist","Applied Scientist","Quantitative Analyst","Algorithm Engineer",
    "SEO Specialist","Digital Marketing Engineer","Growth Engineer","Marketing Technologist",
    "WordPress Developer","Drupal Developer","Laravel Developer","Django Developer","Ruby on Rails Developer","PHP Developer","Symfony Developer",
    "Kubernetes Engineer","Docker Engineer","Terraform Engineer","Infrastructure Engineer",
    "Monitoring Engineer","Observability Engineer","Platform Engineer","Release Engineer",
    "AI Prompt Engineer","LLM Engineer","AI Solutions Architect","Generative AI Engineer",
    "E-commerce Developer","Shopify Developer","Magento Developer","Salesforce Developer",
    "Citrix Engineer","SAP Developer","ServiceNow Developer","Atlassian Administrator",
    "Technical Lead","Engineering Manager","VP of Engineering","CTO","Chief Architect",
    "Junior Developer","Middle Developer","Senior Developer","Lead Developer","Staff Developer","Principal Developer",
    "Intern Software Developer","Graduate Software Engineer","Trainee Developer",
    ];
    const seen = new Set<string>();
    const internVariants: string[] = [];
    for (const t of base) {
      const intern = StudioComponent.toInternTitle(t);
      if (intern && !seen.has(intern.toLowerCase())) {
        seen.add(intern.toLowerCase());
        internVariants.push(intern);
      }
    }
    return [...internVariants, ...base];
  })();

  readonly POPULAR_CITIES: string[] = [
    "Warsaw","Kraków","Wrocław","Gdańsk","Poznań","Łódź","Katowice","Lublin","Szczecin","Bydgoszcz",
    "Białystok","Gdynia","Częstochowa","Radom","Toruń","Sosnowiec","Rzeszów","Kielce","Gliwice","Zabrze",
    "Olsztyn","Bielsko-Biała","Bytom","Zielona Góra","Rybnik","Ruda Śląska","Opole","Tychy","Gorzów Wielkopolski","Elbląg",
    "Płock","Wałbrzych","Włocławek","Tarnów","Chorzów","Koszalin","Kalisz","Legnica","Grudziądz","Jaworzno",
    "Jelenia Góra","Nowy Sącz","Konin","Piotrków Trybunalski","Inowrocław","Lubin","Ostrów Wielkopolski","Gniez","Stargard","Ostrowiec Świętokrzyski",
    "Berlin","Munich","Hamburg","Frankfurt","Cologne","Stuttgart","Düsseldorf","Leipzig","Dortmund","Essen",
    "London","Manchester","Birmingham","Leeds","Glasgow","Edinburgh","Liverpool","Bristol","Sheffield","Nottingham",
    "Paris","Lyon","Marseille","Toulouse","Nice","Nantes","Strasbourg","Bordeaux","Lille","Rennes",
    "Amsterdam","Rotterdam","The Hague","Utrecht","Eindhoven","Tilburg","Groningen","Almere","Breda","Nijmegen",
    "Barcelona","Madrid","Valencia","Seville","Bilbao","Málaga","Zaragoza","Las Palmas","Palma de Mallorca","Alicante",
    "Rome","Milan","Turin","Naples","Florence","Bologna","Genoa","Catania","Verona","Padua",
    "Lisbon","Porto","Braga","Coimbra","Faro","Funchal",
    "Vienna","Graz","Linz","Salzburg","Innsbruck","Klagenfurt",
    "Zurich","Geneva","Basel","Bern","Lausanne","Lucerne",
    "Brussels","Antwerp","Ghent","Bruges","Leuven",
    "Prague","Brno","Ostrava","Plzeň","Olomouc",
    "Budapest","Debrecen","Szeged","Miskolc","Pécs",
    "Warsaw","Kraków","Remote",
    "New York","San Francisco","Los Angeles","Chicago","Seattle","Austin","Boston","Denver","Atlanta","Miami",
    "Washington DC","Dallas","Houston","Phoenix","Portland","San Diego","Minneapolis","Detroit","Nashville","Charlotte",
    "Toronto","Vancouver","Montreal","Calgary","Ottawa","Edmonton","Waterloo","Kitchener",
    "Tel Aviv","Jerusalem","Haifa","Herzliya","Ramat Gan","Petah Tikva",
    "Dubai","Abu Dhabi","Sharjah","Doha","Riyadh","Jeddah",
    "Singapore","Hong Kong","Tokyo","Seoul","Bangkok","Kuala Lumpur","Jakarta","Manila","Ho Chi Minh City","Hanoi",
    "Sydney","Melbourne","Brisbane","Perth","Adelaide","Auckland","Wellington","Christchurch",
    "Cape Town","Johannesburg","Durban","Pretoria","Nairobi","Lagos","Accra","Cairo","Alexandria",
    "Buenos Aires","Santiago","Bogotá","Medellín","Mexico City","Guadalajara","Montevideo","Lima","São Paulo","Rio de Janeiro",
  ];

  readonly regions = REGIONS;
  readonly countries = COUNTRIES;
  readonly workAuthStatus = WORK_AUTH_STATUS;

  readonly steps: Step[] = [
    {
      key: "about",
      kind: "about",
      eyebrow: "Step 1 · About you",
      title: "Let's build your professional twin",
      subtitle: "Tell us who you are and, optionally, link your work elsewhere online.",
    },
    {
      key: "employmentStatus",
      kind: "single",
      eyebrow: "About you",
      title: "Describe your current employment status",
      options: EMPLOYMENT,
    },
    {
      key: "jobTitles",
      kind: "titles",
      eyebrow: "Target role",
      title: "What's your desired job title?",
      hint: "Please enter clear and specific job titles (e.g., Senior Python Developer, UX/UI Designer). Avoid generic or unclear titles like Student or Developer. Specific titles help us match you with the best opportunities.",
    },
    {
      key: "experienceLevel",
      kind: "single",
      eyebrow: "Experience",
      title: "What's your target experience level in this role?",
      options: EXPERIENCE,
    },
    {
      key: "salary",
      kind: "number",
      eyebrow: "Compensation",
      title: "What are your salary expectations?",
      hint: "We use this information to assess job compatibility and help complete applications. It is not used as a strict filter, as many roles do not list salary ranges and compensation is often negotiable. If an application asks for your expected salary, we may use this amount.",
    },
    {
      key: "industries",
      kind: "multi",
      eyebrow: "Industries",
      title: "Select industry preference",
      subtitle: "Choose industries you prefer to work in.",
      options: INDUSTRIES,
    },
    {
      key: "workTypes",
      kind: "multi",
      eyebrow: "Work type",
      title: "What type of work are you open to?",
      options: WORK_TYPES,
    },
    {
      key: "startDate",
      kind: "date",
      eyebrow: "Availability",
      title: "When are you available to start?",
      subtitle: "We'll use this to match you with roles that fit your timeline.",
    },
    {
      key: "willingToRelocate",
      kind: "boolean",
      eyebrow: "Relocation",
      title: "Are you willing to relocate for a job?",
    },
    {
      key: "workMode",
      kind: "multi",
      eyebrow: "Work style",
      title: "How would you like to work?",
      subtitle: "You can pick both. Next, you'll choose locations for each.",
      options: WORK_MODE,
    },
    {
      key: "remoteLocations",
      kind: "countries",
      eyebrow: "Remote",
      title: "Where would you work remotely?",
      subtitle: "Pick countries or regions. We'll find remote jobs available there.",
      showIf: (s) => s.preferences.workMode.includes("remote"),
    },
    {
      key: "onsiteCities",
      kind: "cities",
      eyebrow: "On-site",
      title: "Where would you work on-site?",
      subtitle: "The more cities you add, the more jobs we can find. We also search nearby areas around each city, so you won't miss anything close by.",
      hint: "Type a city name and press Enter.",
      showIf: (s) => s.preferences.workMode.includes("onsite"),
    },
    {
      key: "workAuth",
      kind: "workauth",
      eyebrow: "Work authorization",
      title: "What's your work authorization status?",
      subtitle: "Select your status for each country you're targeting. This helps us write accurate cover letters and application answers.",
    },
    {
      key: "driversLicense",
      kind: "single",
      eyebrow: "Details",
      title: "Do you have a current driver's license?",
      options: YNM,
    },
    {
      key: "gender",
      kind: "single",
      eyebrow: "Details",
      title: "How do you identify?",
      options: [
        { value: "male", title: "Male" },
        { value: "female", title: "Female" },
        { value: "non_binary", title: "Non-binary or prefer to self-describe" },
        { value: "self_describe", title: "I prefer to self-describe" },
        { value: "prefer_not", title: "Prefer not to say" },
      ],
    },
    {
      key: "disability",
      kind: "single",
      eyebrow: "Details",
      title: "Do you have a disability or chronic condition that limits your major life activities?",
      subtitle: "(Disabilities can be physical, visual, auditory, cognitive, mental, or emotional.)",
      options: YNM,
    },
    {
      key: "veteran",
      kind: "single",
      eyebrow: "Details",
      title: "Are you a protected veteran?",
      options: YNM,
    },
    {
      key: "contactEmail",
      kind: "email-consent",
      eyebrow: "Almost done",
      title: "Where should employers reach you?",
      subtitle: "Drop your email so we can send your tailored results, and so hiring managers can reply straight to you.",
    },
    {
      key: "loader",
      kind: "loader",
      title: "Spend less time searching, more time preparing",
      subtitle: "We analyze your data, add in your preferences, and gather information from job boards and applicant tracking system (ATS) platforms.",
    },
  ];

  ngOnInit(): void {
    const saved = this.studio.load();
    if (saved && saved.completedAt) {
      this.state = saved;
      this.mode = "summary";
    } else if (saved) {
      this.state = saved;
    }
    this.titlesText = this.state.preferences.jobTitles.join(", ");
    if (!this.state.preferences.startDate) {
      this.state.preferences.startDate = new Date().toISOString().slice(0, 10);
    }

    // If there is nothing locally (new device / cleared storage), restore the
    // saved profile from the backend (DB) so the user never has to re-enter it.
    if (!saved) {
      this.studio.loadFromBackend().subscribe({
        next: () => {
          const restored = this.studio.load();
          if (restored) {
            this.state = restored;
            this.mode = restored.completedAt ? "summary" : "wizard";
            this.titlesText = this.state.preferences.jobTitles.join(", ");
            if (!this.state.preferences.startDate) {
              this.state.preferences.startDate = new Date().toISOString().slice(0, 10);
            }
          }
        },
        error: () => {},
      });
    }
  }

  get visibleSteps(): Step[] {
    return this.steps.filter((s) => !s.showIf || s.showIf(this.state));
  }

  get current(): Step {
    return this.visibleSteps[this.stepIndex] ?? this.steps[0];
  }

  get progressPercent(): number {
    return Math.round(((this.stepIndex) / this.visibleSteps.length) * 100);
  }

  isLast(): boolean {
    return this.stepIndex >= this.visibleSteps.length - 1;
  }

  canProceed(): boolean {
    const k = this.current.key;
    const p = this.state.preferences;
    switch (this.current.kind) {
      case "about": {
        const gh = this.state.github.trim();
        this.githubError = "";
        let ok = true;
        if (gh && !/^https?:\/\/(www\.)?github\.com\/.+/.test(gh)) {
          this.githubError = "Enter a valid GitHub URL (e.g. https://github.com/username)";
          ok = false;
        }
        return ok;
      }
      case "single":
        return !!(this as any).state.preferences[k] && (this as any).state.preferences[k] !== "";
      case "multi":
        return (this as any).state.preferences[k]?.length > 0;
      case "titles":
        return this.state.preferences.jobTitles.length > 0 && !this.titlesWarning;
      case "countries":
      case "cities":
        return (this as any).state.preferences[k]?.length > 0;
      case "boolean":
        return (this as any).state.preferences[k] !== null && (this as any).state.preferences[k] !== undefined;
      case "email-consent":
        return this.validEmail(p.contactEmail);
      case "date":
      case "number":
      case "workauth":
        return true;
      default:
        return true;
    }
  }

  validEmail(v: string): boolean {
    return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(v);
  }

  getStr(key: string): string {
    return (this.state.preferences as any)[key] ?? "";
  }
  setStr(key: string, value: string): void {
    (this.state.preferences as any)[key] = value;
  }
  hasStr(key: string, value: string): boolean {
    return ((this.state.preferences as any)[key] as string[] | undefined)?.includes(value) ?? false;
  }
  toggleStr(key: string, value: string): void {
    const arr = ((this.state.preferences as any)[key] as string[]) ?? [];
    const i = arr.indexOf(value);
    if (i >= 0) arr.splice(i, 1);
    else arr.push(value);
    (this.state.preferences as any)[key] = arr;
  }
  getStrArray(key: string): string[] {
    return (this.state.preferences as any)[key] ?? [];
  }
  getBool(key: string): boolean | null {
    return (this.state.preferences as any)[key];
  }
  setBool(key: string, value: boolean): void {
    (this.state.preferences as any)[key] = value;
  }

  toggleAllWorkTypes(): void {
    if (this.state.preferences.workTypes.length === WORK_TYPES.length) {
      this.state.preferences.workTypes = [];
    } else {
      this.state.preferences.workTypes = WORK_TYPES.map((t) => t.value as any);
    }
  }

  onTitlesInput(text: string): void {
    this.titlesText = text;
    const q = text.trim().toLowerCase();
    if (q.length < 1) { this.filteredTitles = []; return; }
    const already = new Set(this.state.preferences.jobTitles.map(t => t.toLowerCase()));
    this.filteredTitles = this.JOB_TITLES
      .filter(t => t.toLowerCase().includes(q) && !already.has(t.toLowerCase()))
      .slice(0, 8);
  }

  pickTitle(title: string): void {
    if (!this.state.preferences.jobTitles.includes(title)) {
      this.state.preferences.jobTitles.push(title);
    }
    this.titlesText = "";
    this.filteredTitles = [];
    this.titlesWarning = "";
  }

  removeTitle(i: number): void {
    this.state.preferences.jobTitles.splice(i, 1);
  }

  onTitlesBlur(): void {
    setTimeout(() => { this.titlesFocused = false; }, 150);
  }

  get asapChecked(): boolean {
    return !this.state.preferences.startDate;
  }
  onAsap(v: boolean): void {
    this.state.preferences.startDate = v ? "" : new Date().toISOString().slice(0, 10);
  }

  filterCountries(): void {
    const q = this.countryQuery.trim().toLowerCase();
    this.filteredCountries = q
      ? COUNTRIES.filter((c) => c.title.toLowerCase().includes(q))
      : COUNTRIES;
  }

  addCity(): void {
    const v = this.cityQuery.trim();
    if (v && !this.state.preferences.onsiteCities.includes(v)) {
      this.state.preferences.onsiteCities.push(v);
    }
    this.cityQuery = "";
    this.filteredCities = [];
  }
  removeCity(c: string): void {
    this.state.preferences.onsiteCities = this.state.preferences.onsiteCities.filter((x) => x !== c);
  }

  onCityInput(text: string): void {
    this.cityQuery = text;
    const q = text.trim().toLowerCase();
    if (q.length < 1) { this.filteredCities = []; return; }
    const already = new Set(this.state.preferences.onsiteCities.map(c => c.toLowerCase()));
    this.filteredCities = this.POPULAR_CITIES
      .filter(c => c.toLowerCase().includes(q) && !already.has(c.toLowerCase()))
      .slice(0, 8);
  }

  pickCity(city: string): void {
    if (!this.state.preferences.onsiteCities.includes(city)) {
      this.state.preferences.onsiteCities.push(city);
    }
    this.cityQuery = "";
    this.filteredCities = [];
  }

  onCityBlur(): void {
    setTimeout(() => { this.cityFocused = false; }, 150);
  }

  addAuth(): void {
    if (this.authCountry && !this.state.preferences.workAuth.find((a) => a.country === this.authCountry)) {
      this.state.preferences.workAuth.push({ country: this.authCountry, status: "" });
    }
    this.authCountry = "";
  }
  removeAuth(i: number): void {
    this.state.preferences.workAuth.splice(i, 1);
  }

  getFlag(country: string): string {
    return FLAGS[country] || "🏳️";
  }

  labelFor(key: string, value: string): string {
    const map: Record<string, Choice[]> = {
      employmentStatus: EMPLOYMENT,
      experienceLevel: EXPERIENCE,
    };
    return (map[key]?.find((o) => o.value === value)?.title ?? value) || "—";
  }
  workAuthText(): string {
    const a = this.state.preferences.workAuth;
    return a.length ? a.map((e) => `${e.country}: ${e.status}`).join("; ") : "—";
  }
  labelsFor(key: string, values: string[]): string {
    const map: Record<string, Choice[]> = {
      industries: INDUSTRIES,
      workTypes: WORK_TYPES,
      workMode: WORK_MODE,
    };
    if (!values?.length) return "—";
    return values.map((v) => map[key]?.find((o) => o.value === v)?.title ?? v).join(", ");
  }

  next(): void {
    if (!this.canProceed()) return;
    if (this.isLast()) {
      this.finish();
      return;
    }
    this.stepIndex++;
    // The final "loader" step hides its footer, so it can never be advanced
    // via the Continue button. Kick off the gather/submit pipeline here.
    if (this.current.kind === "loader") {
      this.finish();
    }
  }

  back(): void {
    if (this.stepIndex > 0) {
      this.stepIndex--;
    }
  }

  exitToHome(): void {
    if (this.mode === "summary" || confirm("Leave the Studio? Your progress is saved on this device.")) {
      this.router.navigate(["/"]);
    }
  }

  /** Download the current profile as a JSON file (password is stripped). */
  exportProfile(): void {
    const current = this.studio.load() || this.state;
    const payload = {
      version: 1,
      exportedAt: new Date().toISOString(),
      app: "careertwin",
      state: { ...current },
    };
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "careertwin-profile.json";
    a.click();
    URL.revokeObjectURL(url);
  }

  /** Import a previously exported profile file and persist it (local + DB). */
  importProfile(event: Event): void {
    const input = event.target as HTMLInputElement;
    const file = input.files && input.files[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      try {
        const parsed = JSON.parse(reader.result as string);
        const src = parsed && parsed.state ? parsed.state : parsed;
        if (!src || typeof src !== "object") throw new Error("bad shape");
        const merged: StudioState = { ...this.studio.emptyState(), ...src };
        if (!merged.preferences.startDate) {
          merged.preferences.startDate = new Date().toISOString().slice(0, 10);
        }
        this.state = merged;
        this.studio.save(merged);
        this.importError = null;
        this.studio.submit(merged).subscribe({
          next: () => {
            this.mode = merged.completedAt ? "summary" : "wizard";
            this.stepIndex = 0;
            this.titlesText = this.state.preferences.jobTitles.join(", ");
          },
          error: () => {
            this.importError = "Profile imported locally, but saving to the server failed. It is still available on this device.";
          },
        });
      } catch {
        this.importError = "Invalid CareerTwin profile file.";
      } finally {
        input.value = "";
      }
    };
    reader.readAsText(file);
  }

  retake(): void {
    this.mode = "wizard";
    this.stepIndex = 0;
  }

  initials(): string {
    const n = this.state.profile.fullName.trim();
    if (!n) return "CT";
    const p = n.split(/\s+/);
    return (p[0][0] + (p[1]?.[0] ?? "")).toUpperCase();
  }
  summaryTitles(): string {
    return this.state.preferences.jobTitles.join(", ") || "Your career twin";
  }

  finish(): void {
    if (this.submitting) return;
    this.submitting = true;
    this.stepIndex = this.visibleSteps.length - 1;
    this.progress = 0;

    // Connect to SSE log stream
    this.connectScanSse();

    // Merge firstName + lastName into profile.fullName
    this.state.profile.fullName = resolveProfileName(
      this.state.profile.fullName,
      this.state.firstName,
      this.state.lastName,
    );

    this.gatherLog = [
      { text: "Analyzing your profile and preferences", done: false },
      { text: "Saving your preferences", done: false },
    ];

    this.gatherLog[0].done = true;
    this.progress = 60;
    this.gatherLog[1].done = true;
    this.progress = 100;
    this.complete();
  }

  private complete(): void {
    this.studio.save(this.state);
    this.studio.submit(this.state).subscribe();
    this.disconnectScanSse();
    setTimeout(() => {
      this.submitting = false;
      this.router.navigate(["/dashboard"]);
    }, 900);
  }

  private connectScanSse(): void {
    this.scanConnecting = true;
    this.scanLogs = [];
    this.scanSse = new EventSource("/api/v1/jobs/scan/logs");
    this.scanSse.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        if (data.msg) {
          const d = new Date(data.ts * 1000);
          const time = d.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
          this.scanLogs.push({ msg: data.msg, level: data.level || "info", time });
          // Auto-scroll to bottom
          setTimeout(() => {
            const el = this.consoleBody?.nativeElement;
            if (el) el.scrollTop = el.scrollHeight;
          }, 50);
        }
      } catch {}
    };
    this.scanSse.onerror = () => {
      this.scanConnecting = false;
    };
    this.scanSse.onopen = () => {
      this.scanConnecting = false;
    };
  }

  private disconnectScanSse(): void {
    if (this.scanSse) {
      this.scanSse.close();
      this.scanSse = null;
    }
  }
}
