import { AfterViewInit, Component, inject, OnInit } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";
import gsap from "gsap";
import {
  ProfileService,
  UserProfile,
} from "../../services/profile.service";
import { resolveProfileName } from "../../utils/profile-name";

@Component({
  selector: "ct-profile",
  standalone: true,
  imports: [CommonModule, FormsModule],
  template: `
    <div class="profile page-shell">
      <section class="profile__hero surface">
        <div class="eyebrow">Profile</div>
        <h1 class="section-title">Your career, in one place.</h1>
        <p class="hero-sub">
          Upload your resume to fill this in automatically, or add it
          yourself. Everything here is reused for every CV and cover letter.
        </p>
      </section>

      <section class="profile-card">
        <div class="profile-card__top">
          <div class="avatar">{{ initials() }}</div>
          <div class="profile-card__id">
            <input
              class="name-input"
              [(ngModel)]="profile.fullName"
              [disabled]="!editing"
              placeholder="Your name"
            />
            <input
              class="title-input"
              [(ngModel)]="profile.title"
              [disabled]="!editing"
              placeholder="e.g. Senior Backend Engineer"
            />
            <div class="profile-card__meta" *ngIf="profile.location || profile.email">
              <span *ngIf="profile.location">{{ profile.location }}</span>
              <span *ngIf="profile.email">{{ profile.email }}</span>
            </div>
          </div>
          <div class="profile-card__actions">
            <button class="btn-resume" type="button" (click)="openResumeUpload()">
              <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor">
                <path d="M14 2H6c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V8l-6-6zm-1 2l5 5h-5V4zM6 20V4h5v7h7v9H6z"/>
                <path d="M8 15h8v2H8zm0-3h8v2H8z"/>
              </svg>
              Upload Resume
            </button>
            <button *ngIf="!editing" class="edit-btn" type="button" (click)="startEdit()">Edit</button>
            <button *ngIf="editing" class="edit-btn edit-btn--ghost" type="button" (click)="cancelEdit()">Cancel</button>
            <button *ngIf="editing" class="save-btn" type="button" (click)="save()" [disabled]="saving">
              {{ saving ? "Saving..." : "Save" }}
            </button>
          </div>
        </div>
      </section>

      <div class="grid-2">
        <section class="panel-card">
          <h3>Summary</h3>
          <textarea
            *ngIf="editing"
            [(ngModel)]="profile.summary"
            rows="6"
            placeholder="Describe your stack, experience and the kind of roles you want."
          ></textarea>
          <p *ngIf="!editing" class="plain">{{ profile.summary || "No summary yet." }}</p>
        </section>

        <section class="panel-card">
          <h3>Links</h3>
          <div class="field" *ngFor="let link of linkFields">
            <span>{{ link.label }}</span>
            <input [(ngModel)]="profile[link.key]" [disabled]="!editing" [placeholder]="link.placeholder" />
          </div>
        </section>

        <section class="panel-card panel-card--wide">
          <div class="panel-head">
            <h3>Experience</h3>
            <button *ngIf="editing" class="add-btn" type="button" (click)="addExperience()">+ Add</button>
          </div>
          <div class="item" *ngFor="let item of profile.experience; let i = index">
            <div class="item__row">
              <input class="item__title" [(ngModel)]="item.role" [disabled]="!editing" placeholder="Role" />
              <input class="item__sub" [(ngModel)]="item.company" [disabled]="!editing" placeholder="Company" />
              <button *ngIf="editing" class="remove-btn" type="button" (click)="removeExperience(i)">×</button>
            </div>
            <div class="item__dates" *ngIf="editing">
              <input [(ngModel)]="item.start" placeholder="Start" />
              <input [(ngModel)]="item.end" placeholder="End" />
              <label class="check"><input type="checkbox" [(ngModel)]="item.current" [disabled]="!editing" /> Current</label>
            </div>
            <input class="item__desc" [(ngModel)]="item.description" [disabled]="!editing" placeholder="What you did" />
          </div>
          <p *ngIf="!profile.experience.length" class="empty">No experience added yet.</p>
        </section>

        <section class="panel-card">
          <div class="panel-head">
            <h3>Education</h3>
            <button *ngIf="editing" class="add-btn" type="button" (click)="addEducation()">+ Add</button>
          </div>
          <div class="item" *ngFor="let item of profile.education; let i = index">
            <div class="item__row">
              <input class="item__title" [(ngModel)]="item.school" [disabled]="!editing" placeholder="School" />
              <button *ngIf="editing" class="remove-btn" type="button" (click)="removeEducation(i)">×</button>
            </div>
            <input class="item__sub" [(ngModel)]="item.degree" [disabled]="!editing" placeholder="Degree" />
            <input class="item__desc" [(ngModel)]="item.field" [disabled]="!editing" placeholder="Field of study" />
            <div class="item__dates" *ngIf="editing">
              <input [(ngModel)]="item.start" placeholder="Start" />
              <input [(ngModel)]="item.end" placeholder="End" />
            </div>
          </div>
          <p *ngIf="!profile.education.length" class="empty">No education added yet.</p>
        </section>

        <section class="panel-card">
          <div class="panel-head">
            <h3>Certificates</h3>
            <button *ngIf="editing" class="add-btn" type="button" (click)="addCertificate()">+ Add</button>
          </div>
          <div class="item" *ngFor="let item of profile.certificates; let i = index">
            <div class="item__row">
              <input class="item__title" [(ngModel)]="item.name" [disabled]="!editing" placeholder="Certificate name" />
              <button *ngIf="editing" class="remove-btn" type="button" (click)="removeCertificate(i)">×</button>
            </div>
            <input class="item__sub" [(ngModel)]="item.issuer" [disabled]="!editing" placeholder="Issuer" />
            <div class="item__dates" *ngIf="editing">
              <input [(ngModel)]="item.year" placeholder="Year" />
              <input [(ngModel)]="item.url" placeholder="Credential URL" />
            </div>
          </div>
          <p *ngIf="!profile.certificates.length" class="empty">No certificates added yet.</p>
        </section>

        <section class="panel-card panel-card--wide">
          <div class="panel-head">
            <h3>Skills</h3>
          </div>
          <div class="chip-row">
            <span class="chip" *ngFor="let s of profile.skills; let i = index">
              {{ s }}
              <button *ngIf="editing" class="chip-remove" type="button" (click)="removeSkill(i)">×</button>
            </span>
          </div>
          <div class="skill-add" *ngIf="editing">
            <input
              [(ngModel)]="skillInput"
              (keyup.enter)="addSkill()"
              placeholder="Type a skill and press Enter"
            />
          </div>
          <p *ngIf="!profile.skills.length" class="empty">No skills added yet.</p>
        </section>

        <section class="panel-card panel-card--wide">
          <div class="panel-head">
            <h3>Languages</h3>
          </div>
          <div class="chip-row">
            <span class="chip" *ngFor="let l of profile.languages; let i = index">
              {{ l }}
              <button *ngIf="editing" class="chip-remove" type="button" (click)="removeLanguage(i)">×</button>
            </span>
          </div>
          <div class="language-add" *ngIf="editing">
            <input
              [(ngModel)]="languageNameInput"
              list="language-options"
              (keyup.enter)="addLanguage()"
              placeholder="Search a language, e.g. Russian"
            />
            <datalist id="language-options">
              <option *ngFor="let lang of commonLanguages" [value]="lang"></option>
            </datalist>
            <select [(ngModel)]="languageLevelInput">
              <option *ngFor="let lvl of languageLevels" [value]="lvl">{{ lvl }}</option>
            </select>
            <button class="add-btn" type="button" (click)="addLanguage()">+ Add</button>
          </div>
          <p *ngIf="!profile.languages.length" class="empty">No languages added yet.</p>
        </section>
      </div>
    </div>

    <div class="import-modal" *ngIf="resumeUploadOpen" (click)="resumeUploadOpen = false">
      <div class="import-card" (click)="$event.stopPropagation()">
        <h3>Upload Resume (PDF/DOCX/TXT)</h3>
        <p class="import-note">
          Upload your resume and we'll extract your skills, experience, and education
          to auto-fill your profile and improve job match scoring.
        </p>
        <div class="resume-dropzone"
             (dragover)="$event.preventDefault(); dragOver = true"
             (dragleave)="dragOver = false"
             (drop)="$event.preventDefault(); dragOver = false; onFileDrop($event)"
             [class.drag-over]="dragOver">
          <svg viewBox="0 0 24 24" width="40" height="40" fill="none" stroke="currentColor" stroke-width="1.5">
            <path d="M14 2H6c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V8l-6-6z"/>
            <path d="M14 2v6h6"/>
            <path d="M12 18v-6"/>
            <path d="M9 15l3-3 3 3"/>
          </svg>
          <p>Drag & drop your resume here</p>
          <p class="dropzone-hint">or</p>
          <label class="btn-browse">
            Browse files
            <input type="file" accept=".pdf,.docx,.doc,.txt" (change)="onFileSelect($event)" hidden />
          </label>
        </div>
        <p class="import-status error" *ngIf="resumeError">{{ resumeError }}</p>
        <p class="import-status success" *ngIf="resumeStatus">{{ resumeStatus }}</p>
        <div class="import-actions">
          <button class="edit-btn edit-btn--ghost" type="button" (click)="resumeUploadOpen = false">Close</button>
        </div>
      </div>
    </div>
  `,
  styles: [`
    .profile { display: grid; gap: 1.25rem; padding-bottom: 4rem; }

    .profile__hero {
      padding: 1.5rem;
      background: radial-gradient(circle at top right, rgba(209, 161, 87, 0.18), transparent 28%),
                  linear-gradient(180deg, rgba(28, 18, 12, 0.96), rgba(18, 11, 7, 0.98));
      border: 1px solid var(--color-line);
      border-radius: 2rem;
    }
    .hero-sub { color: var(--color-muted); max-width: 56ch; margin-top: 0.5rem; line-height: 1.7; }

    .profile-card {
      border-radius: 1.8rem;
      border: 1px solid rgba(244, 229, 205, 0.12);
      background: rgba(255, 255, 255, 0.035);
      padding: 1.5rem;
    }
    .profile-card__top { display: flex; gap: 1.25rem; align-items: center; flex-wrap: wrap; }
    .avatar {
      width: 5rem; height: 5rem; border-radius: 1.4rem;
      display: grid; place-items: center; flex-shrink: 0;
      font-family: "Space Grotesk", sans-serif; font-size: 1.8rem; font-weight: 600;
      color: #120b07;
      background: linear-gradient(135deg, #f5df90, #d1a157);
    }
    .profile-card__id { flex: 1; min-width: 14rem; }
    .name-input {
      width: 100%; background: transparent; border: none; outline: none;
      font-family: "Space Grotesk", sans-serif; font-size: 1.5rem; font-weight: 600;
      color: var(--color-foreground); padding: 0.1rem 0;
    }
    .name-input:disabled { opacity: 0.95; }
    .title-input {
      width: 100%; background: transparent; border: none; outline: none;
      color: #d1a157; font-size: 0.95rem; padding: 0.1rem 0;
    }
    .title-input:disabled { opacity: 0.9; }
    .profile-card__meta { display: flex; gap: 0.75rem; flex-wrap: wrap; margin-top: 0.35rem; color: var(--color-muted); font-size: 0.85rem; }
    .profile-card__meta span { position: relative; }
    .profile-card__actions { display: flex; gap: 0.5rem; flex-wrap: wrap; }

    .btn-resume {
      display: inline-flex; align-items: center; gap: 0.5rem;
      border: 1px solid rgba(209, 161, 87, 0.4);
      background: rgba(209, 161, 87, 0.12);
      color: #ecd4ab; border-radius: 999px; padding: 0.6rem 1.1rem;
      font-weight: 600; cursor: pointer; transition: background 160ms ease;
    }
    .btn-resume:hover { background: rgba(209, 161, 87, 0.2); }

    .resume-dropzone {
      border: 2px dashed rgba(236, 212, 171, 0.3);
      border-radius: 1rem; padding: 2rem 1.5rem; text-align: center;
      color: var(--color-muted); cursor: pointer; transition: all 200ms ease;
      margin: 1rem 0;
    }
    .resume-dropzone:hover, .resume-dropzone.drag-over {
      border-color: #ecd4ab; background: rgba(236, 212, 171, 0.06);
      color: #ecd4ab;
    }
    .resume-dropzone svg { margin-bottom: 0.75rem; opacity: 0.6; }
    .resume-dropzone p { margin: 0.25rem 0; }
    .dropzone-hint { font-size: 0.85rem; opacity: 0.6; }
    .btn-browse {
      display: inline-block; margin-top: 0.75rem;
      border: 1px solid rgba(236, 212, 171, 0.3); border-radius: 999px;
      padding: 0.5rem 1.2rem; font-weight: 600; cursor: pointer;
      color: #ecd4ab; background: rgba(236, 212, 171, 0.1);
      transition: background 160ms ease;
    }
    .btn-browse:hover { background: rgba(236, 212, 171, 0.2); }
    .import-status.error { color: #e57373; }
    .import-status.success { color: #81c784; }
    .edit-btn {
      border: 1px solid rgba(236, 212, 171, 0.25); border-radius: 999px;
      background: rgba(236, 212, 171, 0.1); color: #f5df90;
      padding: 0.6rem 1.2rem; font-weight: 600; cursor: pointer;
    }
    .edit-btn:hover { background: rgba(236, 212, 171, 0.18); }
    .edit-btn--ghost { background: rgba(255,255,255,0.04); color: var(--color-foreground); }
    .save-btn {
      border: none; border-radius: 999px; background: #ecd4ab; color: #120b07;
      padding: 0.6rem 1.3rem; font-weight: 700; cursor: pointer;
    }
    .save-btn:disabled { opacity: 0.5; cursor: not-allowed; }
    .imported-flag {
      display: inline-block; margin-top: 1rem; font-size: 0.75rem;
      text-transform: uppercase; letter-spacing: 0.12em;
      color: #5aa9e6; border: 1px solid rgba(36,130,196,0.3);
      border-radius: 999px; padding: 0.35rem 0.7rem;
    }
    .browser-session {
      margin-top: 1rem;
      border-radius: 1.2rem;
      border: 1px solid rgba(90, 169, 230, 0.22);
      background: rgba(36, 130, 196, 0.08);
      padding: 1rem;
      display: grid;
      grid-template-columns: repeat(3, minmax(0, 1fr));
      gap: 0.85rem;
    }
    .browser-session p {
      grid-column: 1 / -1;
      margin: 0;
      color: rgba(244, 229, 205, 0.82);
      line-height: 1.6;
    }
    .browser-session__label {
      display: block;
      font-size: 0.72rem;
      text-transform: uppercase;
      letter-spacing: 0.12em;
      color: rgba(244, 229, 205, 0.56);
      margin-bottom: 0.35rem;
    }

    .grid-2 { display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; }
    .panel-card {
      border-radius: 1.5rem; border: 1px solid rgba(244, 229, 205, 0.1);
      background: rgba(255, 255, 255, 0.03); padding: 1.25rem;
    }
    .panel-card--wide { grid-column: 1 / -1; }
    .panel-card h3 {
      margin: 0 0 0.9rem; font-size: 0.82rem; text-transform: uppercase;
      letter-spacing: 0.14em; color: #f5df90;
    }
    .panel-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 0.9rem; }
    .panel-head h3 { margin: 0; }
    .add-btn {
      border: 1px solid rgba(236, 212, 171, 0.25); border-radius: 999px;
      background: rgba(236, 212, 171, 0.08); color: #f5df90;
      padding: 0.4rem 0.8rem; font-size: 0.8rem; cursor: pointer;
    }
    .plain, .empty { color: var(--color-muted); line-height: 1.6; margin: 0; font-style: italic; }
    .empty { font-style: italic; }

    .field { display: grid; gap: 0.35rem; margin-bottom: 0.7rem; }
    .field span { font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.1em; color: var(--color-muted); }
    .field input, .panel-card input, .panel-card select, .panel-card textarea {
      width: 100%; min-width: 0; border: 1px solid rgba(244, 229, 205, 0.12);
      border-radius: 0.9rem; background: rgba(255, 255, 255, 0.04);
      color: var(--color-foreground); padding: 0.7rem 0.85rem; outline: none; font: inherit;
    }
    .field input:disabled, .panel-card input:disabled, .panel-card textarea:disabled {
      opacity: 0.7;
    }
    .panel-card textarea { resize: vertical; min-height: 5.5rem; }

    .item { border: 1px solid rgba(244, 229, 205, 0.08); border-radius: 1rem; padding: 0.85rem; margin-bottom: 0.75rem; background: rgba(255,255,255,0.02); }
    .item__row { display: flex; gap: 0.6rem; align-items: center; }
    .item__title { flex: 1.4; font-weight: 600; }
    .item__sub { flex: 1; }
    .item__desc { margin-top: 0.5rem; }
    .item__dates { display: flex; gap: 0.6rem; margin-top: 0.5rem; flex-wrap: wrap; }
    .item__dates input { flex: 1; min-width: 6rem; }
    .check { display: inline-flex; align-items: center; gap: 0.35rem; color: var(--color-muted); font-size: 0.85rem; white-space: nowrap; }
    .remove-btn {
      background: none; border: none; color: #f87171; font-size: 1.3rem;
      cursor: pointer; line-height: 1; padding: 0 0.3rem;
    }

    .chip-row { display: flex; flex-wrap: wrap; gap: 0.4rem; }
    .chip {
      display: inline-flex; align-items: center; gap: 0.3rem;
      padding: 0.4rem 0.65rem; border-radius: 999px;
      background: rgba(236, 212, 171, 0.1); border: 1px solid rgba(236, 212, 171, 0.16);
      color: #f5df90; font-size: 0.85rem;
    }
    .chip-remove { background: none; border: none; color: #f87171; cursor: pointer; font-size: 1rem; padding: 0; line-height: 1; }
    .skill-add { margin-top: 0.7rem; }
    .language-add { margin-top: 0.7rem; display: flex; gap: 0.6rem; flex-wrap: wrap; }
    .language-add input { flex: 2; min-width: 10rem; }
    .language-add select { flex: 1; min-width: 8rem; }
    .language-add .add-btn { flex-shrink: 0; }

    .import-modal {
      position: fixed; inset: 0; z-index: 100; display: grid; place-items: center;
      background: rgba(8, 5, 3, 0.7); backdrop-filter: blur(6px); padding: 1rem;
    }
    .import-card {
      width: min(34rem, 100%); border-radius: 1.6rem; border: 1px solid rgba(244,229,205,0.15);
      background: #15100a; padding: 1.5rem; box-shadow: 0 30px 80px rgba(10,6,3,0.5);
    }
    .import-card h3 { margin: 0 0 0.5rem; font-size: 1.2rem; }
    .import-note { color: var(--color-muted); line-height: 1.6; margin: 0 0 1rem; font-size: 0.9rem; }
    .import-actions { display: flex; gap: 0.6rem; justify-content: flex-end; margin-top: 1rem; }
    .import-status { margin-top: 0.8rem; color: #f5df90; font-size: 0.9rem; }
    .import-screenshots { margin-top: 1rem; }
    .screenshots-label { font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.1em; color: var(--color-muted); margin: 0 0 0.5rem; }
    .screenshots-grid { display: flex; gap: 0.5rem; overflow-x: auto; padding-bottom: 0.5rem; }
    .screenshot-thumb {
      width: 120px; height: auto; border-radius: 0.6rem; border: 1px solid rgba(244,229,205,0.15);
      cursor: pointer; flex-shrink: 0; transition: border-color 150ms;
    }
    .screenshot-thumb:hover { border-color: #f5df90; }
    .screenshot-overlay {
      position: fixed; inset: 0; z-index: 200; display: grid; place-items: center;
      background: rgba(0,0,0,0.85); cursor: pointer; padding: 2rem;
    }
    .screenshot-overlay img { max-width: 90vw; max-height: 85vh; border-radius: 1rem; box-shadow: 0 20px 60px rgba(10,6,3,0.6); }

    @media (max-width: 860px) {
      .grid-2 { grid-template-columns: 1fr; }
      .browser-session { grid-template-columns: 1fr; }
    }
  `],
})
export class ProfileComponent implements AfterViewInit, OnInit {
  private readonly profileService = inject(ProfileService);

  profile: UserProfile = this.profileService.emptyProfile();
  editing = false;
  saving = false;
  savedBackup: UserProfile | null = null;

  skillInput = "";
  languageNameInput = "";
  languageLevelInput = "Fluent";
  readonly languageLevels = ["Native", "Fluent", "Intermediate", "Beginner"];
  readonly commonLanguages = [
    "English", "Russian", "Polish", "Ukrainian", "Belarusian", "German",
    "French", "Spanish", "Italian", "Portuguese", "Dutch", "Swedish",
    "Norwegian", "Czech", "Slovak", "Lithuanian", "Latvian", "Estonian",
    "Chinese", "Japanese", "Korean", "Arabic", "Hindi", "Turkish",
  ];

  resumeUploadOpen = false;
  resumeError = "";
  resumeStatus = "";
  dragOver = false;

  readonly linkFields = [
    { key: "github" as const, label: "GitHub", placeholder: "https://github.com/username" },
    { key: "linkedin" as const, label: "LinkedIn", placeholder: "https://linkedin.com/in/username" },
    { key: "website" as const, label: "Website / Portfolio", placeholder: "https://your-site.dev" },
  ];

  ngAfterViewInit(): void {
    gsap.from(".profile__hero, .profile-card, .panel-card", {
      y: 24, opacity: 0, duration: 0.55, stagger: 0.06, ease: "power3.out",
    });
  }

  ngOnInit(): void {
    this.profile = this.profileService.load();
  }

  initials(): string {
    const parts = this.profile.fullName.trim().split(/\s+/).filter(Boolean);
    if (!parts.length) return "CT";
    return (parts[0][0] + (parts[1]?.[0] ?? "")).toUpperCase();
  }

  startEdit(): void {
    this.savedBackup = JSON.parse(JSON.stringify(this.profile));
    this.editing = true;
  }

  cancelEdit(): void {
    if (this.savedBackup) this.profile = this.savedBackup;
    this.editing = false;
  }

  save(): void {
    this.saving = true;
    this.profileService.save(this.profile);
    setTimeout(() => {
      this.saving = false;
      this.editing = false;
    }, 400);
  }

  addSkill(): void {
    const value = this.skillInput.trim();
    if (value && !this.profile.skills.includes(value)) {
      this.profile.skills.push(value);
    }
    this.skillInput = "";
  }

  removeSkill(index: number): void {
    this.profile.skills.splice(index, 1);
  }

  addLanguage(): void {
    const name = this.languageNameInput.trim();
    if (!name) return;
    const value = `${name} (${this.languageLevelInput})`;
    if (!this.profile.languages.some(l => l.toLowerCase() === value.toLowerCase())) {
      this.profile.languages.push(value);
    }
    this.languageNameInput = "";
  }

  removeLanguage(index: number): void {
    this.profile.languages.splice(index, 1);
  }

  addExperience(): void {
    this.profile.experience.push({
      id: this.profileService.uid(), role: "", company: "", start: "", end: "", current: false, description: "",
    });
  }
  removeExperience(index: number): void {
    this.profile.experience.splice(index, 1);
  }

  addEducation(): void {
    this.profile.education.push({
      id: this.profileService.uid(), school: "", degree: "", field: "", start: "", end: "", description: "",
    });
  }
  removeEducation(index: number): void {
    this.profile.education.splice(index, 1);
  }

  addCertificate(): void {
    this.profile.certificates.push({
      id: this.profileService.uid(), name: "", issuer: "", year: "", url: "",
    });
  }
  removeCertificate(index: number): void {
    this.profile.certificates.splice(index, 1);
  }


  openResumeUpload(): void {
    this.resumeUploadOpen = true;
    this.resumeError = "";
    this.resumeStatus = "";
  }

  onFileSelect(event: Event): void {
    const input = event.target as HTMLInputElement;
    if (input.files?.length) {
      this.uploadResumeFile(input.files[0]);
    }
  }

  onFileDrop(event: DragEvent): void {
    const file = event.dataTransfer?.files?.[0];
    if (file) {
      this.uploadResumeFile(file);
    }
  }

  private uploadResumeFile(file: File): void {
    const ext = file.name.split(".").pop()?.toLowerCase();
    if (!["pdf", "docx", "doc", "txt"].includes(ext || "")) {
      this.resumeError = "Unsupported file type. Please upload PDF, DOCX, or TXT.";
      return;
    }

    this.resumeError = "";
    this.resumeStatus = `Uploading ${file.name}...`;

    this.profileService.uploadResume(file).subscribe({
      next: (response) => {
        if (response?.success && response?.profile) {
          const p = response.profile;
          const provider = response.provider || "unknown";
          const isFallback = provider === "regex-fallback";

          // Merge basic fields — only fill empty ones
          this.profile.fullName = resolveProfileName(this.profile.fullName) || resolveProfileName(p.fullName) || this.profile.fullName;
          this.profile.email ||= p.email;
          this.profile.phone ||= p.phone;
          this.profile.location ||= p.location;
          this.profile.summary ||= p.summary;

          // Merge skills — add new, don't duplicate
          for (const s of (p.skills || [])) {
            if (!this.profile.skills.some(existing => existing.toLowerCase() === s.toLowerCase())) {
              this.profile.skills.push(s);
            }
          }

          // Merge languages — add new, don't duplicate
          for (const l of (p.languages || [])) {
            if (!this.profile.languages.some(existing => existing.toLowerCase() === l.toLowerCase())) {
              this.profile.languages.push(l);
            }
          }

          // Merge experience — only add if not already present (by role+company)
          for (const exp of (p.experience || [])) {
            const role = (exp.position || "").trim();
            const company = (exp.company || "").trim();
            if (!role && !company) continue;
            const exists = this.profile.experience.some(e =>
              e.role.toLowerCase() === role.toLowerCase() &&
              e.company.toLowerCase() === company.toLowerCase()
            );
            if (!exists && role) {
              this.profile.experience.push({
                id: this.profileService.uid(),
                role,
                company,
                start: exp.start || "",
                end: exp.end || "",
                current: (exp.end || "").toLowerCase().includes("present"),
                description: (exp.description || "").substring(0, 500),
              });
            }
          }

          // Merge education — only add if not already present (by school)
          for (const edu of (p.education || [])) {
            const school = (edu.school || "").trim();
            if (!school) continue;
            const exists = this.profile.education.some(e =>
              e.school.toLowerCase() === school.toLowerCase()
            );
            if (!exists) {
              this.profile.education.push({
                id: this.profileService.uid(),
                school,
                degree: edu.degree || "",
                field: edu.field || "",
                start: "",
                end: "",
                description: "",
              });
            }
          }

          // Merge certificates — only add if not already present (by name)
          for (const cert of (p.certificates || [])) {
            const name = (cert.name || "").trim();
            if (!name) continue;
            const exists = this.profile.certificates.some(c =>
              c.name.toLowerCase() === name.toLowerCase()
            );
            if (!exists) {
              this.profile.certificates.push({
                id: this.profileService.uid(),
                name,
                issuer: cert.issuer || "",
                year: "",
                url: "",
              });
            }
          }

          this.profileService.save(this.profile);

          // Build status message with provider info
          let msg = response.message || "Resume imported successfully.";
          if (isFallback) {
            msg += "\n\nAI extraction unavailable. For best results with any CV format, set GROQ_API_KEY (free at console.groq.com) in your .env file.";
          }
          this.resumeStatus = msg;
        } else {
          this.resumeError = response?.error || "Failed to parse resume.";
          this.resumeStatus = "";
        }
      },
      error: () => {
        this.resumeError = "Upload failed. Please try again.";
        this.resumeStatus = "";
      },
    });
  }
}
